use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;

/// Estado resumido de um serviço do Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SvcState {
    Running,
    Stopped,
    Paused,
    Other(u32),
}

impl SvcState {
    pub fn label(&self) -> &'static str {
        match self {
            SvcState::Running => "RUNNING",
            SvcState::Stopped => "STOPPED",
            SvcState::Paused => "PAUSED",
            SvcState::Other(_) => "OTHER",
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self, SvcState::Running)
    }
}

#[derive(Debug, Clone)]
pub struct SvcInfo {
    pub name: String,
    pub display_name: String,
    pub state: SvcState,
}

#[cfg(windows)]
fn wide_null(s: &str) -> Vec<u16> {
    OsString::from(s).encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe {
        let raw = p.as_ptr();
        let mut len = 0usize;
        while *raw.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(raw, len))
    }
}

/// Enumera serviços Win32 via SCM API (insensível a idioma do SO,
/// ao contrário do parse de `sc query`).
#[cfg(windows)]
pub fn list_services() -> Result<Vec<SvcInfo>, String> {
    use windows::Win32::Foundation::ERROR_MORE_DATA;
    use windows::Win32::System::Services::*;
    use windows::core::PCWSTR;

    unsafe {
        let scm = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE)
            .map_err(|e| format!("OpenSCManager: {}", e))?;

        let mut needed: u32 = 0;
        let mut count: u32 = 0;
        let mut resume: u32 = 0;
        // Primeira chamada só mede o tamanho. Qualquer erro além de ERROR_MORE_DATA é real.
        if let Err(e) = EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            None,
            &mut needed,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        ) {
            if (e.code().0 as u32) & 0xFFFF != ERROR_MORE_DATA.0 {
                let _ = CloseServiceHandle(scm);
                return Err(format!("EnumServicesStatusEx: {}", e));
            }
        }
        if needed == 0 {
            let _ = CloseServiceHandle(scm);
            return Ok(Vec::new());
        }

        // Buffer em u64 para o alinhamento bater com ENUM_SERVICE_STATUS_PROCESSW.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len() * 8);
        EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            Some(bytes),
            &mut needed,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        )
        .map_err(|e| {
            let _ = CloseServiceHandle(scm);
            format!("EnumServicesStatusEx: {}", e)
        })?;

        let mut out = Vec::with_capacity(count as usize);
        let row_size = std::mem::size_of::<ENUM_SERVICE_STATUS_PROCESSW>();
        for i in 0..count as usize {
            let row = &*(buf.as_ptr().cast::<u8>().add(i * row_size) as *const ENUM_SERVICE_STATUS_PROCESSW);
            let st = row.ServiceStatusProcess.dwCurrentState;
            let state = if st == SERVICE_RUNNING {
                SvcState::Running
            } else if st == SERVICE_STOPPED {
                SvcState::Stopped
            } else if st == SERVICE_PAUSED {
                SvcState::Paused
            } else {
                SvcState::Other(st.0)
            };
            out.push(SvcInfo {
                name: pwstr_to_string(row.lpServiceName),
                display_name: pwstr_to_string(row.lpDisplayName),
                state,
            });
        }
        let _ = CloseServiceHandle(scm);
        Ok(out)
    }
}

#[cfg(not(windows))]
pub fn list_services() -> Result<Vec<SvcInfo>, String> {
    Ok(Vec::new())
}

/// Busca serviços cujo nome ou display contenha `needle` (case-insensitive).
pub fn find_services(needle: &str) -> Result<Vec<SvcInfo>, String> {
    let q = needle.to_lowercase();
    Ok(list_services()?
        .into_iter()
        .filter(|s| {
            s.name.to_lowercase().contains(&q) || s.display_name.to_lowercase().contains(&q)
        })
        .collect())
}

/// Lê um valor REG_SZ/REG_EXPAND_SZ de `HKLM\<subkey>\<value>`.
/// Tenta também o nó Wow6432Node como fallback.
#[cfg(windows)]
pub fn read_hklm_string(subkey: &str, value: &str) -> Option<String> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::*;
    use windows::core::PCWSTR;

    unsafe {
        for key_path in [subkey.to_string(), format!("Wow6432Node\\{}", subkey)] {
            let wkey = wide_null(&key_path);
            let wval = wide_null(value);
            let mut hkey = HKEY::default();
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(wkey.as_ptr()),
                0,
                KEY_READ,
                &mut hkey,
            ) != ERROR_SUCCESS
            {
                continue;
            }
            let mut ty = REG_VALUE_TYPE(0);
            let mut size: u32 = 0;
            if RegQueryValueExW(
                hkey,
                PCWSTR(wval.as_ptr()),
                None,
                Some(&mut ty),
                None,
                Some(&mut size),
            ) != ERROR_SUCCESS
                || size == 0
                || size > 32768
            {
                let _ = RegCloseKey(hkey);
                continue;
            }
            let mut buf = vec![0u8; size as usize];
            let ok = RegQueryValueExW(
                hkey,
                PCWSTR(wval.as_ptr()),
                None,
                Some(&mut ty),
                Some(buf.as_mut_ptr()),
                Some(&mut size),
            ) == ERROR_SUCCESS;
            let _ = RegCloseKey(hkey);
            if ok {
                let nwords = (size as usize) / 2;
                let words: Vec<u16> = buf
                    .chunks_exact(2)
                    .take(nwords)
                    .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                    .collect();
                let s = String::from_utf16_lossy(&words);
                return Some(s.trim_matches('\0').trim().to_string());
            }
        }
        None
    }
}

