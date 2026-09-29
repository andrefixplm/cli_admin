use super::models::{NetConnection, NetProto, TcpState};
use crate::modules::process::ProcessShield;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Linha bruta da tabela do IpHlpApi, antes do enriquecimento com
/// nome do processo (sysinfo) e classificação do `ProcessShield`.
#[derive(Debug, Clone)]
pub struct RawEntry {
    pub proto: NetProto,
    pub ipv6: bool,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    pub remote_port: Option<u16>,
    pub state: Option<TcpState>,
    pub pid: u32,
}

/// Varre TCP+UDP (v4+v6) com dono (PID) via IpHlpApi e enriquece cada
/// linha com o nome do processo e o flag de proteção.
///
/// `pid_names`: mapa PID → nome obtido de `SystemMetricsManager`
/// no instante do scan (PIDs podem ser reciclados entre scan e kill —
/// o `taskkill` informa o erro caso o PID já não exista).
pub fn scan(pid_names: &HashMap<u32, String>, shield: &ProcessShield) -> Vec<NetConnection> {
    let mut conns: Vec<NetConnection> = scan_raw()
        .into_iter()
        .map(|raw| {
            let process_name = pid_names
                .get(&raw.pid)
                .cloned()
                .unwrap_or_else(|| "<desconhecido>".to_string());
            let is_protected = shield.is_protected(&process_name, raw.pid);
            NetConnection {
                proto: raw.proto,
                ipv6: raw.ipv6,
                local_ip: raw.local_ip,
                local_port: raw.local_port,
                remote_ip: raw.remote_ip,
                remote_port: raw.remote_port,
                state: raw.state,
                pid: raw.pid,
                process_name,
                is_protected,
            }
        })
        .collect();
    // Ordem padrão: porta crescente (como `netstat -ano`).
    conns.sort_by_key(|c| (c.local_port, c.proto_label(), c.pid));
    conns
}

// ---------------------------------------------------------------------------
// Implementação Windows via IpHlpApi (GetExtendedTcpTable / GetExtendedUdpTable)
// ---------------------------------------------------------------------------

/// `AF_INET` / `AF_INET6` sem puxar o feature WinSock inteiro.
#[cfg(windows)]
const AF_INET_U32: u32 = 2;
#[cfg(windows)]
const AF_INET6_U32: u32 = 23;

/// Classes de tabela com dono (PID): `*_TABLE_OWNER_PID_ALL` / `*_OWNER_PID`.
#[cfg(windows)]
const TCP_TABLE_OWNER_PID_ALL_I32: i32 = 5;
#[cfg(windows)]
const UDP_TABLE_OWNER_PID_I32: i32 = 1;

#[cfg(windows)]
mod win {
    use super::*;

    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

