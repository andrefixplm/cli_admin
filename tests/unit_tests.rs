use cli_admin::config::AppConfig;
use cli_admin::modules::net::{
    NetConnection, NetProto, NetProtoFilter, NetSort, TcpState, filter_connections,
    sort_connections,
};
use cli_admin::modules::process::{ProcessFilter, ProcessItem, ProcessShield};
use cli_admin::modules::support::{
    add_note, bytes_to_human, certified_status, diff_snapshots, disk_top, expand_subnet,
    extract_ping_avg, list_notes, mask_serial, ntttcp_commands, parse_borrow_log,
    parse_ipnet_table, parse_ntttcp_throughput, parse_ping_counts, parse_ping_summary,
    parse_schtasks_csv, parse_lmstat, parse_totals, parse_user_line, ping_csv_row, read_note,
    PingStats, SvcDiffKind, SvcSnapshot,
};
use cli_admin::modules::top::{ProcessResourceInfo, SystemResourceOverview};
use cli_admin::modules::vmware::{StartMode, StopMode, VmStatus};

#[test]
fn test_default_config() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.general.refresh_rate_ms, 1000);
    assert_eq!(cfg.metrics.default_sort, "vram");
    assert!(cfg.security.require_kill_confirmation);
    assert!(cfg.security.protected_processes.contains(&"csrss.exe".to_string()));
}

#[test]
fn test_process_shield() {
    let protected = vec!["svchost.exe".to_string(), "explorer.exe".to_string()];
    let shield = ProcessShield::new(&protected);

    // Testar processos protegidos explicitamente e pelo kernel NT
    assert!(shield.is_protected("svchost.exe", 1234));
    assert!(shield.is_protected("SVCHOST.EXE", 1234));
    assert!(shield.is_protected("explorer", 5678));
    assert!(shield.is_protected("csrss.exe", 999));
    assert!(shield.is_protected("System", 4));
    assert!(shield.is_protected("any_name.exe", 0));

    // Processos normais devem ser permitidos
    assert!(!shield.is_protected("notepad.exe", 4321));
    assert!(!shield.is_protected("chrome.exe", 9876));
}

#[test]
fn test_process_filter() {
    let filter = ProcessFilter::new();
    let procs = vec![
        ProcessItem {
            pid: 1001,
            name: "notepad.exe".to_string(),
            cpu_pct: 0.5,
            memory_bytes: 1024 * 1024 * 50,
            is_protected: false,
            selected: false,
            exe_path: String::new(),
            user: String::new(),
            start_time: 0,
            threads: 0,
            parent_pid: None,
            gpu_pct: 0.0,
            vram_bytes: 0,
            disk_read_bytes_sec: 0,
            disk_write_bytes_sec: 0,
        },
        ProcessItem {
            pid: 1002,
            name: "chrome.exe".to_string(),
            cpu_pct: 5.0,
            memory_bytes: 1024 * 1024 * 200,
            is_protected: false,
            selected: false,
            exe_path: String::new(),
            user: String::new(),
            start_time: 0,
            threads: 0,
            parent_pid: None,
            gpu_pct: 0.0,
            vram_bytes: 0,
            disk_read_bytes_sec: 0,
            disk_write_bytes_sec: 0,
        },
        ProcessItem {
            pid: 2003,
            name: "tc_server.exe".to_string(),
            cpu_pct: 12.0,
            memory_bytes: 1024 * 1024 * 500,
            is_protected: false,
            selected: false,
            exe_path: String::new(),
            user: String::new(),
            start_time: 0,
            threads: 0,
            parent_pid: None,
            gpu_pct: 0.0,
            vram_bytes: 0,
            disk_read_bytes_sec: 0,
            disk_write_bytes_sec: 0,
        },
    ];

    // Busca por substring no nome
    let res = filter.filter("note", &procs);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].name, "notepad.exe");

    // Busca por PID
    let res_pid = filter.filter("2003", &procs);
    assert_eq!(res_pid.len(), 1);
    assert_eq!(res_pid[0].name, "tc_server.exe");

    // Busca vazia retorna todos
    let res_all = filter.filter("", &procs);
    assert_eq!(res_all.len(), 3);
}

