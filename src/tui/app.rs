use crate::config::{AppConfig, VmConfigEntry};
use crate::modules::net::{
    self, NetConnection, NetProtoFilter, NetSort, filter_connections, sort_connections,
};
use crate::modules::process::{ProcessFilter, ProcessItem, ProcessKiller, ProcessShield};
use crate::modules::support::{
    self, ArpEntry, BootItem, DiagCheck, DiskEntry, GpuAdapter, LicFeature, LicUser, NoteMeta,
    PingStats, SwInstall,
};
use crate::modules::top::{
    GpuPdhCollector, NvidiaCollector, NvidiaGpuInfo, ProcessResourceInfo, SortField,
    SystemMetricsManager, SystemResourceOverview,
};
use crate::modules::vmware::{StartMode, StopMode, VmDiscovery, VmInfo, VmwareClient};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{error, info};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Vmware = 0,
    TopMetrics = 1,
    ProcessManager = 2,
    Network = 3,
    Licenses = 4,
    Support = 5,
}

impl ActiveTab {
    pub fn next(&self) -> Self {
        match self {
            ActiveTab::Vmware => ActiveTab::TopMetrics,
            ActiveTab::TopMetrics => ActiveTab::ProcessManager,
            ActiveTab::ProcessManager => ActiveTab::Network,
            ActiveTab::Network => ActiveTab::Licenses,
            ActiveTab::Licenses => ActiveTab::Support,
            ActiveTab::Support => ActiveTab::Vmware,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            ActiveTab::Vmware => ActiveTab::Support,
            ActiveTab::TopMetrics => ActiveTab::Vmware,
            ActiveTab::ProcessManager => ActiveTab::TopMetrics,
            ActiveTab::Network => ActiveTab::ProcessManager,
            ActiveTab::Licenses => ActiveTab::Network,
            ActiveTab::Support => ActiveTab::Licenses,
        }
    }
}

/// Seções internas da aba Suporte ([ ] alterna).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportSection {
    Ping,
    Lan,
    Sw,
    Disk,
    Boot,
    Notes,
}

impl SupportSection {
    pub fn label(&self) -> &'static str {
        match self {
            SupportSection::Ping => "Latência/Ping",
            SupportSection::Lan => "Mapa da LAN",
            SupportSection::Sw => "SolidWorks",
            SupportSection::Disk => "Disco",
            SupportSection::Boot => "Boot/Login",
            SupportSection::Notes => "Notas",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SupportSection::Ping => SupportSection::Lan,
            SupportSection::Lan => SupportSection::Sw,
            SupportSection::Sw => SupportSection::Disk,
            SupportSection::Disk => SupportSection::Boot,
            SupportSection::Boot => SupportSection::Notes,
            SupportSection::Notes => SupportSection::Ping,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SupportSection::Ping => SupportSection::Notes,
            SupportSection::Lan => SupportSection::Ping,
            SupportSection::Sw => SupportSection::Lan,
            SupportSection::Disk => SupportSection::Sw,
            SupportSection::Boot => SupportSection::Disk,
            SupportSection::Notes => SupportSection::Boot,
        }
    }
}

/// Linha da tabela de licenças: feature expansível + seus checkouts.
#[derive(Debug, Clone)]
pub enum LicRow {
    Feature { index: usize, expanded: bool },
    User { user: LicUser },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    SearchInput,
}

/// Ordenação ativa da aba Gerenciador de Processos.
/// Existe como estado (e não só ordenação inline) para que
/// `refresh_processes()` — chamado a cada tick — preserve a
/// escolha do usuário em vez de voltar sempre para nome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcSort {
    Name,
    Cpu,
    Ram,
    Vram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddVmField {
    Alias,
    Path,
}

#[derive(Debug, Clone)]
pub enum ModalState {
    None,
    ConfirmKill {
        pids: Vec<u32>,
        has_critical: bool,
    },
    ConfirmLicFree {
        feature: String,
        user: LicUser,
    },
    CreateSnapshot {
        vm_index: usize,
        snapshot_name: String,
    },
    AddVm {
        field: AddVmField,
        alias: String,
        path: String,
    },
    Help,
}

pub struct App {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub active_tab: ActiveTab,
    pub input_mode: InputMode,
    pub modal: ModalState,
    pub status_message: Option<(String, Instant)>,

    // VMware Tab State
    pub vm_client: VmwareClient,
    pub vm_discovery: VmDiscovery,
    pub vms: Vec<VmInfo>,
    pub selected_vm_index: usize,

    // TOP Metrics Tab State
    pub system_manager: SystemMetricsManager,
    pub gpu_collector: GpuPdhCollector,
    pub nvidia_info: Option<NvidiaGpuInfo>,
    pub overview: SystemResourceOverview,
    pub top_processes: Vec<ProcessResourceInfo>,
    pub selected_top_index: usize,
    pub top_sort: SortField,
    pub top_table_state: TableState,

    // Process Manager Tab State
    pub shield: ProcessShield,
    pub filter: ProcessFilter,
    pub all_processes: Vec<ProcessItem>,
    pub filtered_processes: Vec<ProcessItem>,
    pub selected_proc_index: usize,
    pub search_query: String,
    pub proc_sort: ProcSort,
    pub proc_table_state: TableState,

    // Network Tab State (portas ativas x processos)
    pub net_connections: Vec<NetConnection>,
    pub filtered_net: Vec<NetConnection>,
    pub selected_net_index: usize,
    pub net_query: String,
    pub net_sort: NetSort,
    pub net_proto_filter: NetProtoFilter,
    pub only_listening: bool,
    pub net_table_state: TableState,

    // Licenses Tab State (FlexNet/SNL)
    pub lic_features: Vec<LicFeature>,
    pub lic_rows: Vec<LicRow>,
    pub selected_lic_index: usize,
    pub lic_table_state: TableState,
    pub lic_lmutil: Option<PathBuf>,
    pub lic_diag: Vec<DiagCheck>,

    // Support Tab State
    pub support_section: SupportSection,
    pub support_table_state: TableState,
    pub selected_support_index: usize,
    // Latência
    pub ping_host: String,
    pub ping_stats: Option<PingStats>,
    pub ping_history: Vec<u32>,
    // LAN
    pub arp_entries: Vec<ArpEntry>,
    // SolidWorks
    pub sw_installs: Vec<SwInstall>,
    pub sw_gpus: Vec<GpuAdapter>,
    // Disco
    pub disk_path: String,
    pub disk_entries: Vec<DiskEntry>,
    // Boot
    pub boot_items: Vec<BootItem>,
    // Notas
    pub support_notes: Vec<NoteMeta>,
    pub note_query: String,
    pub note_body: Option<String>,

