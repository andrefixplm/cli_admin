//! Camada Win32 do SCM. É o único lugar com `unsafe` deste módulo. Cada função
//! abre e fecha os próprios handles (`ScHandle` fecha no `Drop`), e os buffers
//! de tamanho variável são alocados como `u64` para ficarem alinhados.

use super::models::{Passo, StartType, SvcDetail, mensagem_win32};
use crate::modules::support::{SvcInfo, SvcState, list_services};
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Foundation::{CloseHandle, BOOL, ERROR_MORE_DATA, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Services::*;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{PCWSTR, PWSTR};

/// Handle de SC_HANDLE que fecha sozinho ao sair de escopo, em qualquer caminho de erro.
pub struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

impl ScHandle {
    fn raw(&self) -> SC_HANDLE {
        self.0
    }
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsString::from(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Lê uma string terminada em NUL apontada por `p`.
unsafe fn pwstr_to_string(p: PWSTR) -> String {
    unsafe {
        if p.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while *p.0.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len))
    }
}

/// Lê uma lista REG_MULTI_SZ (strings separadas por NUL, terminada por NUL duplo).
unsafe fn multi_sz(p: PWSTR) -> Vec<String> {
    unsafe {
        let mut out = Vec::new();
        if p.is_null() {
            return out;
        }
        let mut i = 0usize;
        loop {
            let inicio = i;
            while *p.0.add(i) != 0 {
                i += 1;
            }
            if i == inicio {
                break;
            }
            out.push(String::from_utf16_lossy(std::slice::from_raw_parts(p.0.add(inicio), i - inicio)));
            i += 1;
        }
        out
    }
}

/// Código Win32 (16 bits) de um erro do windows-rs. Erros do SCM vêm como HRESULT
/// com facility FACILITY_WIN32 (7).
fn codigo_win32(e: &windows::core::Error) -> u32 {
    let c = e.code().0 as u32;
    if (c >> 16) & 0x1FFF == 7 { c & 0xFFFF } else { c }
}

fn erro(acao: &str, e: &windows::core::Error) -> String {
    format!("{}: {}", acao, mensagem_win32(codigo_win32(e)))
}

/// Indica se o processo atual roda elevado (Administrador).
pub fn e_elevado() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut el = TOKEN_ELEVATION::default();
        let mut tam = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut el as *mut TOKEN_ELEVATION as *mut core::ffi::c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut tam,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && el.TokenIsElevated != 0
    }
}

fn abrir_scm(acesso: u32) -> Result<ScHandle, String> {
    unsafe {
        OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), acesso)
            .map(ScHandle)
            .map_err(|e| erro("OpenSCManager", &e))
    }
}

fn abrir_servico(nome: &str, acesso: u32) -> Result<ScHandle, String> {
    let scm = abrir_scm(SC_MANAGER_CONNECT)?;
    let w = wide(nome);
    unsafe {
        OpenServiceW(scm.raw(), PCWSTR(w.as_ptr()), acesso)
            .map(ScHandle)
            .map_err(|e| erro("OpenService", &e))
    }
}

fn traduz_estado(st: SERVICE_STATUS_CURRENT_STATE) -> SvcState {
    if st == SERVICE_RUNNING {
        SvcState::Running
    } else if st == SERVICE_STOPPED {
        SvcState::Stopped
    } else if st == SERVICE_PAUSED {
        SvcState::Paused
    } else {
        SvcState::Other(st.0)
    }
}

/// Estado atual pelo SCM. Usado pelos laços de espera, então abre um handle curto.
pub fn estado(nome: &str) -> Result<SvcState, String> {
    let h = abrir_servico(nome, SERVICE_QUERY_STATUS)?;
    unsafe { ler_estado(h.raw()) }
}

unsafe fn ler_estado(h: SC_HANDLE) -> Result<SvcState, String> {
    unsafe {
        let mut st = SERVICE_STATUS_PROCESS::default();
        let bytes = std::slice::from_raw_parts_mut(
            &mut st as *mut SERVICE_STATUS_PROCESS as *mut u8,
            std::mem::size_of::<SERVICE_STATUS_PROCESS>(),
        );
        let mut needed = 0u32;
        QueryServiceStatusEx(h, SC_STATUS_PROCESS_INFO, Some(bytes), &mut needed)
            .map_err(|e| erro("QueryServiceStatusEx", &e))?;
        Ok(traduz_estado(st.dwCurrentState))
    }
}