#[test]
fn test_top_models() {
    let proc = ProcessResourceInfo {
        pid: 1234,
        name: "test.exe".to_string(),
        cpu_pct: 25.5,
        memory_bytes: 1024 * 1024 * 150, // 150 MB
        disk_read_bytes_sec: 1024 * 500,
        disk_write_bytes_sec: 1024 * 500,
        gpu_pct: 10.0,
        vram_bytes: 1024 * 1024 * 80, // 80 MB
        exe_path: String::new(),
        user: String::new(),
        start_time: 0,
        threads: 0,
        parent_pid: None,
    };

    assert!((proc.memory_mb() - 150.0).abs() < 0.01);
    assert!((proc.vram_mb() - 80.0).abs() < 0.01);
    assert!((proc.total_disk_io_kb_sec() - 1000.0).abs() < 0.01);

    let overview = SystemResourceOverview {
        cpu_pct: 35.0,
        total_ram_bytes: 16 * 1024 * 1024 * 1024,
        used_ram_bytes: 8 * 1024 * 1024 * 1024,
        total_vram_bytes: 8 * 1024 * 1024 * 1024,
        used_vram_bytes: 2 * 1024 * 1024 * 1024,
        gpu_name: Some("RTX 500".to_string()),
        gpu_temp_c: Some(60),
        gpu_power_watts: Some(25.0),
    };

    assert_eq!(overview.ram_usage_pct(), 50.0);
    assert_eq!(overview.vram_usage_pct(), 25.0);
}

#[test]
fn test_vm_models() {
    assert_eq!("gui".parse::<StartMode>().unwrap(), StartMode::Gui);
    assert_eq!("nogui".parse::<StartMode>().unwrap(), StartMode::Nogui);
    assert!("invalid".parse::<StartMode>().is_err());

    assert_eq!("soft".parse::<StopMode>().unwrap(), StopMode::Soft);
    assert_eq!("hard".parse::<StopMode>().unwrap(), StopMode::Hard);

    assert_eq!(VmStatus::Running.to_string(), "RUNNING");
    assert_eq!(VmStatus::Stopped.to_string(), "STOPPED");
    assert_eq!(VmStatus::Paused.to_string(), "PAUSED");
    assert_eq!(VmStatus::Offline.to_string(), "OFFLINE");
}

fn sample_net_conn(
    proto: NetProto,
    port: u16,
    state: Option<TcpState>,
    pid: u32,
    name: &str,
) -> NetConnection {
    NetConnection {
        proto,
        ipv6: false,
        local_ip: "0.0.0.0".to_string(),
        local_port: port,
        remote_ip: "0.0.0.0".to_string(),
        remote_port: if state == Some(TcpState::Listen) {
            None
        } else {
            Some(50000 + port as u16 % 10000)
        },
        state,
        pid,
        process_name: name.to_string(),
        is_protected: false,
    }
}

#[test]
fn test_tcp_state_codes() {
    assert_eq!(TcpState::from_code(2), Some(TcpState::Listen));
    assert_eq!(TcpState::from_code(5), Some(TcpState::Established));
    assert_eq!(TcpState::from_code(11), Some(TcpState::TimeWait));
    assert_eq!(TcpState::from_code(0), None);
    assert_eq!(TcpState::from_code(99), None);
    assert_eq!(TcpState::Listen.label(), "LISTENING");
    assert_eq!(TcpState::Established.label(), "ESTABLISHED");
}

#[test]
fn test_net_connection_endpoints() {
    let c = sample_net_conn(NetProto::Tcp, 8080, Some(TcpState::Listen), 1234, "app.exe");
    assert_eq!(c.local_endpoint(), "0.0.0.0:8080");
    assert_eq!(c.remote_endpoint(), "*");
    assert_eq!(c.state_label(), "LISTENING");
    assert!(c.is_listening());

    let udp = sample_net_conn(NetProto::Udp, 53, None, 567, "dns.exe");
    assert_eq!(udp.state_label(), "—");
    assert!(!udp.is_listening());

    let v6 = NetConnection {
        ipv6: true,
        ..sample_net_conn(NetProto::Tcp, 443, Some(TcpState::Established), 910, "web.exe")
    };
    assert_eq!(v6.local_endpoint(), "[0.0.0.0]:443");
}

