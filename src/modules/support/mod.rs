pub mod netdiag;
pub mod notes;
pub mod winsvc;
pub mod winsys;

pub use notes::{NoteMeta, NotesBackend, add_note, list_notes, read_note};
pub use winsys::{
    BootItem, DiskEntry, SvcDiff, SvcDiffKind, SvcSnapshot, VolumeInfo, boot_run_keys,
    boot_scheduled_tasks,
    bytes_to_human, diff_snapshots, disk_top, disk_top_cancelavel, parse_schtasks_csv,
    snapshot_services, volume_info,
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