/// Lê QUERY_SERVICE_CONFIGW num buffer `u64` (alinhado). As strings apontam para
/// dentro desse mesmo buffer, então ele precisa ficar vivo enquanto são usadas.
unsafe fn ler_config(h: SC_HANDLE) -> Result<Vec<u64>, String> {
    unsafe {
        let mut needed = 0u32;
        // Primeira chamada só mede o tamanho; o erro esperado é ERROR_INSUFFICIENT_BUFFER.
        let _ = QueryServiceConfigW(h, None, 0, &mut needed);
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        QueryServiceConfigW(
            h,
            Some(buf.as_mut_ptr() as *mut QUERY_SERVICE_CONFIGW),
            needed,
            &mut needed,
        )
        .map_err(|e| erro("QueryServiceConfig", &e))?;
        Ok(buf)
    }
}

/// Início atrasado é um flag separado (SERVICE_CONFIG_DELAYED_AUTO_START_INFO).
unsafe fn ler_atrasado(h: SC_HANDLE) -> bool {
    unsafe {
        let mut info = SERVICE_DELAYED_AUTO_START_INFO::default();
        let bytes = std::slice::from_raw_parts_mut(
            &mut info as *mut SERVICE_DELAYED_AUTO_START_INFO as *mut u8,
            std::mem::size_of::<SERVICE_DELAYED_AUTO_START_INFO>(),
        );
        let mut needed = 0u32;
        match QueryServiceConfig2W(h, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, Some(bytes), &mut needed) {
            Ok(()) => info.fDelayedAutostart.as_bool(),
            Err(_) => false,
        }
    }
}

fn traduz_inicio(inicio: SERVICE_START_TYPE, atrasado: bool) -> StartType {
    if inicio == SERVICE_AUTO_START {
        if atrasado { StartType::AutomaticoAtrasado } else { StartType::Automatico }
    } else if inicio == SERVICE_DEMAND_START {
        StartType::Manual
    } else if inicio == SERVICE_DISABLED {
        StartType::Desabilitado
    } else {
        StartType::Desconhecido
    }
}

/// Detalhe completo de um serviço: estado, tipo de inicialização, caminho, conta.
pub fn detalhe(nome: &str) -> Result<SvcDetail, String> {
    let h = abrir_servico(nome, SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG)?;
    unsafe {
        let estado = ler_estado(h.raw())?;
        let buf = ler_config(h.raw())?;
        let cfg = &*(buf.as_ptr() as *const QUERY_SERVICE_CONFIGW);
        let atrasado = ler_atrasado(h.raw());
        Ok(SvcDetail {
            name: nome.to_string(),
            display_name: pwstr_to_string(cfg.lpDisplayName),
            state: estado,
            start_type: traduz_inicio(cfg.dwStartType, atrasado),
            binary_path: pwstr_to_string(cfg.lpBinaryPathName),
            account: pwstr_to_string(cfg.lpServiceStartName),
        })
    }
}

/// Lista todos os serviços Win32 com detalhe. Serviço que falha ao abrir vira
/// entrada parcial em vez de derrubar a listagem inteira.
pub fn listar_detalhes() -> Result<Vec<SvcDetail>, String> {
    let base: Vec<SvcInfo> = list_services()?;
    Ok(base
        .into_iter()
        .map(|s| {
            detalhe(&s.name).unwrap_or(SvcDetail {
                name: s.name,
                display_name: s.display_name,
                state: s.state,
                start_type: StartType::Desconhecido,
                binary_path: String::new(),
                account: String::new(),
            })
        })
        .collect())
}

/// Dependências diretas (o que precisa estar de pé para este serviço subir).
/// Entradas com "+" são grupos de ordem de carga e são ignoradas.
pub fn dependencias(nome: &str) -> Result<Vec<String>, String> {
    let h = abrir_servico(nome, SERVICE_QUERY_CONFIG)?;
    unsafe {
        let buf = ler_config(h.raw())?;
        let cfg = &*(buf.as_ptr() as *const QUERY_SERVICE_CONFIGW);
        Ok(multi_sz(cfg.lpDependencies)
            .into_iter()
            .filter(|d| !d.starts_with('+'))
            .collect())
    }
}

/// Serviços ativos que dependem deste (precisam parar antes dele).
pub fn dependentes_ativos(nome: &str) -> Result<Vec<String>, String> {
    let h = abrir_servico(nome, SERVICE_ENUMERATE_DEPENDENTS)?;
    unsafe {
        let mut needed = 0u32;
        let mut count = 0u32;
        match EnumDependentServicesW(h.raw(), SERVICE_ACTIVE, None, 0, &mut needed, &mut count) {
            Ok(()) => return Ok(Vec::new()),
            Err(e) if codigo_win32(&e) == ERROR_MORE_DATA.0 => {}
            Err(e) => return Err(erro("EnumDependentServices", &e)),
        }
        let mut buf = vec![0u64; (needed as usize).div_ceil(8).max(1)];
        EnumDependentServicesW(
            h.raw(),
            SERVICE_ACTIVE,
            Some(buf.as_mut_ptr() as *mut ENUM_SERVICE_STATUSW),
            needed,
            &mut needed,
            &mut count,
        )
        .map_err(|e| erro("EnumDependentServices", &e))?;

        let p = buf.as_ptr() as *const ENUM_SERVICE_STATUSW;
        Ok((0..count as usize)
            .map(|i| pwstr_to_string((*p.add(i)).lpServiceName))
            .collect())
    }
}