#[test]
fn test_net_filter() {
    let conns = vec![
        sample_net_conn(NetProto::Tcp, 8080, Some(TcpState::Listen), 1111, "java.exe"),
        sample_net_conn(NetProto::Tcp, 5432, Some(TcpState::Listen), 2222, "postgres.exe"),
        sample_net_conn(NetProto::Udp, 53, None, 3333, "dns.exe"),
    ];

    // Busca por porta
    assert_eq!(filter_connections("8080", NetProtoFilter::All, false, &conns).len(), 1);
    // Busca por nome do processo (case-insensitive)
    assert_eq!(
        filter_connections("POSTGRES", NetProtoFilter::All, false, &conns).len(),
        1
    );
    // Busca por estado
    assert_eq!(
        filter_connections("listening", NetProtoFilter::All, false, &conns).len(),
        2
    );
    // Filtro de protocolo
    assert_eq!(filter_connections("", NetProtoFilter::Udp, false, &conns).len(), 1);
    assert_eq!(filter_connections("", NetProtoFilter::Tcp, false, &conns).len(), 2);
    // Busca vazia retorna tudo
    assert_eq!(filter_connections("", NetProtoFilter::All, false, &conns).len(), 3);
}

#[test]
fn test_net_sort() {
    let mut conns = vec![
        sample_net_conn(NetProto::Tcp, 8080, Some(TcpState::Established), 3000, "zebra.exe"),
        sample_net_conn(NetProto::Tcp, 80, Some(TcpState::Listen), 1000, "alpha.exe"),
        sample_net_conn(NetProto::Udp, 53, None, 2000, "meio.exe"),
    ];

    sort_connections(&mut conns, NetSort::Port);
    assert_eq!(conns[0].local_port, 53);
    assert_eq!(conns[2].local_port, 8080);

    sort_connections(&mut conns, NetSort::Name);
    assert_eq!(conns[0].process_name, "alpha.exe");

    sort_connections(&mut conns, NetSort::Pid);
    assert_eq!(conns[0].pid, 1000);

    sort_connections(&mut conns, NetSort::State);
    assert_eq!(conns[0].state, Some(TcpState::Listen));
}

#[test]
fn test_net_proto_filter_cycle() {
    assert_eq!(NetProtoFilter::All.next(), NetProtoFilter::Tcp);
    assert_eq!(NetProtoFilter::Tcp.next(), NetProtoFilter::Udp);
    assert_eq!(NetProtoFilter::Udp.next(), NetProtoFilter::All);
}

const SAMPLE_LMSTAT: &str = r#"Flexible License Manager status on Wed 9/17 10:00

License server status: 25734@srvlic

Feature usage info:
Users of solidworks:  (Total of 10 licenses issued;  Total of 2 licenses in use)

  "solidworks" v2025.0101, vendor: sw_d
  floating license

    joao WS012 192.168.1.20 (v2025.0) (srvlic/25734 101), start Wed 9/17 9:00
    MARIA WS034 192.168.1.21 (v2025.0) (srvlic/25734 102), start Wed 9/17 9:15
Users of swofficepro:  (Total of 5 licenses issued;  Total of 0 licenses in use)

  "swofficepro" v2025.0101, vendor: sw_d
  floating license
"#;

#[test]
fn test_parse_lmstat_totals() {
    assert_eq!(
        parse_totals("Users of solidworks:  (Total of 10 licenses issued;  Total of 2 licenses in use)"),
        Some((10, 2))
    );
    assert_eq!(parse_totals("nada aqui"), None);
}

#[test]
fn test_parse_lmstat_user_line() {
    let u = parse_user_line("    joao WS012 192.168.1.20 (v2025.0) (srvlic/25734 101), start Wed 9/17 9:00").unwrap();
    assert_eq!(u.user, "joao");
    assert_eq!(u.host, "WS012");
    assert_eq!(u.display, "192.168.1.20");
    assert_eq!(u.version, "v2025.0");
    assert_eq!(u.server_host, "srvlic");
    assert_eq!(u.server_port, 25734);
    assert_eq!(u.handle, 101);
    assert_eq!(u.checkout, "Wed 9/17 9:00");
    assert!(parse_user_line("  linha qualquer sem checkout").is_none());
}

#[test]
fn test_parse_lmstat_full() {
    let feats = parse_lmstat(SAMPLE_LMSTAT);
    assert_eq!(feats.len(), 2);
    assert_eq!(feats[0].name, "solidworks");
    assert_eq!(feats[0].total_issued, 10);
    assert_eq!(feats[0].total_used, 2);
    assert_eq!(feats[0].free(), 8);
    assert_eq!(feats[0].users.len(), 2);
    assert_eq!(feats[1].name, "swofficepro");
    assert_eq!(feats[1].free(), 5);
}

