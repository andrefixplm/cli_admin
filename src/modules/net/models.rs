use serde::{Deserialize, Serialize};

/// Protocolo da conexão/socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetProto {
    Tcp,
    Udp,
}

impl NetProto {
    pub fn label(&self) -> &'static str {
        match self {
            NetProto::Tcp => "TCP",
            NetProto::Udp => "UDP",
        }
    }
}

/// Estado de uma conexão TCP (códigos de `MIB_TCP_STATE` do IpHlpApi).
///
/// Códigos oficiais: 1=CLOSED, 2=LISTEN, 3=SYN_SENT, 4=SYN_RCVD,
/// 5=ESTAB, 6=FIN_WAIT1, 7=FIN_WAIT2, 8=CLOSE_WAIT, 9=CLOSING,
/// 10=LAST_ACK, 11=TIME_WAIT, 12=DELETE_TCB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
    DeleteTcb,
}

impl TcpState {
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(TcpState::Closed),
            2 => Some(TcpState::Listen),
            3 => Some(TcpState::SynSent),
            4 => Some(TcpState::SynReceived),
            5 => Some(TcpState::Established),
            6 => Some(TcpState::FinWait1),
            7 => Some(TcpState::FinWait2),
            8 => Some(TcpState::CloseWait),
            9 => Some(TcpState::Closing),
            10 => Some(TcpState::LastAck),
            11 => Some(TcpState::TimeWait),
            12 => Some(TcpState::DeleteTcb),
            _ => None,
        }
    }

    /// Rótulo compatível com a saída do `netstat -ano`.
    pub fn label(&self) -> &'static str {
        match self {
            TcpState::Closed => "CLOSED",
            TcpState::Listen => "LISTENING",
            TcpState::SynSent => "SYN_SENT",
            TcpState::SynReceived => "SYN_RECEIVED",
            TcpState::Established => "ESTABLISHED",
            TcpState::FinWait1 => "FIN_WAIT_1",
            TcpState::FinWait2 => "FIN_WAIT_2",
            TcpState::CloseWait => "CLOSE_WAIT",
            TcpState::Closing => "CLOSING",
            TcpState::LastAck => "LAST_ACK",
            TcpState::TimeWait => "TIME_WAIT",
            TcpState::DeleteTcb => "DELETE_TCB",
        }
    }

    /// Ordem de exibição alinhada ao código numérico oficial.
    pub fn sort_order(&self) -> u8 {
        match self {
            TcpState::Closed => 1,
            TcpState::Listen => 2,
            TcpState::SynSent => 3,
            TcpState::SynReceived => 4,
            TcpState::Established => 5,
            TcpState::FinWait1 => 6,
            TcpState::FinWait2 => 7,
            TcpState::CloseWait => 8,
            TcpState::Closing => 9,
            TcpState::LastAck => 10,
            TcpState::TimeWait => 11,
            TcpState::DeleteTcb => 12,
        }
    }
}

/// Uma linha da tabela de rede: socket local (+remoto p/ TCP) e o
/// processo dono (PID + nome resolvido via sysinfo no instante do scan).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetConnection {
    pub proto: NetProto,
    pub ipv6: bool,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    /// `None` quando não há par remoto (UDP ou TCP em LISTENING).
    pub remote_port: Option<u16>,
    /// `None` para UDP (protocolo sem estado).
    pub state: Option<TcpState>,
    pub pid: u32,
    #[serde(default)]
    pub process_name: String,
    #[serde(default)]
    pub is_protected: bool,
}

impl NetConnection {
    pub fn proto_label(&self) -> &'static str {
        self.proto.label()
    }

    pub fn state_label(&self) -> &'static str {
        match &self.state {
            Some(s) => s.label(),
            None => "—",
        }
    }

    pub fn local_endpoint(&self) -> String {
        if self.ipv6 {
            format!("[{}]:{}", self.local_ip, self.local_port)
        } else {
            format!("{}:{}", self.local_ip, self.local_port)
        }
    }

    pub fn remote_endpoint(&self) -> String {
        match self.remote_port {
            Some(port) => {
                if self.ipv6 {
                    format!("[{}]:{}", self.remote_ip, port)
                } else {
                    format!("{}:{}", self.remote_ip, port)
                }
            }
            None => "*".to_string(),
        }
    }

    pub fn is_listening(&self) -> bool {
        matches!(self.state, Some(TcpState::Listen))
    }

    /// Casa contra porta, PID, nome do processo, endereços, estado e protocolo.
    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        if self.local_port.to_string().contains(&q) {
            return true;
        }
        if self.pid.to_string().contains(&q) {
            return true;
        }
        if self.process_name.to_lowercase().contains(&q) {
            return true;
        }
        if self.local_ip.to_lowercase().contains(&q) {
            return true;
        }
        if self.remote_ip.to_lowercase().contains(&q) {
            return true;
        }
        if self.state_label().to_lowercase().contains(&q) {
            return true;
        }
        if self.proto_label().to_lowercase().contains(&q) {
            return true;
        }
        false
    }
}

/// Ordenação ativa da aba Rede.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetSort {
    Port,
    Pid,
    Name,
    State,
}

/// Filtro de protocolo da aba Rede.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetProtoFilter {
    All,
    Tcp,
    Udp,
}

impl NetProtoFilter {
    pub fn label(&self) -> &'static str {
        match self {
            NetProtoFilter::All => "TCP+UDP",
            NetProtoFilter::Tcp => "TCP",
            NetProtoFilter::Udp => "UDP",
        }
    }

    /// Cicla All → TCP → UDP → All (tecla `p`).
    pub fn next(&self) -> Self {
        match self {
            NetProtoFilter::All => NetProtoFilter::Tcp,
            NetProtoFilter::Tcp => NetProtoFilter::Udp,
            NetProtoFilter::Udp => NetProtoFilter::All,
        }
    }
}

/// Filtra por protocolo, modo "só LISTENING" e busca textual.
pub fn filter_connections<'a>(
    query: &str,
    proto_filter: NetProtoFilter,
    only_listening: bool,
    conns: &'a [NetConnection],
) -> Vec<&'a NetConnection> {
    conns
        .iter()
        .filter(|c| match proto_filter {
            NetProtoFilter::All => true,
            NetProtoFilter::Tcp => c.proto == NetProto::Tcp,
            NetProtoFilter::Udp => c.proto == NetProto::Udp,
        })
        .filter(|c| !only_listening || c.is_listening() || c.proto == NetProto::Udp)
        .filter(|c| c.matches_query(query))
        .collect()
}

/// Ordena a lista in-place conforme a escolha ativa da aba.
pub fn sort_connections(conns: &mut [NetConnection], sort: NetSort) {
    match sort {
        NetSort::Port => {
            conns.sort_by_key(|c| (c.local_port, c.proto_label(), c.pid));
        }
        NetSort::Pid => {
            conns.sort_by_key(|c| (c.pid, c.local_port));
        }
        NetSort::Name => {
            conns.sort_by(|a, b| {
                a.process_name
                    .to_lowercase()
                    .cmp(&b.process_name.to_lowercase())
                    .then(a.local_port.cmp(&b.local_port))
            });
        }
        NetSort::State => {
            conns.sort_by(|a, b| {
                let sa = a.state.map(|s| s.sort_order()).unwrap_or(u8::MAX);
                let sb = b.state.map(|s| s.sort_order()).unwrap_or(u8::MAX);
                sa.cmp(&sb).then(a.local_port.cmp(&b.local_port))
            });
        }
    }
}
