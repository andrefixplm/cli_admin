use cli_admin::config::AppConfig;
use cli_admin::modules::net::{
    NetConnection, NetProto, NetProtoFilter, NetSort, TcpState, filter_connections,
    sort_connections,
};
use cli_admin::modules::process::{ProcessFilter, ProcessItem, ProcessShield};
use cli_admin::modules::support::{
    add_note, bytes_to_human, diff_snapshots, disk_top, disk_top_cancelavel,
    expand_subnet, list_notes, ntttcp_commands, parse_ipnet_table, parse_ntttcp_throughput,
    parse_ping_counts, parse_ping_summary, parse_schtasks_csv, ping_csv_row, read_note,
    volume_info, PingStats, SvcDiffKind, SvcSnapshot,
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
            cmd_line: String::new(),
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
            cmd_line: String::new(),
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
            cmd_line: String::new(),
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
        cmd_line: String::new(),
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
fn test_format_cmd_line() {
    use cli_admin::modules::top::system::format_cmd_line;
    use std::ffi::OsString;

    let args: Vec<OsString> = vec![
        r"C:\Program Files\app.exe".into(),
        "--flag".into(),
        "a b".into(),
        "say \"hi\"".into(),
    ];
    assert_eq!(
        format_cmd_line(&args),
        r#""C:\Program Files\app.exe" --flag "a b" "say \"hi\"""#
    );
    assert_eq!(format_cmd_line(&[]), "");
}

#[test]
fn test_processes_csv_export() {
    use cli_admin::modules::process::processes_to_csv;

    let items = vec![ProcessItem {
        pid: 42,
        name: "cmd.exe".to_string(),
        cpu_pct: 1.0,
        memory_bytes: 1024 * 1024 * 10,
        is_protected: false,
        selected: false,
        exe_path: r"C:\Windows\cmd.exe".to_string(),
        user: r"DOM\andre".to_string(),
        start_time: 0,
        threads: 0,
        parent_pid: Some(1),
        gpu_pct: 0.0,
        vram_bytes: 0,
        disk_read_bytes_sec: 0,
        disk_write_bytes_sec: 0,
        cmd_line: "cmd /c \"echo, oi\"".to_string(),
    }];

    let csv = processes_to_csv(&items);
    assert!(csv.starts_with("\u{FEFF}PID,PPID,Nome"));
    assert!(csv.lines().next().unwrap().contains("Linha_de_Comando"));
    assert!(csv.contains(",\"cmd /c \"\"echo, oi\"\"\","));
    assert!(csv.lines().nth(1).unwrap().ends_with(",NAO"));
}

// ---------------------------------------------------------------------------
// V3: escudo na finalização, linha de comando completa e refresh enxuto
// ---------------------------------------------------------------------------

#[test]
fn v3_separar_protegidos_nunca_libera_protegido() {
    use cli_admin::modules::process::killer::separar_protegidos;
    let alvos = [(100, false), (200, true), (300, false), (400, true)];
    let (liberados, bloqueados) = separar_protegidos(&alvos);
    assert_eq!(liberados, vec![100, 300]);
    assert_eq!(bloqueados, vec![200, 400]);
}

#[test]
fn v3_separar_protegidos_lista_vazia() {
    use cli_admin::modules::process::killer::separar_protegidos;
    let (liberados, bloqueados) = separar_protegidos(&[]);
    assert!(liberados.is_empty() && bloqueados.is_empty());
}

#[test]
fn v3_format_cmd_line_aspas_em_argumento_com_espaco() {
    use cli_admin::modules::top::system::format_cmd_line;
    use std::ffi::OsString;
    let args = vec![
        OsString::from(r"C:\Program Files\app.exe"),
        OsString::from("--flag"),
        OsString::from("valor com espaco"),
    ];
    assert_eq!(
        format_cmd_line(&args),
        r#""C:\Program Files\app.exe" --flag "valor com espaco""#
    );
}

/// Ponta a ponta: um processo criado pelo teste precisa aparecer com a linha de comando
/// completa (executável + argumentos), não só com o caminho do executável.
#[test]
fn v3_linha_de_comando_completa_do_proprio_processo() {
    use cli_admin::modules::top::SystemMetricsManager;
    use std::collections::HashMap;

    let mut child = std::process::Command::new("ping")
        .args(["-n", "8", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("ping deve existir no Windows");
    let pid = child.id();

    // Dá tempo para o processo existir e atualiza a lista de processos.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut manager = SystemMetricsManager::new();
    manager.refresh();
    let procs = manager.get_all_processes(&HashMap::new());
    let _ = child.kill();
    let _ = child.wait();

    let alvo = procs
        .iter()
        .find(|p| p.pid == pid)
        .expect("o processo criado pelo teste deve estar na lista");
    assert!(
        alvo.cmd_line.contains("-n") && alvo.cmd_line.contains("127.0.0.1"),
        "linha de comando incompleta: {:?}",
        alvo.cmd_line
    );
}

// ---------------------------------------------------------------------------
// V3: disco sob demanda, cancelável, com informação do volume
// ---------------------------------------------------------------------------

#[test]
fn v3_disk_top_cancelavel_cancelado_devolve_none() {
    use std::sync::atomic::AtomicBool;
    let root = std::env::temp_dir().join(format!("cli_admin_cancel_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::write(root.join("a").join("x.bin"), vec![0u8; 64]).unwrap();

    // Já cancelado antes de começar: não deve devolver resultado
    let cancelado = AtomicBool::new(true);
    assert!(disk_top_cancelavel(&root, 10, &cancelado).is_none());

    // Sem cancelamento: mesmo resultado de disk_top
    let livre = AtomicBool::new(false);
    let r = disk_top_cancelavel(&root, 10, &livre).expect("sem cancelar deve devolver");
    assert_eq!(r.len(), disk_top(&root, 10).len());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn v3_volume_info_da_pasta_temporaria() {
    let v = volume_info(&std::env::temp_dir()).expect("a unidade do TEMP deve ser encontrada");
    assert!(v.total > 0, "capacidade total deve ser positiva");
    assert!(v.free <= v.total, "livre não pode passar do total");
}

#[test]
fn v3_volume_info_caminho_inexistente_e_none() {
    assert!(volume_info(std::path::Path::new(r"Z:\caminho\que\nao\existe")).is_none());
}

/// Comportamento pedido: o Disco NÃO calcula ao entrar na seção; só por tecla; sem travar a
/// interface; o resultado chega pelo `tick`; e `Esc` cancela.
#[tokio::test]
async fn v3_disco_so_calcula_por_tecla_sem_travar_e_esc_cancela() {
    use cli_admin::tui::app::{ActiveTab, App, SupportSection};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::time::{Duration, Instant};

    let root = std::env::temp_dir().join(format!("cli_admin_tui_disk_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("grande")).unwrap();
    std::fs::create_dir_all(root.join("pequena")).unwrap();
    std::fs::write(root.join("grande").join("a.bin"), vec![0u8; 4096]).unwrap();
    std::fs::write(root.join("pequena").join("b.bin"), vec![0u8; 128]).unwrap();

    let mut app = App::new(AppConfig::default(), std::path::PathBuf::from("config.toml")).await;
    app.active_tab = ActiveTab::Support;
    app.support_section = SupportSection::Disk;
    app.disk_path = root.display().to_string();

    // 1) Entrar na seção Disco não calcula nada
    app.refresh_support_section().await;
    assert!(app.disk_scan.is_none(), "não pode iniciar sozinho ao entrar na seção");
    assert!(app.disk_entries.is_empty());

    // 2) A tecla [r] inicia em segundo plano e devolve o controle na hora
    let t0 = Instant::now();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)).await;
    assert!(t0.elapsed() < Duration::from_secs(2), "a tecla não pode travar a interface");
    assert!(app.disk_scan.is_some() || !app.disk_entries.is_empty());

    // 3) O resultado chega pelo tick
    for _ in 0..200 {
        app.tick().await;
        if app.disk_scan.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(app.disk_scan.is_none(), "o cálculo deveria ter terminado");
    assert_eq!(app.disk_entries.len(), 2);
    assert!(app.disk_entries[0].path.ends_with("grande"), "a maior pasta vem primeiro");
    assert!(app.disk_volume.is_some(), "a ocupação da unidade deve estar preenchida");

    // 4) [Esc] cancela um cálculo em andamento
    app.disk_entries.clear();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)).await;
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).await;
    assert!(app.disk_scan.is_none(), "Esc deve cancelar e limpar o estado");
    let _ = std::fs::remove_dir_all(&root);
}

/// Renderiza a TUI num terminal virtual e confere o gráfico de disco e a ausência da aba Licenças.
#[tokio::test]
async fn v3_render_grafico_de_disco_e_sem_aba_licencas() {
    use cli_admin::modules::support::{DiskEntry, VolumeInfo};
    use cli_admin::tui::app::{ActiveTab, App, SupportSection};
    use cli_admin::tui::ui;
    use ratatui::{Terminal, backend::TestBackend};

    let mut app = App::new(AppConfig::default(), std::path::PathBuf::from("config.toml")).await;
    app.active_tab = ActiveTab::Support;
    app.support_section = SupportSection::Disk;
    app.disk_path = r"C:\Dados".to_string();
    app.disk_entries = vec![
        DiskEntry { path: r"C:\Dados\videos".into(), bytes: 80 * 1024 * 1024 * 1024 },
        DiskEntry { path: r"C:\Dados\projetos".into(), bytes: 40 * 1024 * 1024 * 1024 },
        DiskEntry { path: r"C:\Dados\docs".into(), bytes: 5 * 1024 * 1024 * 1024 },
    ];
    app.disk_volume = Some(VolumeInfo {
        mount: r"C:\".into(),
        total: 500 * 1024 * 1024 * 1024,
        free: 120 * 1024 * 1024 * 1024,
    });

    let mut terminal = Terminal::new(TestBackend::new(150, 32)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();

    let buffer = terminal.backend().buffer().clone();
    let mut tela = String::new();
    for y in 0..buffer.area.height {
        let linha: String = (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect();
        tela.push_str(linha.trim_end());
        tela.push('\n');
    }
    println!("{tela}");

    assert!(tela.contains("█"), "deve haver barras cheias");
    assert!(tela.contains("░"), "deve haver trecho vazio nas barras menores");
    assert!(tela.contains("PROPORÇÃO") && tela.contains("% UNIDADE"));
    assert!(tela.contains("Unidade C:"), "deve mostrar a ocupação da unidade");
    assert!(tela.contains("videos"));
    assert!(!tela.contains("Licenças"), "a aba Licenças foi removida");
    assert!(tela.contains("[6] ") && tela.contains("[7] ") && tela.contains("Serviços"), "Suporte é [6]; Serviços é a nova [7]");
}