#[test]
fn test_extract_ping_avg_locales() {
    let en = "Packets: Sent = 4, Received = 4\nMinimum = 1ms, Maximum = 2ms, Average = 1ms";
    assert_eq!(extract_ping_avg(en), Some("1ms".to_string()));
    let pt = "Pacotes: Enviados = 4, Recebidos = 4\nMínimo = 1ms, Máximo = 2ms, Média = 2ms";
    assert_eq!(extract_ping_avg(pt), Some("2ms".to_string()));
    assert_eq!(extract_ping_avg("sem dados"), None);
}

#[test]
fn test_notes_crud() {
    let dir = std::env::temp_dir().join(format!("cli_admin_notes_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let meta = add_note(&dir, "Visita Cliente X", &["snl".to_string()], "Trocado TIMEOUTALL.").unwrap();
    assert!(meta.id.starts_with("20"));
    assert!(meta.path.exists());

    let listed = list_notes(&dir, "");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Visita Cliente X");
    assert_eq!(listed[0].tags, vec!["snl".to_string()]);

    assert_eq!(list_notes(&dir, "cliente").len(), 1);
    assert_eq!(list_notes(&dir, "inexistente").len(), 0);

    let body = read_note(&dir, &meta.id[..12]).unwrap();
    assert!(body.contains("Trocado TIMEOUTALL."));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_parse_ping_summary_locales() {
    let en = "    Minimum = 1ms, Maximum = 5ms, Average = 2ms";
    assert_eq!(parse_ping_summary(en), (Some(1), Some(5), Some(2)));
    let pt = "    Mínimo = 0ms, Máximo = 1ms, Média = 0ms";
    assert_eq!(parse_ping_summary(pt), (Some(0), Some(1), Some(0)));
    // Console CP850 após from_utf8_lossy (acentos viram �)
    let mojibake = "    M�nimo = 0ms, M�ximo = 2ms, M�dia = 1ms";
    assert_eq!(parse_ping_summary(mojibake), (Some(0), Some(2), Some(1)));
    assert_eq!(parse_ping_summary("nada"), (None, None, None));
}

#[test]
fn test_parse_ping_counts() {
    let out = "Resposta de 8.8.8.8: bytes=32 tempo=10ms TTL=115\nResposta de 8.8.8.8: bytes=32 tempo=11ms TTL=115\nEsgotado o tempo limite do pedido.\n";
    assert_eq!(parse_ping_counts(out), (3, 2));
    let en = "Reply from 8.8.8.8: bytes=32 time=10ms TTL=115\nRequest timed out.\n";
    assert_eq!(parse_ping_counts(en), (2, 1));
}

#[test]
fn test_expand_subnet() {
    let ips = expand_subnet("192.168.1.0/30").unwrap();
    assert_eq!(ips.len(), 2);
    assert_eq!(ips[0].to_string(), "192.168.1.1");
    assert_eq!(ips[1].to_string(), "192.168.1.2");
    assert!(expand_subnet("10.0.0.1").is_err());
    assert!(expand_subnet("192.168.1.0/8").is_err());
}

#[test]
fn test_parse_ipnet_table() {
    // 1 entrada: count + linha de 24 bytes
    let mut buf = Vec::new();
    buf.extend_from_slice(&1u32.to_ne_bytes());
    buf.extend_from_slice(&1u32.to_ne_bytes()); // ifIndex
    buf.extend_from_slice(&6u32.to_ne_bytes()); // mac len
    buf.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33, 0, 0]); // mac
    buf.extend_from_slice(&0x0101A8C0u32.to_ne_bytes()); // 192.168.1.1 em ordem de rede
    buf.extend_from_slice(&3u32.to_ne_bytes()); // dinâmica
    let entries = parse_ipnet_table(&buf);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].ip, "192.168.1.1");
    assert_eq!(entries[0].mac, "AA-BB-CC-11-22-33");
    assert_eq!(entries[0].entry_type, 3);
    assert!(parse_ipnet_table(&[]).is_empty());
}