    /// Chamada two-step genérica: primeiro descobre o tamanho, depois lê.
    unsafe fn query_table(is_tcp: bool, af: u32, class_value: i32) -> Vec<u8> {
        use windows::Win32::NetworkManagement::IpHelper::{
            GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_CLASS, UDP_TABLE_CLASS,
        };

        let mut size: u32 = 0;
        if is_tcp {
            let _ = unsafe {
                GetExtendedTcpTable(
                    None,
                    &mut size,
                    false,
                    af,
                    TCP_TABLE_CLASS(class_value),
                    0,
                )
            };
        } else {
            let _ = unsafe {
                GetExtendedUdpTable(
                    None,
                    &mut size,
                    false,
                    af,
                    UDP_TABLE_CLASS(class_value),
                    0,
                )
            };
        }
        if size == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u8; size as usize];
        let ret = unsafe {
            if is_tcp {
                GetExtendedTcpTable(
                    Some(buf.as_mut_ptr() as *mut std::ffi::c_void),
                    &mut size,
                    false,
                    af,
                    TCP_TABLE_CLASS(class_value),
                    0,
                )
            } else {
                GetExtendedUdpTable(
                    Some(buf.as_mut_ptr() as *mut std::ffi::c_void),
                    &mut size,
                    false,
                    af,
                    UDP_TABLE_CLASS(class_value),
                    0,
                )
            }
        };
        if ret != 0 {
            tracing::warn!(
                "IpHlpApi retornou erro {} (tcp={}, af={})",
                ret,
                is_tcp,
                af
            );
            return Vec::new();
        }
        if (size as usize) > buf.len() {
            buf.truncate(size as usize);
        }
        buf
    }

    fn read_u32(buf: &[u8], off: usize) -> u32 {
        u32::from_ne_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
    }

    /// Porta gravada em network byte order nos 16 bits baixos do DWORD.
    fn port_from_dword(v: u32) -> u16 {
        u16::from_be((v & 0xFFFF) as u16)
    }

    /// DWORD lido da memória bruta contém os 4 bytes em ordem de rede.
    fn ipv4_from_dword(v: u32) -> String {
        Ipv4Addr::from(v.to_ne_bytes()).to_string()
    }

    fn ipv6_from_bytes(buf: &[u8], off: usize) -> String {
        let mut b = [0u8; 16];
        b.copy_from_slice(&buf[off..off + 16]);
        Ipv6Addr::from(b).to_string()
    }

    /// `MIB_TCPROW_OWNER_PID`: 6 DWORDs
    /// (state, localAddr, localPort, remoteAddr, remotePort, pid).
    fn parse_tcp_v4(buf: &[u8], out: &mut Vec<super::RawEntry>) {
        if buf.len() < 4 {
            return;
        }
        let n = read_u32(buf, 0) as usize;
        let row = 24usize;
        for i in 0..n {
            let off = 4 + i * row;
            if off + row > buf.len() {
                break;
            }
            let state = read_u32(buf, off);
            let local_port = port_from_dword(read_u32(buf, off + 8));
            let remote_port_raw = port_from_dword(read_u32(buf, off + 16));
            out.push(super::RawEntry {
                proto: NetProto::Tcp,
                ipv6: false,
                local_ip: ipv4_from_dword(read_u32(buf, off + 4)),
                local_port,
                remote_ip: ipv4_from_dword(read_u32(buf, off + 12)),
                remote_port: if remote_port_raw == 0 { None } else { Some(remote_port_raw) },
                state: TcpState::from_code(state),
                pid: read_u32(buf, off + 20),
            });
        }
    }

    /// `MIB_TCP6ROW_OWNER_PID`: addr[16], scope, port, addr[16], scope,
    /// port, state, pid = 56 bytes.
    fn parse_tcp_v6(buf: &[u8], out: &mut Vec<super::RawEntry>) {
        if buf.len() < 4 {
            return;
        }
        let n = read_u32(buf, 0) as usize;
        let row = 56usize;
        for i in 0..n {
            let off = 4 + i * row;
            if off + row > buf.len() {
                break;
            }
            let local_port = port_from_dword(read_u32(buf, off + 20));
            let remote_port_raw = port_from_dword(read_u32(buf, off + 44));
            out.push(super::RawEntry {
                proto: NetProto::Tcp,
                ipv6: true,
                local_ip: ipv6_from_bytes(buf, off),
                local_port,
                remote_ip: ipv6_from_bytes(buf, off + 24),
                remote_port: if remote_port_raw == 0 { None } else { Some(remote_port_raw) },
                state: TcpState::from_code(read_u32(buf, off + 48)),
                pid: read_u32(buf, off + 52),
            });
        }
    }

    /// `MIB_UDPROW_OWNER_PID`: 3 DWORDs (localAddr, localPort, pid).
    fn parse_udp_v4(buf: &[u8], out: &mut Vec<super::RawEntry>) {
        if buf.len() < 4 {
            return;
        }
        let n = read_u32(buf, 0) as usize;
        let row = 12usize;
        for i in 0..n {
            let off = 4 + i * row;
            if off + row > buf.len() {
                break;
            }
            out.push(super::RawEntry {
                proto: NetProto::Udp,
                ipv6: false,
                local_ip: ipv4_from_dword(read_u32(buf, off)),
                local_port: port_from_dword(read_u32(buf, off + 4)),
                remote_ip: "*".to_string(),
                remote_port: None,
                state: None,
                pid: read_u32(buf, off + 8),
            });
        }
    }

    /// `MIB_UDP6ROW_OWNER_PID`: addr[16], scope, port, pid = 28 bytes.
    fn parse_udp_v6(buf: &[u8], out: &mut Vec<super::RawEntry>) {
        if buf.len() < 4 {
            return;
        }
        let n = read_u32(buf, 0) as usize;
        let row = 28usize;
        for i in 0..n {
            let off = 4 + i * row;
            if off + row > buf.len() {
                break;
            }
            out.push(super::RawEntry {
                proto: NetProto::Udp,
                ipv6: true,
                local_ip: ipv6_from_bytes(buf, off),
                local_port: port_from_dword(read_u32(buf, off + 20)),
                remote_ip: "*".to_string(),
                remote_port: None,
                state: None,
                pid: read_u32(buf, off + 24),
            });
        }
    }

    pub(super) fn scan_raw_impl() -> Vec<super::RawEntry> {
        let mut out = Vec::new();
        unsafe {
            let buf = query_table(true, super::AF_INET_U32, super::TCP_TABLE_OWNER_PID_ALL_I32);
            parse_tcp_v4(&buf, &mut out);
            let buf = query_table(true, super::AF_INET6_U32, super::TCP_TABLE_OWNER_PID_ALL_I32);
            parse_tcp_v6(&buf, &mut out);
            let buf = query_table(false, super::AF_INET_U32, super::UDP_TABLE_OWNER_PID_I32);
            parse_udp_v4(&buf, &mut out);
            let buf = query_table(false, super::AF_INET6_U32, super::UDP_TABLE_OWNER_PID_I32);
            parse_udp_v6(&buf, &mut out);
            let _ = ERROR_INSUFFICIENT_BUFFER;
        }
        out
    }
}

#[cfg(windows)]
fn scan_raw() -> Vec<RawEntry> {
    win::scan_raw_impl()
}

/// Fora do Windows não há IpHlpApi — retorna vazio (binário é Windows-only).
#[cfg(not(windows))]
fn scan_raw() -> Vec<RawEntry> {
    Vec::new()
}
