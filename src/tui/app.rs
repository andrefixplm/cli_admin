use crate::config::{AppConfig, VmConfigEntry};
use crate::modules::process::{
    export_processes, ProcessFilter, ProcessItem, ProcessKiller, ProcessShield,
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
}

impl ActiveTab {
    pub fn next(&self) -> Self {
        match self {
            ActiveTab::Vmware => ActiveTab::TopMetrics,
            ActiveTab::TopMetrics => ActiveTab::ProcessManager,
            ActiveTab::ProcessManager => ActiveTab::Vmware,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            ActiveTab::Vmware => ActiveTab::ProcessManager,
            ActiveTab::TopMetrics => ActiveTab::Vmware,
            ActiveTab::ProcessManager => ActiveTab::TopMetrics,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    SearchInput,
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
    pub proc_table_state: TableState,

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
            config,
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
            proc_table_state: TableState::default(),
            kill_history: Vec::new(),
        };

        app.refresh_vms().await;
        app.refresh_metrics().await;
        app.refresh_processes();

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
                cmd_line: proc.cmd_line.clone(),
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

        // Ordenar por nome para facilitar busca visual
        items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

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
                    self.search_query.pop();
                    self.apply_process_filter();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.apply_process_filter();
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
            _ => {}
        }

        // Atalhos específicos da aba ativa
        match self.active_tab {
            ActiveTab::Vmware => self.handle_vm_key(key).await,
            ActiveTab::TopMetrics => self.handle_top_key(key).await,
            ActiveTab::ProcessManager => self.handle_process_key(key).await,
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
            _ => {}
        }
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
                self.all_processes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
                self.apply_process_filter();
                self.set_status("Ordenado por nome");
            }
            // Ordenar por CPU
            KeyCode::Char('C') => {
                self.all_processes.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
                self.apply_process_filter();
                self.set_status("Ordenado por CPU%");
            }
            // Ordenar por Memória
            KeyCode::Char('M') => {
                self.all_processes.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
                self.apply_process_filter();
                self.set_status("Ordenado por Memória");
            }
            // Ordenar por VRAM
            KeyCode::Char('V') => {
                self.all_processes.sort_by(|a, b| b.vram_bytes.cmp(&a.vram_bytes));
                self.apply_process_filter();
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
        export_processes(&self.filtered_processes, format, None)
            .map(|path| path.display().to_string())
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
