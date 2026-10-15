use cli_admin::config::AppConfig;
use cli_admin::modules::process::export::to_csv;
use cli_admin::modules::process::{ProcessFilter, ProcessItem, ProcessShield};
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
            cmd_line: String::new(),
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
            cmd_line: String::new(),
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
            cmd_line: String::new(),
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
        cmd_line: String::new(),
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

#[test]
fn test_export_csv_cmd_line() {
    let procs = vec![ProcessItem {
        pid: 4242,
        name: "SLDWORKS.exe".to_string(),
        cpu_pct: 3.0,
        memory_bytes: 1024 * 1024 * 100,
        is_protected: false,
        selected: false,
        exe_path: r"C:\Program Files\SOLIDWORKS Corp\SOLIDWORKS\SLDWORKS.exe".to_string(),
        cmd_line: r#""C:\Program Files\SOLIDWORKS Corp\SOLIDWORKS\SLDWORKS.exe" /r "C:\PDM\a,b.sldprt""#
            .to_string(),
        user: String::new(),
        start_time: 0,
        threads: 0,
        parent_pid: Some(1000),
        gpu_pct: 0.0,
        vram_bytes: 0,
        disk_read_bytes_sec: 0,
        disk_write_bytes_sec: 0,
    }];

    let csv = to_csv(&procs);
    let mut lines = csv.lines();
    assert!(lines.next().unwrap().contains("Linha_de_Comando"));
    let row = lines.next().unwrap();
    assert!(row.starts_with("4242,1000,SLDWORKS.exe,"));
    // Aspas duplicadas e campo citado por conter vírgula (RFC 4180)
    assert!(row.contains(
        r#","""C:\Program Files\SOLIDWORKS Corp\SOLIDWORKS\SLDWORKS.exe"" /r ""C:\PDM\a,b.sldprt""","#
    ));
}