/// Envia um passo ao SCM. A espera pelo estado final fica com quem chama.
pub fn controlar(nome: &str, passo: Passo) -> Result<(), String> {
    match passo {
        Passo::Iniciar => {
            let h = abrir_servico(nome, SERVICE_START)?;
            unsafe { StartServiceW(h.raw(), None).map_err(|e| erro("StartService", &e)) }
        }
        Passo::Parar | Passo::Pausar | Passo::Continuar => {
            let (acesso, controle) = match passo {
                Passo::Parar => (SERVICE_STOP, SERVICE_CONTROL_STOP),
                Passo::Pausar => (SERVICE_PAUSE_CONTINUE, SERVICE_CONTROL_PAUSE),
                _ => (SERVICE_PAUSE_CONTINUE, SERVICE_CONTROL_CONTINUE),
            };
            let h = abrir_servico(nome, acesso)?;
            let mut st = SERVICE_STATUS::default();
            unsafe {
                ControlService(h.raw(), controle, &mut st).map_err(|e| erro("ControlService", &e))
            }
        }
    }
}

/// Converte o tipo do modelo para (start type do SCM, início atrasado?).
fn mapa_inicio(tipo: StartType) -> Result<(SERVICE_START_TYPE, bool), String> {
    match tipo {
        StartType::Automatico => Ok((SERVICE_AUTO_START, false)),
        StartType::AutomaticoAtrasado => Ok((SERVICE_AUTO_START, true)),
        StartType::Manual => Ok((SERVICE_DEMAND_START, false)),
        StartType::Desabilitado => Ok((SERVICE_DISABLED, false)),
        StartType::Desconhecido => Err("tipo de inicialização desconhecido".to_string()),
    }
}

unsafe fn definir_atrasado(h: SC_HANDLE, atrasado: bool) -> Result<(), String> {
    unsafe {
        let info = SERVICE_DELAYED_AUTO_START_INFO { fDelayedAutostart: BOOL::from(atrasado) };
        ChangeServiceConfig2W(
            h,
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            Some(&info as *const _ as *const core::ffi::c_void),
        )
        .map_err(|e| erro("ChangeServiceConfig2", &e))
    }
}

/// Muda só o tipo de inicialização. Os demais campos ficam com SERVICE_NO_CHANGE.
pub fn mudar_inicializacao(nome: &str, tipo: StartType) -> Result<(), String> {
    let (inicio, atrasado) = mapa_inicio(tipo)?;
    let h = abrir_servico(nome, SERVICE_CHANGE_CONFIG)?;
    unsafe {
        ChangeServiceConfigW(
            h.raw(),
            ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
            inicio,
            SERVICE_ERROR(SERVICE_NO_CHANGE),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
        .map_err(|e| erro("ChangeServiceConfig", &e))?;
        if inicio == SERVICE_AUTO_START {
            definir_atrasado(h.raw(), atrasado)?;
        }
    }
    Ok(())
}

/// Cria um serviço Win32 próprio. A conta fica como LocalSystem (padrão do SCM).
pub fn criar(nome: &str, exibicao: &str, binario: &str, tipo: StartType) -> Result<(), String> {
    let (inicio, atrasado) = mapa_inicio(tipo)?;
    let scm = abrir_scm(SC_MANAGER_CREATE_SERVICE)?;
    let n = wide(nome);
    let d = wide(exibicao);
    let b = wide(binario);
    unsafe {
        let h = CreateServiceW(
            scm.raw(),
            PCWSTR(n.as_ptr()),
            PCWSTR(d.as_ptr()),
            SERVICE_QUERY_STATUS,
            SERVICE_WIN32_OWN_PROCESS,
            inicio,
            SERVICE_ERROR_NORMAL,
            PCWSTR(b.as_ptr()),
            PCWSTR::null(),
            None,
            PCWSTR::null(),
            PCWSTR::null(),
            PCWSTR::null(),
        )
        .map(ScHandle)
        .map_err(|e| erro("CreateService", &e))?;
        if inicio == SERVICE_AUTO_START && atrasado {
            definir_atrasado(h.raw(), true)?;
        }
    }
    Ok(())
}