#[cfg(not(windows))]
pub fn read_hklm_string(_subkey: &str, _value: &str) -> Option<String> {
    None
}

/// Enumera subchaves de `HKLM\<subkey>` (ex: versões instaladas).
#[cfg(windows)]
pub fn enum_hklm_subkeys(subkey: &str) -> Vec<String> {
    enum_subkeys_inner(false, subkey)
}

/// Enumera subchaves de `HKCU\<subkey>`.
#[cfg(windows)]
pub fn enum_hkcu_subkeys(subkey: &str) -> Vec<String> {
    enum_subkeys_inner(true, subkey)
}

#[cfg(windows)]
fn enum_subkeys_inner(current_user: bool, subkey: &str) -> Vec<String> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::*;
    use windows::core::PCWSTR;

    let mut out = Vec::new();
    unsafe {
        for key_path in [subkey.to_string(), format!("Wow6432Node\\{}", subkey)] {
            let wkey = wide_null(&key_path);
            let mut hkey = HKEY::default();
            let root = if current_user { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE };
            if RegOpenKeyExW(root, PCWSTR(wkey.as_ptr()), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS {
                continue;
            }
            let mut index = 0u32;
            loop {
                let mut name_buf = vec![0u16; 256];
                if RegEnumKeyW(hkey, index, Some(name_buf.as_mut_slice())) != ERROR_SUCCESS {
                    break;
                }
                let len = name_buf.iter().position(|&c| c == 0).unwrap_or(name_buf.len());
                out.push(String::from_utf16_lossy(&name_buf[..len]));
                index += 1;
            }
            let _ = RegCloseKey(hkey);
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(not(windows))]
pub fn enum_hklm_subkeys(_subkey: &str) -> Vec<String> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn enum_hkcu_subkeys(_subkey: &str) -> Vec<String> {
    Vec::new()
}

/// Enumera valores (nome, dado string) de `HKLM\<subkey>`.
#[cfg(windows)]
pub fn enum_hklm_values(subkey: &str) -> Vec<(String, String)> {
    enum_values_inner(false, subkey)
}

/// Enumera valores (nome, dado string) de `HKCU\<subkey>`.
#[cfg(windows)]
pub fn enum_hkcu_values(subkey: &str) -> Vec<(String, String)> {
    enum_values_inner(true, subkey)
}

#[cfg(windows)]
fn enum_values_inner(current_user: bool, subkey: &str) -> Vec<(String, String)> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::*;
    use windows::core::PCWSTR;

    let mut out = Vec::new();
    unsafe {
        for key_path in [subkey.to_string(), format!("Wow6432Node\\{}", subkey)] {
            let wkey = wide_null(&key_path);
            let mut hkey = HKEY::default();
            let root = if current_user { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE };
            if RegOpenKeyExW(root, PCWSTR(wkey.as_ptr()), 0, KEY_READ, &mut hkey) != ERROR_SUCCESS {
                continue;
            }
            let mut index = 0u32;
            loop {
                let mut name_buf = vec![0u16; 256];
                let mut name_len = 256u32;
                let mut ty: u32 = 0;
                let mut data = vec![0u8; 4096];
                let mut data_len = 4096u32;
                if RegEnumValueW(
                    hkey,
                    index,
                    windows::core::PWSTR(name_buf.as_mut_ptr()),
                    &mut name_len,
                    None,
                    Some(&mut ty),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                ) != ERROR_SUCCESS
                {
                    break;
                }
                let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                // REG_SZ(1)/REG_EXPAND_SZ(2): texto; outros tipos: hexadecimal
                let value = if ty == 1 || ty == 2 {
                    let nwords = (data_len as usize) / 2;
                    let words: Vec<u16> = data
                        .chunks_exact(2)
                        .take(nwords)
                        .map(|c| u16::from_ne_bytes([c[0], c[1]]))
                        .collect();
                    String::from_utf16_lossy(&words).trim_matches('\0').trim().to_string()
                } else {
                    format!("0x{}", data[..(data_len as usize).min(data.len())]
                        .iter()
                        .map(|b| format!("{:02X}", b))
                        .collect::<String>())
                };
                if !out.iter().any(|(n, _)| n == &name) {
                    out.push((name, value));
                }
                index += 1;
            }
            let _ = RegCloseKey(hkey);
        }
    }
    out
}

#[cfg(not(windows))]
pub fn enum_hklm_values(_subkey: &str) -> Vec<(String, String)> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn enum_hkcu_values(_subkey: &str) -> Vec<(String, String)> {
    Vec::new()
}
