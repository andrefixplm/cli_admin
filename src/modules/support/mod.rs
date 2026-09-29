pub mod licensing;
pub mod netdiag;
pub mod notes;
pub mod solidworks;
pub mod winsvc;
pub mod winsys;

pub use licensing::{
    DiagCheck, LicFeature, LicUser, diagnose_snl, discover_lmutil, extract_ping_avg,
    firewall_mentions_ports, free_checkout, parse_lmstat, parse_totals, parse_user_line,
    query_all, splm_status,
};
pub use notes::{NoteMeta, NotesBackend, add_note, list_notes, read_note};
pub use solidworks::{
    BorrowEntry, CleanItem, GpuAdapter, SwInstall, certified_status, default_clean_roots,
    delete_clean_targets, discover_snl_log, gpu_adapters, mask_serial, nvidia_driver_version,
    parse_borrow_log, scan_clean_targets, sw_installations,
};
pub use winsys::{
    BootItem, DiskEntry, SvcDiff, SvcDiffKind, SvcSnapshot, boot_run_keys, boot_scheduled_tasks,
    bytes_to_human, diff_snapshots, disk_top, parse_schtasks_csv, snapshot_services,
};
pub use netdiag::{
    ArpEntry, PingStats, ProbeResult, arp_type_label, discover_ntttcp, expand_subnet,
    ntttcp_commands, parse_ipnet_table, parse_ntttcp_throughput, parse_ping_counts,
    parse_ping_summary, ping_csv_row, ping_series, probe_port, read_arp_table, resolve_host,
    sweep_subnet,
};
pub use winsvc::{
    SvcInfo, SvcState, enum_hkcu_subkeys, enum_hkcu_values, enum_hklm_subkeys, enum_hklm_values,
    find_services, list_services, read_hklm_string,
};