    // Kill History
    pub kill_history: Vec<KillRecord>,
}

#[derive(Debug, Clone)]
pub struct KillRecord {
    pub pid: u32,
    pub name: String,
    pub killed_at: Instant,
    pub success: bool,
}

impl App {
    pub async fn new(config: AppConfig, config_path: PathBuf) -> Self {
        let vm_client = VmwareClient::new(&config.vmware.vmrun_path);
        let vm_discovery = VmDiscovery::new(config.vmware.clone(), vm_client.clone());

        let shield = ProcessShield::new(&config.security.protected_processes);
        let filter = ProcessFilter::new();

        let top_sort = config
            .metrics
            .default_sort
            .parse::<SortField>()
            .unwrap_or(SortField::Vram);

        let system_manager = SystemMetricsManager::new();
        let gpu_collector = GpuPdhCollector::new(config.metrics.enable_gpu_metrics);

        let nvidia_info = if config.metrics.enable_nvml {
            NvidiaCollector::query_global_telemetry().await
        } else {
            None
        };

        let overview = SystemResourceOverview {
            cpu_pct: 0.0,
            total_ram_bytes: 0,
            used_ram_bytes: 0,
            total_vram_bytes: 0,
            used_vram_bytes: 0,
            gpu_name: None,
            gpu_temp_c: None,
            gpu_power_watts: None,
        };

        let mut app = Self {
            config: config.clone(),
            config_path,
            active_tab: ActiveTab::Vmware,
            input_mode: InputMode::Normal,
            modal: ModalState::None,
            status_message: Some((
                "Sistema iniciado com sucesso. Pressione [?] para ajuda.".to_string(),
                Instant::now(),
            )),

            vm_client,
            vm_discovery,
            vms: Vec::new(),
            selected_vm_index: 0,

            system_manager,
            gpu_collector,
            nvidia_info,
            overview,
            top_processes: Vec::new(),
            selected_top_index: 0,
            top_sort,
            top_table_state: TableState::default(),

            shield,
            filter,
            all_processes: Vec::new(),
            filtered_processes: Vec::new(),
            selected_proc_index: 0,
            search_query: String::new(),
            proc_sort: ProcSort::Name,
            proc_table_state: TableState::default(),
            net_connections: Vec::new(),
            filtered_net: Vec::new(),
            selected_net_index: 0,
            net_query: String::new(),
            net_sort: NetSort::Port,
            net_proto_filter: NetProtoFilter::All,
            only_listening: false,
            net_table_state: TableState::default(),
            lic_features: Vec::new(),
            lic_rows: Vec::new(),
            selected_lic_index: 0,
            lic_table_state: TableState::default(),
            lic_lmutil: support::discover_lmutil(config.licensing.lmutil_path.as_deref()),
            lic_diag: Vec::new(),
            support_section: SupportSection::Ping,
            support_table_state: TableState::default(),
            selected_support_index: 0,
            ping_host: String::from("8.8.8.8"),
            ping_stats: None,
            ping_history: Vec::new(),
            arp_entries: Vec::new(),
            sw_installs: Vec::new(),
            sw_gpus: Vec::new(),
            disk_path: String::from("."),
            disk_entries: Vec::new(),
            boot_items: Vec::new(),
            support_notes: Vec::new(),
            note_query: String::new(),
            note_body: None,
            kill_history: Vec::new(),
        };

        app.refresh_vms().await;
        app.refresh_metrics().await;
        app.refresh_processes();
        app.refresh_network();

        app
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        info!("TUI Status: {}", m);
        self.status_message = Some((m, Instant::now()));
    }

    pub async fn refresh_vms(&mut self) {
        self.vms = self.vm_discovery.discover_all().await;
        if self.selected_vm_index >= self.vms.len() && !self.vms.is_empty() {
            self.selected_vm_index = self.vms.len() - 1;
        }
    }

    pub async fn refresh_metrics(&mut self) {
        self.system_manager.refresh();
        let gpu_metrics = self.gpu_collector.get_metrics().await;

        if self.config.metrics.enable_nvml {
            if let Some(info) = NvidiaCollector::query_global_telemetry().await {
                self.nvidia_info = Some(info);
            }
        }

        self.overview = self
            .system_manager
            .get_overview(self.nvidia_info.as_ref(), &gpu_metrics);

        self.top_processes = self.system_manager.get_top_processes(
            self.top_sort,
            self.config.metrics.top_process_count,
            &gpu_metrics,
        );

        if self.selected_top_index >= self.top_processes.len() && !self.top_processes.is_empty() {
            self.selected_top_index = self.top_processes.len() - 1;
        }
        self.top_table_state.select(Some(self.selected_top_index));
    }