#[test]
fn test_ntttcp_helpers() {
    let cmds = ntttcp_commands("receiver", "10.0.0.5", 60, 4);
    assert_eq!(cmds.len(), 1);
    assert!(cmds[0].contains("-r") && cmds[0].contains("10.0.0.5"));
    let out = "Total Throughput       :  9412.55 Mbps\nblah";
    assert!((parse_ntttcp_throughput(out).unwrap() - 9412.55).abs() < 0.01);
    assert!(parse_ntttcp_throughput("sem dados").is_none());
}

#[test]
fn test_mask_serial() {
    assert_eq!(mask_serial("9000 0563 JXJJ"), "**** **** JXJJ");
    // Serial real nunca aparece em claro: só o último grupo sobrevive
    let m = mask_serial("9010 0563 2950 8363 HRXH YG85");
    assert!(m.ends_with("YG85"));
    assert!(!m.contains("9010"));
}

#[test]
fn test_parse_borrow_log() {
    let log = "16:20:11 (sw_d) BORROW: \"solidworks\" joao@WS012 1 licenses\n\
               16:21:00 (sw_d) Server started\n\
               17:00:00 (sw_d) BORROW: \"swofficepro\" maria@WS034 1 licenses\n";
    let entries = parse_borrow_log(log);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].feature, "solidworks");
    assert_eq!(entries[0].user_host, "joao@WS012");
    assert_eq!(entries[1].time, "17:00:00");
}

#[test]
fn test_disk_top_fixture() {
    let root = std::env::temp_dir().join(format!("cli_admin_disk_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("pasta_a")).unwrap();
    std::fs::create_dir_all(root.join("pasta_b")).unwrap();
    std::fs::write(root.join("pasta_a").join("big.bin"), vec![0u8; 2048]).unwrap();
    std::fs::write(root.join("pasta_b").join("small.bin"), vec![0u8; 512]).unwrap();

    let top = disk_top(&root, 10);
    assert_eq!(top.len(), 2);
    assert!(top[0].path.ends_with("pasta_a"));
    assert_eq!(top[0].bytes, 2048);
    assert_eq!(bytes_to_human(2048), "2.0 KB");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn test_diff_snapshots() {
    let mk = |n: &str, st: &str| SvcSnapshot { name: n.to_string(), display: n.to_string(), state: st.to_string() };
    let before = vec![mk("svc_a", "RUNNING"), mk("svc_b", "STOPPED"), mk("svc_old", "RUNNING")];
    let after = vec![mk("svc_a", "STOPPED"), mk("svc_b", "STOPPED"), mk("svc_new", "RUNNING")];
    let diffs = diff_snapshots(&before, &after);
    assert_eq!(diffs.len(), 3);
    assert!(diffs.iter().any(|d| d.kind == SvcDiffKind::Changed && d.name == "svc_a"));
    assert!(diffs.iter().any(|d| d.kind == SvcDiffKind::Added && d.name == "svc_new"));
    assert!(diffs.iter().any(|d| d.kind == SvcDiffKind::Removed && d.name == "svc_old"));
    assert!(diff_snapshots(&before, &before).is_empty());
}

#[test]
fn test_parse_schtasks_csv() {
    let out = "\"\\Minha Tarefa\",\"01/01/2026 09:00:00\",\"Pronta\"\n\"\\Outra\",\"N/A\",\"Desabilitada\"\n";
    let rows = parse_schtasks_csv(out);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "\\Minha Tarefa");
    assert_eq!(rows[0].2, "Pronta");
}

#[test]
fn test_ping_csv_row() {
    let s = PingStats { host: "srv".to_string(), sent: 4, received: 3, min_ms: Some(1), max_ms: Some(5), avg_ms: Some(2) };
    let row = ping_csv_row(&s);
    assert!(row.contains(",srv,4,25.0,1,2,5"));
}

#[test]
fn test_certified_status() {
    assert!(certified_status("NVIDIA RTX 500 Ada Generation Laptop GPU").starts_with("LINHA"));
    assert!(certified_status("Intel(R) Arc(TM) Pro Graphics").starts_with("LINHA"));
    assert!(certified_status("NVIDIA Quadro P2000").starts_with("LINHA"));
    assert!(certified_status("NVIDIA GeForce RTX 4060").starts_with("NÃO"));
    assert!(certified_status("XYZ 123").starts_with("DESCONHECIDA"));
}