    pub fn refresh_processes(&mut self) {
        // Criar snapshot de processos a partir do sysinfo - busca TODOS os processos
        let selected_pids: std::collections::HashSet<u32> = self
            .all_processes
            .iter()
            .filter(|p| p.selected)
            .map(|p| p.pid)
            .collect();

        let gpu_metrics = self.gpu_collector.get_metrics_sync();
        let all_procs = self.system_manager.get_all_processes(&gpu_metrics);

        let mut items = Vec::new();
        for proc in &all_procs {
            let is_protected = self.shield.is_protected(&proc.name, proc.pid);
            let selected = selected_pids.contains(&proc.pid);

            items.push(ProcessItem {
                pid: proc.pid,
                name: proc.name.clone(),
                cpu_pct: proc.cpu_pct,
                memory_bytes: proc.memory_bytes,
                is_protected,
                selected,
                exe_path: proc.exe_path.clone(),
                user: proc.user.clone(),
                start_time: proc.start_time,
                threads: proc.threads,
                parent_pid: proc.parent_pid,
                gpu_pct: proc.gpu_pct,
                vram_bytes: proc.vram_bytes,
                disk_read_bytes_sec: proc.disk_read_bytes_sec,
                disk_write_bytes_sec: proc.disk_write_bytes_sec,
            });
        }

        // Ordenar conforme a escolha ativa da aba (persiste entre ticks)
        match self.proc_sort {
            ProcSort::Name => {
                items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            ProcSort::Cpu => {
                items.sort_by(|a, b| {
                    b.cpu_pct
                        .partial_cmp(&a.cpu_pct)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            ProcSort::Ram => {
                items.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
            }
            ProcSort::Vram => {
                items.sort_by(|a, b| b.vram_bytes.cmp(&a.vram_bytes));
            }
        }

        self.all_processes = items;
        self.apply_process_filter();
    }

    pub fn apply_process_filter(&mut self) {
        let filtered_refs = self.filter.filter(&self.search_query, &self.all_processes);
        self.filtered_processes = filtered_refs.into_iter().cloned().collect();

        if self.selected_proc_index >= self.filtered_processes.len()
            && !self.filtered_processes.is_empty()
        {
            self.selected_proc_index = self.filtered_processes.len() - 1;
        }

        // Sincronizar TableState com o índice selecionado
        self.proc_table_state.select(Some(self.selected_proc_index));
    }

    /// Varre sockets TCP/UDP (IpHlpApi) e cruza com sysinfo + `ProcessShield`.
    pub fn refresh_network(&mut self) {
        let gpu_metrics = self.gpu_collector.get_metrics_sync();
        let pid_names: std::collections::HashMap<u32, String> = self
            .system_manager
            .get_all_processes(&gpu_metrics)
            .into_iter()
            .map(|p| (p.pid, p.name))
            .collect();

        let mut conns = net::scan(&pid_names, &self.shield);
        sort_connections(&mut conns, self.net_sort);
        self.net_connections = conns;
        self.apply_net_filter();
    }

    pub fn apply_net_filter(&mut self) {
        let filtered_refs = filter_connections(
            &self.net_query,
            self.net_proto_filter,
            self.only_listening,
            &self.net_connections,
        );
        self.filtered_net = filtered_refs.into_iter().cloned().collect();

        if self.selected_net_index >= self.filtered_net.len() && !self.filtered_net.is_empty() {
            self.selected_net_index = self.filtered_net.len() - 1;
        }
        self.net_table_state.select(Some(self.selected_net_index));
    }

    /// Recarrega features/uso de licenças via lmstat.
    pub async fn refresh_licenses(&mut self) {
        let lmutil = match &self.lic_lmutil {
            Some(p) => p.clone(),
            None => {
                self.set_status("lmutil.exe não encontrado — configure licensing.lmutil_path.");
                return;
            }
        };
        let server = self.config.licensing.default_server.clone();
        match support::query_all(&lmutil, server.as_deref()).await {
            Ok(features) => {
                let total_users: usize = features.iter().map(|f| f.users.len()).sum();
                self.set_status(format!(
                    "{} feature(s), {} checkout(s) ativos.",
                    features.len(),
                    total_users
                ));
                self.lic_features = features;
                self.build_lic_rows();
            }
            Err(e) => self.set_status(format!("Erro no lmstat: {}", e)),
        }
    }

    fn build_lic_rows(&mut self) {
        let mut rows = Vec::new();
        for (index, f) in self.lic_features.iter().enumerate() {
            let expanded = self
                .lic_rows
                .iter()
                .any(|r| matches!(r, LicRow::Feature { index: i, expanded: true } if *i == index));
            // Preserva expansão anterior pelo nome da feature
            rows.push(LicRow::Feature { index, expanded });
            if expanded {
                for u in &f.users {
                    rows.push(LicRow::User { user: u.clone() });
                }
            }
        }
        self.lic_rows = rows;
        if self.selected_lic_index >= self.lic_rows.len() && !self.lic_rows.is_empty() {
            self.selected_lic_index = self.lic_rows.len() - 1;
        }
        self.lic_table_state.select(Some(self.selected_lic_index));
    }

    fn support_list_len(&self) -> usize {
        match self.support_section {
            SupportSection::Ping => 0,
            SupportSection::Lan => self.arp_entries.len(),
            SupportSection::Sw => self.sw_installs.len() + self.sw_gpus.len(),
            SupportSection::Disk => self.disk_entries.len(),
            SupportSection::Boot => self.boot_items.len(),
            SupportSection::Notes => self.support_notes.len(),
        }
    }

    fn clamp_support_selection(&mut self) {
        let len = self.support_list_len();
        if len == 0 {
            self.selected_support_index = 0;
        } else if self.selected_support_index >= len {
            self.selected_support_index = len - 1;
        }
        self.support_table_state.select(Some(self.selected_support_index));
    }

    /// Carrega a seção atual do Suporte (chamado ao trocar de seção ou com `r`).
    /// Ping e Disco são manuais (custo); as demais carregam aqui.
    pub async fn refresh_support_section(&mut self) {
        match self.support_section {
            SupportSection::Ping => {
                self.set_status("Seção Latência: edite o host com [/] e rode com [r].");
            }
            SupportSection::Lan => {
                self.arp_entries = support::read_arp_table();
                self.set_status(format!("{} entrada(s) ARP.", self.arp_entries.len()));
            }
            SupportSection::Sw => {
                self.sw_installs = support::sw_installations();
                self.sw_gpus = support::gpu_adapters().await;
                self.set_status(format!(
                    "{} instalação(ões) SW, {} GPU(s).",
                    self.sw_installs.len(),
                    self.sw_gpus.len()
                ));
            }
            SupportSection::Disk => {
                self.run_disk_top();
            }
            SupportSection::Boot => {
                let mut items = support::boot_run_keys();
                items.extend(support::boot_scheduled_tasks().await);
                self.boot_items = items;
                self.set_status(format!("{} item(ns) de inicialização.", self.boot_items.len()));
            }
            SupportSection::Notes => {
                self.reload_notes();
            }
        }
        self.clamp_support_selection();
    }

    fn run_disk_top(&mut self) {
        let root = PathBuf::from(&self.disk_path);
        self.set_status(format!("Calculando tamanhos em {} ...", root.display()));
        self.disk_entries = support::disk_top(&root, self.config.support.disk_top_n);
        self.set_status(format!(
            "Top {} em {} calculado(s).",
            self.disk_entries.len(),
            root.display()
        ));
    }

    fn reload_notes(&mut self) {
        let dir = self.config.notes.resolved_dir();
        self.support_notes = support::list_notes(&dir, &self.note_query);
    }

    pub async fn run_ping_series(&mut self) {
        let host = self.ping_host.clone();
        let n = self.config.netdiag.ping_count;
        self.set_status(format!("Pingando {} ({} pacotes)...", host, n));
        let stats = support::ping_series(&host, n).await;
        if let Some(avg) = stats.avg_ms {
            self.ping_history.push(avg);
            if self.ping_history.len() > 30 {
                self.ping_history.remove(0);
            }
        }
        self.set_status(stats.summary());
        self.ping_stats = Some(stats);
    }

    pub async fn tick(&mut self) {
        // Limpar status message após 5 segundos
        if let Some((_, time)) = self.status_message {
            if time.elapsed() > Duration::from_secs(5) {
                self.status_message = None;
            }
        }

        // Atualizar métricas continuamente
        self.refresh_metrics().await;
        // Se estiver na aba de processos e não estiver digitando, atualizar processos
        if self.active_tab == ActiveTab::ProcessManager
            && self.input_mode != InputMode::SearchInput
        {
            self.refresh_processes();
        }
        // Aba Rede: revarrer sockets somente quando visível e fora de digitação
        if self.active_tab == ActiveTab::Network && self.input_mode != InputMode::SearchInput {
            self.refresh_network();
        }
    }

    fn search_backspace(&mut self) {
        match self.active_tab {
            ActiveTab::Network => {
                self.net_query.pop();
                self.apply_net_filter();
            }
            ActiveTab::Support => match self.support_section {
                SupportSection::Ping => {
                    self.ping_host.pop();
                }
                SupportSection::Disk => {
                    self.disk_path.pop();
                }
                SupportSection::Notes => {
                    self.note_query.pop();
                    self.reload_notes();
                    self.clamp_support_selection();
                }
                _ => {}
            },
            _ => {
                self.search_query.pop();
                self.apply_process_filter();
            }
        }
    }

    fn search_push(&mut self, c: char) {
        match self.active_tab {
            ActiveTab::Network => {
                self.net_query.push(c);
                self.apply_net_filter();
            }
            ActiveTab::Support => match self.support_section {
                SupportSection::Ping => {
                    self.ping_host.push(c);
                }
                SupportSection::Disk => {
                    self.disk_path.push(c);
                }
                SupportSection::Notes => {
                    self.note_query.push(c);
                    self.reload_notes();
                    self.clamp_support_selection();
                }
                _ => {}
            },
            _ => {
                self.search_query.push(c);
                self.apply_process_filter();
            }
        }
    }

    /// Manipula eventos de teclado na TUI
    pub async fn handle_key(&mut self, key: KeyEvent) -> bool {
        // Se modal estiver ativo, interceptar entradas
        match &self.modal {
            ModalState::Help => {
                if key.code == KeyCode::Esc || key.code == KeyCode::Char('q') || key.code == KeyCode::Enter {
                    self.modal = ModalState::None;
                }
                return false;
            }
            ModalState::ConfirmKill { pids, .. } => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        let pids_to_kill = pids.clone();
                        self.modal = ModalState::None;
                        self.execute_kill_pids(&pids_to_kill).await;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.modal = ModalState::None;
                        self.set_status("Operação de cancelada pelo usuário.");
                    }
                    _ => {}
                }
                return false;
            }
            ModalState::ConfirmLicFree { feature, user } => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        let (f, u) = (feature.clone(), user.clone());
                        self.modal = ModalState::None;
                        self.execute_lic_free(&f, &u).await;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.modal = ModalState::None;
                        self.set_status("Liberação de licença cancelada.");
                    }
                    _ => {}
                }
                return false;
            }
            ModalState::CreateSnapshot { vm_index, snapshot_name } => {
                let idx = *vm_index;
                let mut name = snapshot_name.clone();
                match key.code {
                    KeyCode::Enter => {
                        if !name.trim().is_empty() {
                            self.modal = ModalState::None;
                            self.execute_create_snapshot(idx, &name).await;
                        }
                    }
                    KeyCode::Esc => {
                        self.modal = ModalState::None;
                    }
                    KeyCode::Backspace => {
                        name.pop();
                        self.modal = ModalState::CreateSnapshot {
                            vm_index: idx,
                            snapshot_name: name,
                        };
                    }
                    KeyCode::Char(c) => {
                        name.push(c);
                        self.modal = ModalState::CreateSnapshot {
                            vm_index: idx,
                            snapshot_name: name,
                        };
                    }
                    _ => {}
                }
                return false;
            }
            ModalState::AddVm { field, alias, path } => {
                let mut field = *field;
                let mut alias = alias.clone();
                let mut path = path.clone();
                match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        field = match field {
                            AddVmField::Alias => AddVmField::Path,
                            AddVmField::Path => AddVmField::Alias,
                        };
                    }
                    KeyCode::Enter => {
                        if !alias.trim().is_empty() && !path.trim().is_empty() {
                            self.modal = ModalState::None;
                            self.execute_add_vm(alias.trim().to_string(), path.trim().to_string())
                                .await;
                        }
                        return false;
                    }
                    KeyCode::Esc => {
                        self.modal = ModalState::None;
                        return false;
                    }
                    KeyCode::Backspace => match field {
                        AddVmField::Alias => {
                            alias.pop();
                        }
                        AddVmField::Path => {
                            path.pop();
                        }
                    },
                    KeyCode::Char(c) => match field {
                        AddVmField::Alias => alias.push(c),
                        AddVmField::Path => path.push(c),
                    },
                    _ => {}
                }
                self.modal = ModalState::AddVm { field, alias, path };
                return false;
            }
            ModalState::None => {}
        }

        // Se estiver em modo de digitação de busca
        if self.input_mode == InputMode::SearchInput {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.input_mode = InputMode::Normal;
                }
                KeyCode::Backspace => {
                    self.search_backspace();
                }
                KeyCode::Char(c) => {
                    self.search_push(c);
                }
                KeyCode::Up => self.select_prev(),
                KeyCode::Down => self.select_next(),
                _ => {}
            }
            return false;
        }

        // Atalhos globais
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('?') => {
                self.modal = ModalState::Help;
                return false;
            }
            KeyCode::Tab => {
                self.active_tab = self.active_tab.next();
                return false;
            }
            KeyCode::BackTab => {
                self.active_tab = self.active_tab.prev();
                return false;
            }
            KeyCode::Char('1') => {
                self.active_tab = ActiveTab::Vmware;
                return false;
            }
            KeyCode::Char('2') => {
                self.active_tab = ActiveTab::TopMetrics;
                return false;
            }
            KeyCode::Char('3') => {
                self.active_tab = ActiveTab::ProcessManager;
                return false;
            }
            KeyCode::Char('4') => {
                self.active_tab = ActiveTab::Network;
                return false;
            }
            KeyCode::Char('5') => {
                self.active_tab = ActiveTab::Licenses;
                return false;
            }
            KeyCode::Char('6') => {
                self.active_tab = ActiveTab::Support;
                return false;
            }
            _ => {}
        }

        // Atalhos específicos da aba ativa
        match self.active_tab {
            ActiveTab::Vmware => self.handle_vm_key(key).await,
            ActiveTab::TopMetrics => self.handle_top_key(key).await,
            ActiveTab::ProcessManager => self.handle_process_key(key).await,
            ActiveTab::Network => self.handle_network_key(key).await,
            ActiveTab::Licenses => self.handle_licenses_key(key).await,
            ActiveTab::Support => self.handle_support_key(key).await,
        }

        false
    }

    async fn handle_vm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.set_status("Atualizando inventário e status das VMs...");
                self.refresh_vms().await;
            }
            // Iniciar VM
            KeyCode::Char('>') | KeyCode::Char('s') => {
                if let Some(vm) = self.vms.get(self.selected_vm_index) {
                    let path = vm.path.clone();
                    let name = vm.name.clone();
                    self.set_status(format!("Iniciando VM '{}'...", name));
                    let res = self.vm_client.start(&path, StartMode::Nogui).await;
                    match res {
                        Ok(_) => self.set_status(format!("VM '{}' iniciada com sucesso.", name)),
                        Err(e) => self.set_status(format!("Erro ao iniciar VM: {}", e)),
                    }
                    self.refresh_vms().await;
                }
            }
            // Parar VM
            KeyCode::Char('x') | KeyCode::Char('X') => {
                if let Some(vm) = self.vms.get(self.selected_vm_index) {
                    let path = vm.path.clone();
                    let name = vm.name.clone();
                    self.set_status(format!("Desligando VM '{}'...", name));
                    let res = self.vm_client.stop(&path, StopMode::Soft).await;
                    match res {
                        Ok(_) => self.set_status(format!("VM '{}' desligada.", name)),
                        Err(e) => self.set_status(format!("Erro ao desligar VM: {}", e)),
                    }
                    self.refresh_vms().await;
                }
            }
            // Pausar VM
            KeyCode::Char('p') | KeyCode::Char('P') => {
                if let Some(vm) = self.vms.get(self.selected_vm_index) {
                    let path = vm.path.clone();
                    let name = vm.name.clone();
                    self.set_status(format!("Pausando VM '{}'...", name));
                    let res = self.vm_client.pause(&path).await;
                    match res {
                        Ok(_) => self.set_status(format!("VM '{}' pausada.", name)),
                        Err(e) => self.set_status(format!("Erro ao pausar: {}", e)),
                    }
                    self.refresh_vms().await;
                }
            }
            // Retomar VM (Unpause)
            KeyCode::Char('u') | KeyCode::Char('U') => {
                if let Some(vm) = self.vms.get(self.selected_vm_index) {
                    let path = vm.path.clone();
                    let name = vm.name.clone();
                    self.set_status(format!("Retomando VM '{}'...", name));
                    let res = self.vm_client.unpause(&path).await;
                    match res {
                        Ok(_) => self.set_status(format!("VM '{}' retomada.", name)),
                        Err(e) => self.set_status(format!("Erro ao retomar: {}", e)),
                    }
                    self.refresh_vms().await;
                }
            }
            // Suspender VM
            KeyCode::Char('z') | KeyCode::Char('Z') => {
                if let Some(vm) = self.vms.get(self.selected_vm_index) {
                    let path = vm.path.clone();
                    let name = vm.name.clone();
                    self.set_status(format!("Suspendendo VM '{}'...", name));
                    let res = self.vm_client.suspend(&path, StopMode::Soft).await;
                    match res {
                        Ok(_) => self.set_status(format!("VM '{}' suspensa.", name)),
                        Err(e) => self.set_status(format!("Erro ao suspender: {}", e)),
                    }
                    self.refresh_vms().await;
                }
            }
            // Criar Snapshot
            KeyCode::Char('n') | KeyCode::Char('N') => {
                if self.selected_vm_index < self.vms.len() {
                    self.modal = ModalState::CreateSnapshot {
                        vm_index: self.selected_vm_index,
                        snapshot_name: String::new(),
                    };
                }
            }
            // Cadastrar nova VM
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.modal = ModalState::AddVm {
                    field: AddVmField::Alias,
                    alias: String::new(),
                    path: String::new(),
                };
            }
            _ => {}
        }
    }

    async fn handle_top_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Char('c') => {
                self.top_sort = SortField::Cpu;
                self.set_status("Ordenando processos por CPU%");
                self.refresh_metrics().await;
            }
            KeyCode::Char('m') => {
                self.top_sort = SortField::Ram;
                self.set_status("Ordenando processos por Memória RAM");
                self.refresh_metrics().await;
            }
            KeyCode::Char('d') => {
                self.top_sort = SortField::DiskIo;
                self.set_status("Ordenando processos por Disco I/O");
                self.refresh_metrics().await;
            }
            KeyCode::Char('g') => {
                self.top_sort = SortField::Gpu;
                self.set_status("Ordenando processos por GPU%");
                self.refresh_metrics().await;
            }
            KeyCode::Char('v') => {
                self.top_sort = SortField::Vram;
                self.set_status("Ordenando processos por VRAM");
                self.refresh_metrics().await;
            }
            // Levar processo selecionado para o Gerenciador (aba 3) já filtrado
            KeyCode::Enter => {
                self.jump_top_to_process_manager();
            }
            _ => {}
        }
    }

    /// Salta da aba TOP para o Gerenciador de Processos com o
    /// processo selecionado já filtrado pelo nome exato.
    /// Homônimos (ex. vários `chrome.exe`) aparecem juntos, o que
    /// é útil para finalização em lote; o match exato rankeia primeiro.
    fn jump_top_to_process_manager(&mut self) {
        let name = match self.top_processes.get(self.selected_top_index) {
            Some(proc) => proc.name.clone(),
            None => {
                self.set_status("Nenhum processo selecionado na aba TOP.");
                return;
            }
        };

        self.active_tab = ActiveTab::ProcessManager;
        self.input_mode = InputMode::Normal;
        self.search_query = name.clone();
        self.refresh_processes();
        // Voltar o cursor para o topo (match exato rankeia primeiro no fuzzy)
        self.selected_proc_index = 0;
        self.apply_process_filter();
        self.set_status(format!(
            "Filtrado por '{}'. [/] edita a busca, [Ctrl+C] limpa.",
            name
        ));
    }

    async fn handle_process_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Char('/') => {
                self.input_mode = InputMode::SearchInput;
            }
            // Selecionar / desselecionar processo com Espaço
            KeyCode::Char(' ') => {
                if let Some(target) = self.filtered_processes.get_mut(self.selected_proc_index) {
                    target.selected = !target.selected;
                    let pid = target.pid;
                    let sel = target.selected;

                    // Sincronizar na lista principal
                    if let Some(orig) = self.all_processes.iter_mut().find(|p| p.pid == pid) {
                        orig.selected = sel;
                    }
                }
            }
            // Limpar busca
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search_query.clear();
                self.apply_process_filter();
            }
            // Ordenar por nome
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.proc_sort = ProcSort::Name;
                self.refresh_processes();
                self.set_status("Ordenado por nome");
            }
            // Ordenar por CPU
            KeyCode::Char('C') => {
                self.proc_sort = ProcSort::Cpu;
                self.refresh_processes();
                self.set_status("Ordenado por CPU%");
            }
            // Ordenar por Memória
            KeyCode::Char('M') => {
                self.proc_sort = ProcSort::Ram;
                self.refresh_processes();
                self.set_status("Ordenado por Memória");
            }
            // Ordenar por VRAM
            KeyCode::Char('V') => {
                self.proc_sort = ProcSort::Vram;
                self.refresh_processes();
                self.set_status("Ordenado por VRAM");
            }
            // Exportar lista (JSON com Shift+E, CSV com E)
            KeyCode::Char('e') => {
                match self.export_processes("csv") {
                    Ok(filename) => self.set_status(format!("Exportado para: {}", filename)),
                    Err(e) => self.set_status(format!("Erro ao exportar: {}", e)),
                }
            }
            KeyCode::Char('E') => {
                match self.export_processes("json") {
                    Ok(filename) => self.set_status(format!("Exportado para: {}", filename)),
                    Err(e) => self.set_status(format!("Erro ao exportar: {}", e)),
                }
            }
            // Disparar ação de Kill
            KeyCode::Char('K') | KeyCode::Delete | KeyCode::Enter => {
                self.trigger_kill_modal();
            }
            _ => {}
        }
    }

    async fn handle_network_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Char('/') => {
                self.input_mode = InputMode::SearchInput;
            }
            // Limpar busca
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.net_query.clear();
                self.apply_net_filter();
            }
            // Atualizar varredura
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.set_status("Revarrendo portas ativas...");
                self.refresh_network();
            }
            // Ciclar filtro de protocolo TCP+UDP → TCP → UDP
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.net_proto_filter = self.net_proto_filter.next();
                self.apply_net_filter();
                self.set_status(format!("Filtro de protocolo: {}", self.net_proto_filter.label()));
            }
            // Alternar modo "somente em escuta"
            KeyCode::Char('l') | KeyCode::Char('L') => {
                self.only_listening = !self.only_listening;
                self.apply_net_filter();
                self.set_status(if self.only_listening {
                    "Exibindo somente sockets em escuta.".to_string()
                } else {
                    "Exibindo todas as conexões.".to_string()
                });
            }
            // Ordenações
            KeyCode::Char('o') | KeyCode::Char('O') => {
                self.net_sort = NetSort::Port;
                self.refresh_network();
                self.set_status("Ordenado por porta");
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.net_sort = NetSort::Name;
                self.refresh_network();
                self.set_status("Ordenado por processo");
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.net_sort = NetSort::Pid;
                self.refresh_network();
                self.set_status("Ordenado por PID");
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                self.net_sort = NetSort::State;
                self.refresh_network();
                self.set_status("Ordenado por estado");
            }
            // Exportar lista (JSON com Shift+E, CSV com E)
            KeyCode::Char('e') => {
                match self.export_network("csv") {
                    Ok(filename) => self.set_status(format!("Exportado para: {}", filename)),
                    Err(e) => self.set_status(format!("Erro ao exportar: {}", e)),
                }
            }
            KeyCode::Char('E') => {
                match self.export_network("json") {
                    Ok(filename) => self.set_status(format!("Exportado para: {}", filename)),
                    Err(e) => self.set_status(format!("Erro ao exportar: {}", e)),
                }
            }
            // Disparar ação de Kill no processo dono da porta
            KeyCode::Char('K') | KeyCode::Delete | KeyCode::Enter => {
                self.trigger_net_kill_modal();
            }
            _ => {}
        }
    }

    fn trigger_net_kill_modal(&mut self) {
        let target = match self.filtered_net.get(self.selected_net_index) {
            Some(c) => c.clone(),
            None => {
                self.set_status("Nenhum socket selecionado para finalizar.");
                return;
            }
        };

        self.modal = ModalState::ConfirmKill {
            pids: vec![target.pid],
            has_critical: target.is_protected,
        };
        self.set_status(format!(
            "Alvo: {} ({} pid {}) — confirme a finalização.",
            target.local_endpoint(),
            target.process_name,
            target.pid
        ));
    }

    /// Exporta a lista de sockets filtrados para um arquivo JSON ou CSV
    pub fn export_network(&self, format: &str) -> Result<String, String> {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("net_ports_{}.{}", timestamp, format);

        let content = match format {
            "json" => {
                serde_json::to_string_pretty(&self.filtered_net)
                    .map_err(|e| format!("Erro ao serializar JSON: {}", e))?
            }
            "csv" => {
                let mut csv = String::from("Proto,Local,Remoto,Estado,PID,Processo,Protegido\n");
                for c in &self.filtered_net {
                    csv.push_str(&format!(
                        "{},{},{},{},{},{},{}\n",
                        c.proto_label(),
                        c.local_endpoint(),
                        c.remote_endpoint(),
                        c.state_label(),
                        c.pid,
                        c.process_name.replace(',', ";"),
                        if c.is_protected { "SIM" } else { "NAO" }
                    ));
                }
                csv
            }
            _ => return Err(format!("Formato '{}' não suportado. Use 'json' ou 'csv'.", format)),
        };

        std::fs::write(&filename, &content)
            .map_err(|e| format!("Erro ao escrever arquivo: {}", e))?;

        Ok(filename)
    }

    async fn handle_licenses_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            // Atualizar lmstat
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.set_status("Consultando lmstat...");
                self.refresh_licenses().await;
            }
            // Expandir/recolher checkouts da feature (Enter ou Espaço)
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(LicRow::Feature { index, .. }) =
                    self.lic_rows.get(self.selected_lic_index).cloned()
                {
                    let name = self
                        .lic_features
                        .get(index)
                        .map(|f| f.name.clone())
                        .unwrap_or_default();
                    // Alterna: reconstroi linhas com a expansão invertida
                    let mut expanded: std::collections::HashSet<usize> = self
                        .lic_rows
                        .iter()
                        .filter_map(|r| match r {
                            LicRow::Feature { index: i, expanded: true } => Some(*i),
                            _ => None,
                        })
                        .collect();
                    if !expanded.insert(index) {
                        expanded.remove(&index);
                    }
                    let mut rows = Vec::new();
                    for (i, f) in self.lic_features.iter().enumerate() {
                        let exp = expanded.contains(&i);
                        rows.push(LicRow::Feature { index: i, expanded: exp });
                        if exp {
                            for u in &f.users {
                                rows.push(LicRow::User { user: u.clone() });
                            }
                        }
                    }
                    self.lic_rows = rows;
                    self.lic_table_state.select(Some(self.selected_lic_index));
                    self.set_status(format!("Feature '{}': {}", name, if expanded.contains(&index) { "expandida" } else { "recolhida" }));
                }
            }
            // Diagnóstico SNL (resultado no painel de detalhes)
            KeyCode::Char('d') | KeyCode::Char('D') => {
                let server = self
                    .config
                    .licensing
                    .default_server
                    .clone()
                    .and_then(|s| s.rsplit_once('@').map(|(_, h)| h.to_string()))
                    .unwrap_or_else(|| "localhost".to_string());
                self.set_status(format!("Diagnosticando {} ...", server));
                self.lic_diag = support::diagnose_snl(
                    &server,
                    self.config.licensing.lm_port,
                    self.config.licensing.vendor_port,
                )
                .await;
                let fails = self.lic_diag.iter().filter(|c| !c.ok).count();
                self.set_status(format!("Diagnóstico: {} falha(s).", fails));
            }
            // Liberar checkout selecionado (linha de usuário)
            KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Delete => {
                if let Some(LicRow::User { user }) = self.lic_rows.get(self.selected_lic_index).cloned() {
                    // Descobre a feature dona procurando a Feature row acima
                    let mut feature = String::new();
                    for row in self.lic_rows[..self.selected_lic_index].iter().rev() {
                        if let LicRow::Feature { index, .. } = row {
                            feature = self.lic_features.get(*index).map(|f| f.name.clone()).unwrap_or_default();
                            break;
                        }
                    }
                    if feature.is_empty() {
                        self.set_status("Feature do checkout não identificada.");
                    } else {
                        self.modal = ModalState::ConfirmLicFree { feature, user };
                    }
                } else {
                    self.set_status("Selecione uma linha de usuário (expanda a feature com Enter).");
                }
            }
            _ => {}
        }
    }

    async fn execute_lic_free(&mut self, feature: &str, user: &LicUser) {
        let lmutil = match &self.lic_lmutil {
            Some(p) => p.clone(),
            None => {
                self.set_status("lmutil.exe não encontrado.");
                return;
            }
        };
        let server = self.config.licensing.default_server.clone();
        self.set_status(format!("Liberando '{}' de {} @ {} ...", feature, user.user, user.host));
        match support::free_checkout(&lmutil, server.as_deref(), feature, user).await {
            Ok(_) => self.set_status(format!("Checkout '{}' de {} liberado.", feature, user.user)),
            Err(e) => self.set_status(format!("Erro ao liberar: {}", e)),
        }
        self.refresh_licenses().await;
    }

    async fn handle_support_key(&mut self, key: KeyEvent) {
        match key.code {
            // Alternar seção
            KeyCode::Char('[') => {
                self.support_section = self.support_section.prev();
                self.selected_support_index = 0;
                self.note_body = None;
                self.refresh_support_section().await;
            }
            KeyCode::Char(']') => {
                self.support_section = self.support_section.next();
                self.selected_support_index = 0;
                self.note_body = None;
                self.refresh_support_section().await;
            }
            KeyCode::Up | KeyCode::Char('k') => self.select_prev(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next(),
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            // Editar campo da seção (host ping / caminho disco / busca notas)
            KeyCode::Char('/') => match self.support_section {
                SupportSection::Ping | SupportSection::Disk | SupportSection::Notes => {
                    self.input_mode = InputMode::SearchInput;
                }
                _ => {
                    self.set_status("Esta seção não tem campo editável.");
                }
            },
            // Executar/atualizar ação da seção
            KeyCode::Char('r') | KeyCode::Char('R') | KeyCode::Enter => {
                match self.support_section {
                    SupportSection::Ping => self.run_ping_series().await,
                    SupportSection::Disk => self.run_disk_top(),
                    _ => self.refresh_support_section().await,
                }
                self.clamp_support_selection();
            }
            // Limpar busca de notas
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.note_query.clear();
                self.reload_notes();
                self.clamp_support_selection();
            }
            // Exportar evidência de ping (CSV)
            KeyCode::Char('e') => {
                if self.support_section != SupportSection::Ping {
                    self.set_status("Exportação disponível na seção Latência.");
                    return;
                }
                match &self.ping_stats {
                    Some(stats) => {
                        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
                        let filename = format!("ping_{}_{}.csv", stats.host.replace('.', "_"), ts);
                        let content = format!(
                            "quando,host,enviados,perda_pct,min_ms,avg_ms,max_ms\n{}\n",
                            support::ping_csv_row(stats)
                        );
                        match std::fs::write(&filename, &content) {
                            Ok(_) => self.set_status(format!("Exportado para: {}", filename)),
                            Err(e) => self.set_status(format!("Erro ao exportar: {}", e)),
                        }
                    }
                    None => self.set_status("Rode uma série de ping primeiro ([r])."),
                }
            }
            // Ler nota selecionada no painel de detalhes
            KeyCode::Char('v') | KeyCode::Char('V') => {
                if self.support_section != SupportSection::Notes {
                    return;
                }
                if let Some(meta) = self.support_notes.get(self.selected_support_index) {
                    let dir = self.config.notes.resolved_dir();
                    match support::read_note(&dir, &meta.id) {
                        Ok(body) => self.note_body = Some(body),
                        Err(e) => self.set_status(format!("Erro ao ler nota: {}", e)),
                    }
                }
            }
            _ => {}
        }
    }

    pub fn select_next(&mut self) {
        match self.active_tab {
            ActiveTab::Vmware => {
                if !self.vms.is_empty() && self.selected_vm_index < self.vms.len() - 1 {
                    self.selected_vm_index += 1;
                }
            }
            ActiveTab::TopMetrics => {
                if !self.top_processes.is_empty()
                    && self.selected_top_index < self.top_processes.len() - 1
                {
                    self.selected_top_index += 1;
                    self.top_table_state.select(Some(self.selected_top_index));
                }
            }
            ActiveTab::ProcessManager => {
                if !self.filtered_processes.is_empty()
                    && self.selected_proc_index < self.filtered_processes.len() - 1
                {
                    self.selected_proc_index += 1;
                    self.proc_table_state.select(Some(self.selected_proc_index));
                }
            }
            ActiveTab::Network => {
                if !self.filtered_net.is_empty()
                    && self.selected_net_index < self.filtered_net.len() - 1
                {
                    self.selected_net_index += 1;
                    self.net_table_state.select(Some(self.selected_net_index));
                }
            }
            ActiveTab::Licenses => {
                if !self.lic_rows.is_empty() && self.selected_lic_index < self.lic_rows.len() - 1 {
                    self.selected_lic_index += 1;
                    self.lic_table_state.select(Some(self.selected_lic_index));
                }
            }
            ActiveTab::Support => {
                if self.selected_support_index + 1 < self.support_list_len() {
                    self.selected_support_index += 1;
                    self.support_table_state.select(Some(self.selected_support_index));
                }
            }
        }
    }

    pub fn select_prev(&mut self) {
        match self.active_tab {
            ActiveTab::Vmware => {
                if self.selected_vm_index > 0 {
                    self.selected_vm_index -= 1;
                }
            }
            ActiveTab::TopMetrics => {
                if self.selected_top_index > 0 {
                    self.selected_top_index -= 1;
                    self.top_table_state.select(Some(self.selected_top_index));
                }
            }
            ActiveTab::ProcessManager => {
                if self.selected_proc_index > 0 {
                    self.selected_proc_index -= 1;
                    self.proc_table_state.select(Some(self.selected_proc_index));
                }
            }
            ActiveTab::Network => {
                if self.selected_net_index > 0 {
                    self.selected_net_index -= 1;
                    self.net_table_state.select(Some(self.selected_net_index));
                }
            }
            ActiveTab::Licenses => {
                if self.selected_lic_index > 0 {
                    self.selected_lic_index -= 1;
                    self.lic_table_state.select(Some(self.selected_lic_index));
                }
            }
            ActiveTab::Support => {
                if self.selected_support_index > 0 {
                    self.selected_support_index -= 1;
                    self.support_table_state.select(Some(self.selected_support_index));
                }
            }
        }
    }

    pub fn page_down(&mut self) {
        for _ in 0..10 {
            self.select_next();
        }
    }

    pub fn page_up(&mut self) {
        for _ in 0..10 {
            self.select_prev();
        }
    }

    /// Exporta a lista de processos filtrados para um arquivo JSON ou CSV
    pub fn export_processes(&self, format: &str) -> Result<String, String> {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("processes_{}.{}", timestamp, format);

        let content = match format {
            "json" => {
                serde_json::to_string_pretty(&self.filtered_processes)
                    .map_err(|e| format!("Erro ao serializar JSON: {}", e))?
            }
            "csv" => {
                let mut csv = String::from("PID,Nome,CPU%,Memoria_MB,Path,Usuario,Uptime,Protegido\n");
                for proc in &self.filtered_processes {
                    csv.push_str(&format!(
                        "{},{},{:.1},{:.1},{},{},{},{}\n",
                        proc.pid,
                        proc.name.replace(',', ";"),
                        proc.cpu_pct,
                        proc.memory_mb(),
                        proc.exe_path.replace(',', ";"),
                        proc.user.replace(',', ";"),
                        proc.formatted_uptime(),
                        if proc.is_protected { "SIM" } else { "NAO" }
                    ));
                }
                csv
            }
            _ => return Err(format!("Formato '{}' não suportado. Use 'json' ou 'csv'.", format)),
        };

        std::fs::write(&filename, &content)
            .map_err(|e| format!("Erro ao escrever arquivo: {}", e))?;

        Ok(filename)
    }

    fn trigger_kill_modal(&mut self) {
        let mut pids_to_kill: Vec<u32> = self
            .all_processes
            .iter()
            .filter(|p| p.selected)
            .map(|p| p.pid)
            .collect();

        // Se nenhum estiver marcado com checkbox, usar o processo atualmente destacado pelo cursor
        if pids_to_kill.is_empty() {
            if let Some(target) = self.filtered_processes.get(self.selected_proc_index) {
                pids_to_kill.push(target.pid);
            }
        }

        if pids_to_kill.is_empty() {
            self.set_status("Nenhum processo selecionado para finalizar.");
            return;
        }

        // Verificar se há processos protegidos selecionados
        let mut has_critical = false;
        for &pid in &pids_to_kill {
            if let Some(proc) = self.all_processes.iter().find(|p| p.pid == pid) {
                if proc.is_protected {
                    has_critical = true;
                    break;
                }
            }
        }

        self.modal = ModalState::ConfirmKill {
            pids: pids_to_kill,
            has_critical,
        };
    }

    async fn execute_kill_pids(&mut self, pids: &[u32]) {
        self.set_status(format!("Finalizando {} processo(s)...", pids.len()));
        let results = ProcessKiller::kill_batch(pids, true, true).await;

        let mut success_count = 0;
        for (pid, res) in &results {
            let name = self.all_processes.iter()
                .find(|p| p.pid == *pid)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| format!("PID {}", pid));

            match res {
                Ok(_) => {
                    success_count += 1;
                    self.kill_history.push(KillRecord {
                        pid: *pid,
                        name,
                        killed_at: Instant::now(),
                        success: true,
                    });
                }
                Err(e) => {
                    error!("Erro ao finalizar PID {}: {}", pid, e);
                    self.kill_history.push(KillRecord {
                        pid: *pid,
                        name,
                        killed_at: Instant::now(),
                        success: false,
                    });
                }
            }
        }

        // Manter apenas os últimos 50 registros
        if self.kill_history.len() > 50 {
            self.kill_history = self.kill_history.split_off(self.kill_history.len() - 50);
        }

        self.set_status(format!(
            "Processos finalizados com sucesso: {} de {}",
            success_count,
            pids.len()
        ));

        self.refresh_metrics().await;
        self.refresh_processes();
        self.refresh_network();
    }

    async fn execute_create_snapshot(&mut self, vm_index: usize, snapshot_name: &str) {
        if let Some(vm) = self.vms.get(vm_index) {
            let path = vm.path.clone();
            let name = vm.name.clone();
            self.set_status(format!(
                "Criando snapshot '{}' na VM '{}'...",
                snapshot_name, name
            ));
            let res = self
                .vm_client
                .create_snapshot(&path, snapshot_name)
                .await;
            match res {
                Ok(_) => self.set_status(format!(
                    "Snapshot '{}' criado com sucesso na VM '{}'!",
                    snapshot_name, name
                )),
                Err(e) => self.set_status(format!("Erro ao criar snapshot: {}", e)),
            }
        }
    }

    async fn execute_add_vm(&mut self, alias: String, path_str: String) {
        if self.config.has_alias(&alias) {
            self.set_status(format!(
                "Já existe uma VM cadastrada com o alias '{}'.",
                alias
            ));
            return;
        }

        let path = PathBuf::from(&path_str);
        let entry = VmConfigEntry {
            alias: alias.clone(),
            name: None,
            path: path.clone(),
        };

        match AppConfig::append_vm_entry(&self.config_path, &entry) {
            Ok(_) => {
                self.config.vmware.vms.push(entry);
                self.vm_discovery.set_config(self.config.vmware.clone());
                let offline_note = if path.exists() { "" } else { " (offline no momento)" };
                self.set_status(format!(
                    "VM '{}' cadastrada com sucesso{}.",
                    alias, offline_note
                ));
                self.refresh_vms().await;
            }
            Err(e) => self.set_status(format!("Erro ao salvar VM no config.toml: {}", e)),
        }
    }
}
