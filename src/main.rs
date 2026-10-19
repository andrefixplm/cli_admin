use cli_admin::config::{AppConfig, VmConfigEntry};
use cli_admin::logger;
use cli_admin::modules::net::{self, NetProtoFilter};
use cli_admin::modules::process::killer::separar_protegidos;
use cli_admin::modules::process::{export, ProcessFilter, ProcessItem, ProcessKiller, ProcessShield};
use cli_admin::modules::support;
use cli_admin::modules::top::{
    GpuPdhCollector, NvidiaCollector, SortField, SystemMetricsManager,
};
use cli_admin::modules::vmware::{StartMode, StopMode, VmDiscovery, VmwareClient};
use cli_admin::tui;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "cli_admin_v3",
    author = "Andre PLM",
    version,
    about = "Ferramenta modular de administração de VMware Workstation, TOP Métricas, Process Killer e Rede/Portas",
    long_about = "CLI Admin é um utilitário de alta performance escrito em Rust para gerenciar máquinas virtuais do VMware Workstation, monitorar consumo de hardware (CPU, RAM, Disco, GPU e VRAM) e pesquisar/finalizar processos com interface interativa (TUI) e comandos rápidos scriptáveis."
)]
struct Cli {
    /// Caminho customizado para o arquivo de configuração (.toml)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Ativa o modo de depuração verboso (log detalhado)
    #[arg(short, long, global = true)]
    debug: bool,

    /// Força o modo interativo TUI (padrão se nenhum subcomando for passado)
    #[arg(short, long)]
    interactive: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Gerenciamento e controle de máquinas virtuais do VMware Workstation
    Vm(VmArgs),

    /// Exibe a lista TOP de processos ordenados por consumo de recursos
    Top(TopArgs),

    /// Pesquisa e finaliza processos (taskkill facilitado)
    Kill(KillArgs),

    /// Lista portas ativas e finaliza o processo dono de uma porta
    Net(NetArgs),

    /// Notas locais de campo (Markdown)
    Notes(NotesArgs),

    /// Diagnóstico de rede: ping, probe, mapa LAN, throughput, DNS
    Netdiag(NetdiagArgs),

    /// Suporte Windows: disco, snapshot de serviços, boot
    Winsys(WinsysArgs),

    /// Exporta a lista de processos (CSV ou JSON) com a linha de comando de cada um
    Export(ExportArgs),
}

#[derive(Args, Debug)]
struct ExportArgs {
    /// Filtro opcional por nome ou PID (mesma busca da aba Processos)
    query: Option<String>,

    /// Formato do relatório: csv ou json
    #[arg(short, long, default_value = "csv")]
    format: String,

    /// Arquivo de saída (padrão: processes_<timestamp>.<formato> no diretório atual)
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct VmArgs {
    #[command(subcommand)]
    action: VmAction,
}

#[derive(Subcommand, Debug)]
enum VmAction {
    /// Lista todas as máquinas virtuais encontradas e seu status atual
    List,

    /// Inicia uma máquina virtual
    Start {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
        /// Inicia com interface gráfica (por padrão inicia em segundo plano / nogui)
        #[arg(long)]
        gui: bool,
    },

    /// Desliga uma máquina virtual
    Stop {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
        /// Força o desligamento imediato (power off / hard) em vez de desligamento limpo
        #[arg(long)]
        hard: bool,
    },

    /// Pausa a execução de uma máquina virtual
    Pause {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
    },

    /// Retoma a execução de uma máquina virtual pausada
    Unpause {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
    },

    /// Suspende a execução de uma máquina virtual
    Suspend {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
        #[arg(long)]
        hard: bool,
    },

    /// Reinicia uma máquina virtual
    Reset {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
        #[arg(long)]
        hard: bool,
    },

    /// Gerenciamento de Snapshots da máquina virtual
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },

    /// Cadastra uma nova máquina virtual na lista (útil para VMs offline/em disco removível)
    Add {
        /// Apelido curto e único para identificar a VM (ex: tc-dcta)
        alias: String,
        /// Caminho completo para o arquivo .vmx (não precisa existir no momento do cadastro)
        path: PathBuf,
        /// Nome amigável opcional para exibição (padrão: nome do arquivo)
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum SnapshotAction {
    /// Lista os snapshots existentes na máquina virtual
    List {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
    },
    /// Cria um novo snapshot
    Create {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
        /// Nome do snapshot
        name: String,
    },
    /// Reverte a máquina virtual para um snapshot existente
    Revert {
        /// Alias, nome ou caminho do arquivo .vmx
        target: String,
        /// Nome do snapshot para o qual reverter
        name: String,
    },
}

#[derive(Args, Debug)]
struct TopArgs {
    /// Campo de ordenação: cpu, ram, io, gpu, vram
    #[arg(short, long)]
    sort: Option<String>,

    /// Quantidade de processos a exibir
    #[arg(short = 'n', long)]
    count: Option<usize>,
}

#[derive(Args, Debug)]
struct KillArgs {
    /// Termo de busca (nome do processo ou PID)
    query: String,

    /// Encerramento suave: sem /F. Padrão é forçado (/F), como antes
    #[arg(long)]
    soft: bool,

    /// Não finaliza os processos filhos. Padrão inclui a árvore (/T)
    #[arg(long)]
    sem_arvore: bool,

    /// Não solicita confirmação antes de finalizar processos
    #[arg(short = 'y', long)]
    yes: bool,

    /// Modo simulação: exibe os processos correspondentes sem finalizá-los
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct NetArgs {
    #[command(subcommand)]
    action: NetAction,
}
#[derive(Subcommand, Debug)]
enum NetAction {
    /// Lista as portas ativas (TCP/UDP) e os processos vinculados
    List {
        /// Filtro de protocolo: tcp, udp ou all
        #[arg(long)]
        proto: Option<String>,

        /// Filtro de estado (ex: LISTENING, ESTABLISHED)
        #[arg(long)]
        state: Option<String>,

        /// Busca textual (porta, processo, endereço, estado)
        #[arg(long)]
        query: Option<String>,

        /// Exibe somente sockets em escuta (TCP LISTENING + UDP)
        #[arg(long)]
        listening: bool,
    },

    /// Finaliza o(s) processo(s) dono(s) de uma porta local
    Kill {
        /// Porta local a liberar (ex: 8080)
        #[arg(long)]
        port: u16,

        /// Protocolo: tcp, udp ou all
        #[arg(long, default_value = "all")]
        proto: String,

        /// Encerramento suave: sem /F. Padrão é forçado (/F)
        #[arg(long)]
        soft: bool,

        /// Não finaliza os processos filhos. Padrão inclui a árvore (/T)
        #[arg(long)]
        sem_arvore: bool,

        /// Não solicita confirmação antes de finalizar processos
        #[arg(short = 'y', long)]
        yes: bool,

        /// Modo simulação: exibe os alvos sem finalizá-los
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args, Debug)]
struct NotesArgs {
    #[command(subcommand)]
    action: NotesAction,
}

#[derive(Subcommand, Debug)]
enum NotesAction {
    /// Cria uma nota de campo
    Add {
        /// Título da nota
        title: String,
        /// Corpo da nota (ou via stdin com pipe)
        #[arg(long)]
        body: Option<String>,
        /// Tags separadas por vírgula
        #[arg(long)]
        tags: Option<String>,
    },

    /// Lista notas (busca opcional)
    List {
        /// Busca em título, tags e corpo
        #[arg(long)]
        query: Option<String>,
    },

    /// Exibe uma nota pelo id (prefixo único basta)
    Show {
        /// Id ou prefixo do id
        id: String,
    },
}

#[derive(Args, Debug)]
struct NetdiagArgs {
    #[command(subcommand)]
    action: NetdiagAction,
}

#[derive(Subcommand, Debug)]
enum NetdiagAction {
    /// Série de ping com perda e min/avg/max
    Ping {
        /// Host ou IP
        host: String,
        /// Quantidade de pacotes
        #[arg(short = 'n', long)]
        count: Option<u32>,
        /// Exporta linha CSV de evidência para arquivo
        #[arg(long)]
        csv: Option<PathBuf>,
    },

    /// Testa se host:porta responde (tempo de connect)
    Probe {
        /// Host ou IP
        host: String,
        /// Porta
        port: u16,
    },

    /// Mapeia a LAN via tabela ARP (+probe opcional de portas)
    Scan {
        /// Varredura TCP ativa em subnet (ex: 192.168.1.0/24)
        #[arg(long)]
        subnet: Option<String>,
        /// Portas para probe (padrão: 22,80,135,139,443,445,3389)
        #[arg(long)]
        ports: Option<String>,
    },

    /// Orquestra teste de throughput com ntttcp (binário externo MS)
    Speed {
        /// Papel: sender ou receiver
        #[arg(long, default_value = "receiver")]
        role: String,
        /// IP do par
        #[arg(long)]
        peer: String,
        /// Duração em segundos
        #[arg(long, default_value_t = 60)]
        secs: u64,
        /// Threads
        #[arg(long, default_value_t = 4)]
        threads: u32,
    },

    /// Resolução DNS do host
    Dns {
        /// Host
        host: String,
    },
}

#[derive(Args, Debug)]
struct WinsysArgs {
    #[command(subcommand)]
    action: WinsysAction,
}

#[derive(Subcommand, Debug)]
enum WinsysAction {
    /// Top pastas por tamanho
    DiskTop {
        /// Diretório raiz (padrão: .)
        #[arg(long)]
        path: Option<PathBuf>,
        /// Quantidade
        #[arg(short = 'n', long)]
        count: Option<usize>,
    },

    /// Salva snapshot de serviços em JSON
    SvcSave {
        /// Arquivo de saída
        file: PathBuf,
    },

    /// Compara dois snapshots (adicionados/removidos/alterados)
    SvcDiff {
        /// Snapshot "antes"
        before: PathBuf,
        /// Snapshot "depois"
        after: PathBuf,
    },

    /// Auditoria de boot/login: Run keys + tarefas agendadas
    BootAudit,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Carregar configurações externalizadas
    let (config, config_path) = AppConfig::load(cli.config.as_deref())?;

    // Configurar sistema de logging
    let log_to_stdout = cli.command.is_some() && cli.debug;
    let _log_guard = logger::init_logger(
        &config.general.log_file,
        &config.general.log_level,
        log_to_stdout,
        cli.debug,
    );

    info!("Iniciando CLI Admin v{} com configuração: {:?}", env!("CARGO_PKG_VERSION"), config_path);

    // Se nenhum subcomando for fornecido ou se --interactive for passado, abrir o TUI
    match cli.command {
        None => {
            tui::run_tui(config, config_path).await?;
        }
        Some(cmd) => match cmd {
            Commands::Vm(args) => handle_vm_cli(config, &config_path, args).await?,
            Commands::Top(args) => handle_top_cli(config, args).await?,
            Commands::Kill(args) => handle_kill_cli(config, args).await?,
            Commands::Net(args) => handle_net_cli(config, args).await?,
            Commands::Notes(args) => handle_notes_cli(config, args).await?,
            Commands::Netdiag(args) => handle_netdiag_cli(config, args).await?,
            Commands::Winsys(args) => handle_winsys_cli(config, args).await?,
            Commands::Export(args) => handle_export_cli(config, args).await?,
        },
    }

    Ok(())
}

async fn handle_vm_cli(
    config: AppConfig,
    config_path: &std::path::Path,
    args: VmArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = VmwareClient::new(&config.vmware.vmrun_path);
    let discovery = VmDiscovery::new(config.vmware.clone(), client.clone());

    match args.action {
        VmAction::List => {
            println!("\n=== Inventário de Máquinas Virtuais VMware Workstation ===");
            let vms = discovery.discover_all().await;
            if vms.is_empty() {
                println!("Nenhuma máquina virtual encontrada nos diretórios configurados.");
                println!("Verifique a chave 'scan_directories' ou '[[vmware.vms]]' no arquivo config.toml.");
            } else {
                println!("{:<16} {:<32} {:<12} {}", "ALIAS", "NOME", "STATUS", "CAMINHO (.VMX)");
                println!("{:-<16} {:-<32} {:-<12} {:-<40}", "", "", "", "");
                for vm in vms {
                    let alias = vm.alias.as_deref().unwrap_or("-");
                    println!("{:<16} {:<32} {:<12} {}", alias, vm.name, vm.status, vm.path.display());
                }
            }
            println!();
        }
        VmAction::Start { target, gui } => {
            let path = discovery.resolve_vm_path(&target).await?;
            let mode = if gui { StartMode::Gui } else { StartMode::Nogui };
            println!("Iniciando VM '{}' ({}) no modo {}...", target, path.display(), mode.as_str());
            match client.start(&path, mode).await {
                Ok(_) => println!("✔ Máquina virtual iniciada com sucesso."),
                Err(e) => return Err(format!("✖ Erro ao iniciar máquina virtual: {}", e).into()),
            }
        }
        VmAction::Stop { target, hard } => {
            let path = discovery.resolve_vm_path(&target).await?;
            let mode = if hard { StopMode::Hard } else { StopMode::Soft };
            println!("Parando VM '{}' no modo {}...", target, mode.as_str());
            match client.stop(&path, mode).await {
                Ok(_) => println!("✔ Máquina virtual parada com sucesso."),
                Err(e) => return Err(format!("✖ Erro ao parar máquina virtual: {}", e).into()),
            }
        }
        VmAction::Pause { target } => {
            let path = discovery.resolve_vm_path(&target).await?;
            println!("Pausando VM '{}'...", target);
            match client.pause(&path).await {
                Ok(_) => println!("✔ Máquina virtual pausada com sucesso."),
                Err(e) => return Err(format!("✖ Erro ao pausar máquina virtual: {}", e).into()),
            }
        }
        VmAction::Unpause { target } => {
            let path = discovery.resolve_vm_path(&target).await?;
            println!("Retomando VM '{}'...", target);
            match client.unpause(&path).await {
                Ok(_) => println!("✔ Máquina virtual retomada com sucesso."),
                Err(e) => return Err(format!("✖ Erro ao retomar máquina virtual: {}", e).into()),
            }
        }
        VmAction::Suspend { target, hard } => {
            let path = discovery.resolve_vm_path(&target).await?;
            let mode = if hard { StopMode::Hard } else { StopMode::Soft };
            println!("Suspendendo VM '{}'...", target);
            match client.suspend(&path, mode).await {
                Ok(_) => println!("✔ Máquina virtual suspensa com sucesso."),
                Err(e) => return Err(format!("✖ Erro ao suspender máquina virtual: {}", e).into()),
            }
        }
        VmAction::Reset { target, hard } => {
            let path = discovery.resolve_vm_path(&target).await?;
            let mode = if hard { StopMode::Hard } else { StopMode::Soft };
            println!("Reiniciando VM '{}'...", target);
            match client.reset(&path, mode).await {
                Ok(_) => println!("✔ Máquina virtual reiniciada com sucesso."),
                Err(e) => return Err(format!("✖ Erro ao reiniciar máquina virtual: {}", e).into()),
            }
        }
        VmAction::Snapshot { action } => match action {
            SnapshotAction::List { target } => {
                let path = discovery.resolve_vm_path(&target).await?;
                println!("Listando snapshots da VM '{}'...", target);
                match client.list_snapshots(&path).await {
                    Ok(snapshots) => {
                        if snapshots.is_empty() {
                            println!("Nenhum snapshot encontrado para esta máquina virtual.");
                        } else {
                            println!("Total de snapshots: {}", snapshots.len());
                            for (i, s) in snapshots.iter().enumerate() {
                                println!("  [{}] {}", i + 1, s.name);
                            }
                        }
                    }
                    Err(e) => return Err(format!("✖ Erro ao listar snapshots: {}", e).into()),
                }
            }
            SnapshotAction::Create { target, name } => {
                let path = discovery.resolve_vm_path(&target).await?;
                println!("Criando snapshot '{}' na VM '{}'...", name, target);
                match client.create_snapshot(&path, &name).await {
                    Ok(_) => println!("✔ Snapshot criado com sucesso!"),
                    Err(e) => return Err(format!("✖ Erro ao criar snapshot: {}", e).into()),
                }
            }
            SnapshotAction::Revert { target, name } => {
                let path = discovery.resolve_vm_path(&target).await?;
                println!("Revertendo VM '{}' para snapshot '{}'...", target, name);
                match client.revert_snapshot(&path, &name).await {
                    Ok(_) => println!("✔ Máquina virtual revertida com sucesso!"),
                    Err(e) => return Err(format!("✖ Erro ao reverter snapshot: {}", e).into()),
                }
            }
        },
        VmAction::Add { alias, path, name } => {
            if config.has_alias(&alias) {
                eprintln!("✖ Já existe uma VM cadastrada com o alias '{}'.", alias);
                return Ok(());
            }

            let entry = VmConfigEntry {
                alias: alias.clone(),
                name,
                path: path.clone(),
            };

            match AppConfig::append_vm_entry(config_path, &entry) {
                Ok(_) => {
                    println!(
                        "✔ VM '{}' cadastrada com sucesso em {:?}.",
                        alias, config_path
                    );
                    if !path.exists() {
                        println!(
                            "  Aviso: o caminho informado não está acessível no momento (VM offline/disco desconectado). Ela aparecerá na lista com status OFFLINE."
                        );
                    }
                }
                Err(e) => return Err(format!("✖ Erro ao salvar configuração: {}", e).into()),
            }
        }
    }

    Ok(())
}

async fn handle_top_cli(
    config: AppConfig,
    args: TopArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let sort_field = match args.sort {
        Some(s) => s.parse::<SortField>()?,
        None => config.metrics.default_sort.parse::<SortField>()?,
    };

    let count = args.count.unwrap_or(config.metrics.top_process_count);

    println!("Coletando telemetria e métricas de desempenho...");

    let mut system_manager = SystemMetricsManager::new();
    let gpu_collector = GpuPdhCollector::new(config.metrics.enable_gpu_metrics);

    // Aguardar uma janela breve para cálculo de delta de CPU e carregamento de contadores PDH
    tokio::time::sleep(Duration::from_millis(600)).await;
    system_manager.refresh();

    let gpu_metrics = gpu_collector.get_metrics().await;
    let nvidia_info = if config.metrics.enable_nvml {
        NvidiaCollector::query_global_telemetry().await
    } else {
        None
    };

    let overview = system_manager.get_overview(nvidia_info.as_ref(), &gpu_metrics);
    let top_procs = system_manager.get_top_processes(sort_field, count, &gpu_metrics);

    println!("\n=== RESUMO GERAL DO SISTEMA ===");
    let ram_used_gb = overview.used_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let ram_total_gb = overview.total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let vram_used_gb = overview.used_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let vram_total_gb = overview.total_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    println!("CPU Global : {:.1}%", overview.cpu_pct);
    println!("Memória RAM: {:.2} / {:.2} GB ({:.1}%)", ram_used_gb, ram_total_gb, overview.ram_usage_pct());
    println!("Memória VRAM: {:.2} / {:.2} GB ({:.1}%)", vram_used_gb, vram_total_gb, overview.vram_usage_pct());
    if let Some(gpu_name) = &overview.gpu_name {
        let temp = overview.gpu_temp_c.map(|t| format!(" | Temp: {}°C", t)).unwrap_or_default();
        let pwr = overview.gpu_power_watts.map(|w| format!(" | Consumo: {:.1}W", w)).unwrap_or_default();
        println!("Adaptador GPU: {}{}{}", gpu_name, temp, pwr);
    }

    println!("\n=== TOP PROCESSOS (ORDENADO POR {}) ===", sort_field.as_str().to_uppercase());
    println!(
        "{:<8} {:<30} {:<10} {:<12} {:<14} {:<10} {:<12}",
        "PID", "PROCESSO", "CPU %", "RAM (MB)", "DISCO (KB/s)", "GPU %", "VRAM (MB)"
    );
    println!("{:-<8} {:-<30} {:-<10} {:-<12} {:-<14} {:-<10} {:-<12}", "", "", "", "", "", "", "");

    for p in top_procs {
        println!(
            "{:<8} {:<30} {:<10.1} {:<12.1} {:<14.1} {:<10.1} {:<12.1}",
            p.pid,
            cli_admin::texto::truncar(&p.name, 28),
            p.cpu_pct,
            p.memory_mb(),
            p.total_disk_io_kb_sec(),
            p.gpu_pct,
            p.vram_mb()
        );
    }
    println!();

    Ok(())
}

async fn handle_export_cli(
    config: AppConfig,
    args: ExportArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let format = args.format.to_lowercase();
    if format != "csv" && format != "json" {
        return Err(format!("Formato '{}' não suportado. Use csv ou json.", args.format).into());
    }

    println!("Coletando lista de processos...");

    let mut system_manager = SystemMetricsManager::new();
    tokio::time::sleep(Duration::from_millis(600)).await;
    system_manager.refresh();

    let shield = ProcessShield::new(&config.security.protected_processes);
    let dummy_gpu = std::collections::HashMap::new();
    let items: Vec<ProcessItem> = system_manager
        .get_all_processes(&dummy_gpu)
        .iter()
        .map(|p| ProcessItem::from_resource(p, shield.is_protected(&p.name, p.pid), false))
        .collect();

    let query = args.query.unwrap_or_default();
    let mut selected: Vec<ProcessItem> = ProcessFilter::new()
        .filter(&query, &items)
        .into_iter()
        .cloned()
        .collect();
    selected.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));

    let content = export::render(&selected, &format)?;
    let path = match args.output {
        Some(path) => path.to_string_lossy().to_string(),
        None => export::default_filename(&format),
    };
    export::write_file(&path, &content)?;

    println!("Exportados {} processos para {}", selected.len(), path);

    let sem_cmd = selected.iter().filter(|p| p.cmd_line.is_empty()).count();
    if sem_cmd > 0 {
        println!(
            "Aviso: {} processo(s) sem linha de comando. Execute como Administrador para ler processos de outros usuários e do sistema.",
            sem_cmd
        );
    }

    Ok(())
}

/// Pergunta `pergunta (s/N)` e devolve `true` só para uma resposta afirmativa.
/// Única implementação da confirmação, usada por kill e net kill.
fn confirmar(pergunta: &str) -> std::io::Result<bool> {
    use std::io::{self, Write};
    print!("{pergunta} (s/N): ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(input.trim().to_lowercase().as_str(), "s" | "sim" | "y" | "yes"))
}

async fn handle_kill_cli(
    config: AppConfig,
    args: KillArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut system_manager = SystemMetricsManager::new();
    system_manager.refresh();

    let shield = ProcessShield::new(&config.security.protected_processes);
    let filter = ProcessFilter::new();

    let dummy_gpu = std::collections::HashMap::new();
    let procs = system_manager.get_top_processes(SortField::Ram, 0, &dummy_gpu);

    let items: Vec<ProcessItem> = procs
        .into_iter()
        .map(|p| {
            let is_protected = shield.is_protected(&p.name, p.pid);
            ProcessItem::from_resource(&p, is_protected, false)
        })
        .collect();

    let matches = filter.filter(&args.query, &items);

    if matches.is_empty() {
        println!("Nenhum processo encontrado correspondente ao filtro '{}'.", args.query);
        return Ok(());
    }

    println!("\nProcessos encontrados correspondentes a '{}':", args.query);
    println!("{:<8} {:<14} {:<32} {:<10}", "PID", "STATUS", "NOME", "RAM (MB)");
    println!("{:-<8} {:-<14} {:-<32} {:-<10}", "", "", "", "");

    for p in &matches {
        let status = if p.is_protected { "🛡️ CRÍTICO" } else { "NORMAL" };
        println!("{:<8} {:<14} {:<32} {:<10.1}", p.pid, status, p.name, p.memory_mb());
    }

    println!();

    // Escudo (A-02): protegidos nunca entram na lista de encerramento, nem com -y.
    let alvos: Vec<(u32, bool)> = matches.iter().map(|p| (p.pid, p.is_protected)).collect();
    let (pids_to_kill, bloqueados) = separar_protegidos(&alvos);
    if !bloqueados.is_empty() {
        eprintln!(
            "🚨 {} processo(s) protegido(s) IGNORADO(S), não serão encerrados: {:?}",
            bloqueados.len(),
            bloqueados
        );
    }

    if pids_to_kill.is_empty() {
        println!("Nenhum processo elegível para encerrar.");
        return Ok(());
    }

    if args.dry_run {
        println!("\n✔ [DRY-RUN] Simulação concluída com sucesso. Nenhum processo foi finalizado.");
        return Ok(());
    }

    if !args.yes && !confirmar(&format!("Deseja realmente finalizar os {} processo(s) acima?", pids_to_kill.len()))? {
        println!("Operação cancelada pelo usuário.");
        return Ok(());
    }

    println!("Finalizando processos...");
    let results = ProcessKiller::kill_batch(&pids_to_kill, !args.soft, !args.sem_arvore).await;

    for (pid, res) in results {
        match res {
            Ok(msg) => println!("✔ PID {}: {}", pid, msg),
            Err(err) => eprintln!("✖ PID {}: Erro: {}", pid, err),
        }
    }

    Ok(())
}

fn parse_net_proto_filter(s: &str) -> Result<NetProtoFilter, String> {
    match s.to_lowercase().as_str() {
        "tcp" => Ok(NetProtoFilter::Tcp),
        "udp" => Ok(NetProtoFilter::Udp),
        "all" | "todos" => Ok(NetProtoFilter::All),
        other => Err(format!(
            "Protocolo desconhecido: {}. Opções: tcp, udp, all",
            other
        )),
    }
}

async fn handle_net_cli(
    config: AppConfig,
    args: NetArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut system_manager = SystemMetricsManager::new();
    system_manager.refresh();

    let shield = ProcessShield::new(&config.security.protected_processes);
    let dummy_gpu = std::collections::HashMap::new();
    let pid_names: std::collections::HashMap<u32, String> = system_manager
        .get_all_processes(&dummy_gpu)
        .into_iter()
        .map(|p| (p.pid, p.name))
        .collect();

    let conns = net::scan(&pid_names, &shield);

    match args.action {
        NetAction::List {
            proto,
            state,
            query,
            listening,
        } => {
            let proto_filter = match proto {
                Some(s) => parse_net_proto_filter(&s)?,
                None => NetProtoFilter::All,
            };
            let q = query.unwrap_or_default();
            let mut filtered = net::filter_connections(&q, proto_filter, listening, &conns)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>();
            if let Some(st) = state {
                let st_lower = st.to_lowercase();
                filtered.retain(|c| c.state_label().to_lowercase().contains(&st_lower));
            }

            if filtered.is_empty() {
                println!("Nenhuma porta encontrada para os filtros informados.");
                return Ok(());
            }

            println!("\n=== PORTAS ATIVAS (TCP/UDP) ===");
            println!(
                "{:<6} {:<24} {:<24} {:<12} {:<8} {:<28} {:<10}",
                "PROTO", "LOCAL", "REMOTO", "ESTADO", "PID", "PROCESSO", "STATUS"
            );
            println!(
                "{:-<6} {:-<24} {:-<24} {:-<12} {:-<8} {:-<28} {:-<10}",
                "", "", "", "", "", "", ""
            );
            for c in &filtered {
                let status = if c.is_protected { "🛡️ CRÍTICO" } else { "NORMAL" };
                let name = cli_admin::texto::truncar(&c.process_name, 26);
                println!(
                    "{:<6} {:<24} {:<24} {:<12} {:<8} {:<28} {:<10}",
                    c.proto_label(),
                    c.local_endpoint(),
                    c.remote_endpoint(),
                    c.state_label(),
                    c.pid,
                    name,
                    status
                );
            }
            println!("\nTotal: {} socket(s).", filtered.len());
            println!();
        }
        NetAction::Kill {
            port,
            proto,
            soft,
            sem_arvore,
            yes,
            dry_run,
        } => {
            let proto_filter = parse_net_proto_filter(&proto)?;
            let targets: Vec<_> = conns
                .iter()
                .filter(|c| {
                    c.local_port == port
                        && match proto_filter {
                            NetProtoFilter::All => true,
                            NetProtoFilter::Tcp => c.proto == net::NetProto::Tcp,
                            NetProtoFilter::Udp => c.proto == net::NetProto::Udp,
                        }
                })
                .collect();

            if targets.is_empty() {
                println!(
                    "Nenhum socket encontrado na porta {} (filtro: {}).",
                    port,
                    proto_filter.label()
                );
                return Ok(());
            }

            println!("\nSockets na porta {}:", port);
            println!(
                "{:<6} {:<24} {:<12} {:<8} {:<28} {:<10}",
                "PROTO", "LOCAL", "ESTADO", "PID", "PROCESSO", "STATUS"
            );
            for c in &targets {
                let status = if c.is_protected { "🛡️ CRÍTICO" } else { "NORMAL" };
                println!(
                    "{:<6} {:<24} {:<12} {:<8} {:<28} {:<10}",
                    c.proto_label(),
                    c.local_endpoint(),
                    c.state_label(),
                    c.pid,
                    c.process_name,
                    status
                );
            }
            println!();

            // PIDs distintos donos da porta; protegidos ficam de fora (A-02)
            let mut seen = std::collections::HashSet::new();
            let alvos: Vec<(u32, bool)> = targets
                .iter()
                .filter(|c| seen.insert(c.pid))
                .map(|c| (c.pid, c.is_protected))
                .collect();
            let (pids_to_kill, bloqueados) = separar_protegidos(&alvos);
            if !bloqueados.is_empty() {
                eprintln!(
                    "🚨 {} processo(s) protegido(s) IGNORADO(S), não serão encerrados: {:?}",
                    bloqueados.len(),
                    bloqueados
                );
            }

            if pids_to_kill.is_empty() {
                println!("Nenhum processo elegível para encerrar na porta {}.", port);
                return Ok(());
            }

            if dry_run {
                println!("✔ [DRY-RUN] Simulação concluída. Nenhum processo foi finalizado.");
                return Ok(());
            }

            if !yes
                && !confirmar(&format!(
                    "Deseja realmente finalizar {} processo(s) dono(s) da porta {}?",
                    pids_to_kill.len(),
                    port
                ))?
            {
                println!("Operação cancelada pelo usuário.");
                return Ok(());
            }

            println!("Finalizando processos...");
            let results = ProcessKiller::kill_batch(&pids_to_kill, !soft, !sem_arvore).await;
            for (pid, res) in results {
                match res {
                    Ok(msg) => println!("✔ PID {}: {}", pid, msg),
                    Err(err) => eprintln!("✖ PID {}: Erro: {}", pid, err),
                }
            }
        }
    }

    Ok(())
}

async fn handle_notes_cli(
    config: AppConfig,
    args: NotesArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = config.notes.resolved_dir();
    match args.action {
        NotesAction::Add { title, body, tags } => {
            let tag_list: Vec<String> = tags
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let text = match body {
                Some(b) => b,
                None => {
                    // Permite pipe: echo "..." | cli_admin notes add "titulo"
                    use std::io::{self, IsTerminal, Read};
                    if io::stdin().is_terminal() {
                        String::new()
                    } else {
                        let mut s = String::new();
                        io::stdin().read_to_string(&mut s).unwrap_or(0);
                        s.trim().to_string()
                    }
                }
            };
            match support::add_note(&dir, &title, &tag_list, &text) {
                Ok(meta) => println!("✔ Nota criada: {} ({})", meta.id, meta.path.display()),
                Err(e) => eprintln!("✖ {}", e),
            }
        }
        NotesAction::List { query } => {
            let notes = support::list_notes(&dir, &query.unwrap_or_default());
            if notes.is_empty() {
                println!("Nenhuma nota em {}.", dir.display());
                return Ok(());
            }
            for n in notes {
                let tags = if n.tags.is_empty() { String::new() } else { format!(" [{}]", n.tags.join(",")) };
                println!("{:<24} {}{}{}", n.id, n.title, tags, if n.created.is_empty() { String::new() } else { format!("  ({})", n.created) });
            }
        }
        NotesAction::Show { id } => {
            match support::read_note(&dir, &id) {
                Ok(body) => println!("{}", body),
                Err(e) => eprintln!("✖ {}", e),
            }
        }
    }
    Ok(())
}

fn parse_port_list(s: &str) -> Result<Vec<u16>, String> {
    s.split(',')
        .map(|p| {
            p.trim()
                .parse::<u16>()
                .map_err(|_| format!("Porta inválida: '{}'", p.trim()))
        })
        .collect()
}

async fn handle_netdiag_cli(
    config: AppConfig,
    args: NetdiagArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        NetdiagAction::Ping { host, count, csv } => {
            let n = count.unwrap_or(config.netdiag.ping_count);
            let stats = support::ping_series(&host, n).await;
            println!("\n=== PING {} ===", host);
            println!("{}", stats.summary());
            if let Some(path) = csv {
                let row = support::ping_csv_row(&stats);
                let mut content = String::from("quando,host,enviados,perda_pct,min_ms,avg_ms,max_ms\n");
                content.push_str(&row);
                content.push('\n');
                let existed = path.exists();
                if existed {
                    use std::io::Write;
                    let mut f = std::fs::OpenOptions::new().append(true).open(&path)?;
                    writeln!(f, "{}", row)?;
                } else {
                    std::fs::write(&path, content)?;
                }
                println!("Evidência anexada em: {}", path.display());
            }
            println!();
        }
        NetdiagAction::Probe { host, port } => {
            let r = support::probe_port(&host, port, config.netdiag.probe_timeout_ms).await;
            if r.open {
                println!("✔ {}:{} ABERTA ({} ms)", r.host, r.port, r.elapsed_ms);
            } else {
                println!("✖ {}:{} FECHADA/sem resposta ({} ms)", r.host, r.port, r.elapsed_ms);
            }
        }
        NetdiagAction::Scan { subnet, ports } => {
            if let Some(cidr) = subnet {
                let plist = match ports {
                    Some(p) => parse_port_list(&p)?,
                    None => vec![22, 80, 135, 139, 443, 445, 3389],
                };
                println!("Varrendo {} nas portas {:?}...", cidr, plist);
                match support::sweep_subnet(&cidr, &plist, 500, config.netdiag.sweep_concurrency).await {
                    Ok(open) if open.is_empty() => println!("Nenhuma porta aberta encontrada."),
                    Ok(open) => {
                        println!("\n=== PORTAS ABERTAS ===");
                        for (ip, port) in open {
                            println!("  {}:{}", ip, port);
                        }
                    }
                    Err(e) => eprintln!("✖ {}", e),
                }
            } else {
                let entries = support::read_arp_table();
                if entries.is_empty() {
                    println!("Tabela ARP vazia ou indisponível.");
                    return Ok(());
                }
                println!("\n=== MAPA DA LAN (ARP) ===");
                println!("{:<18} {:<20} {:<10}", "IP", "MAC", "TIPO");
                for e in &entries {
                    println!("{:<18} {:<20} {:<10}", e.ip, e.mac, support::arp_type_label(e.entry_type));
                }
                println!("\nTotal: {} entrada(s).", entries.len());
                if let Some(p) = ports {
                    let plist = parse_port_list(&p)?;
                    println!("Probe nas portas {:?}...", plist);
                    for e in &entries {
                        for port in &plist {
                            let r = support::probe_port(&e.ip, *port, 500).await;
                            if r.open {
                                println!("  ✔ {}:{} ({})", e.ip, port, e.mac);
                            }
                        }
                    }
                }
            }
            println!();
        }
        NetdiagAction::Speed { role, peer, secs, threads } => {
            println!("\n=== TESTE DE THROUGHPUT (ntttcp) ===");
            match support::discover_ntttcp(config.netdiag.ntttcp_path.as_deref()) {
                Some(exe) => {
                    println!("Binário: {}", exe.display());
                    println!("Execute no {}:", if role.to_lowercase().starts_with('r') { "RECEPTOR" } else { "TRANSMISSOR" });
                }
                None => {
                    println!("ntttcp.exe não encontrado (baixe em github.com/microsoft/ntttcp).");
                    println!("Comandos prontos — execute receptor e transmissor:");
                }
            }
            for cmd in support::ntttcp_commands(&role, &peer, secs, threads) {
                println!("  {}", cmd);
            }
            println!("\nDica: rode receptor e transmissor com o mesmo -t. iperf3 NÃO é recomendado no Windows.");
            println!();
        }
        NetdiagAction::Dns { host } => {
            let ips = support::resolve_host(&host).await;
            if ips.is_empty() {
                println!("✖ {} não resolveu.", host);
            } else {
                println!("{} resolve para:", host);
                for ip in ips {
                    println!("  {}", ip);
                }
            }
        }
    }
    Ok(())
}

async fn handle_winsys_cli(
    config: AppConfig,
    args: WinsysArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.action {
        WinsysAction::DiskTop { path, count } => {
            let root = path.unwrap_or_else(|| PathBuf::from("."));
            let n = count.unwrap_or(config.support.disk_top_n);
            println!("Calculando tamanhos em {} ...", root.display());
            let entries = support::disk_top(&root, n);
            println!("\n=== TOP PASTAS ({}) ===", root.display());
            for e in &entries {
                println!("  {:<12} {}", support::bytes_to_human(e.bytes), e.path.display());
            }
            println!();
        }
        WinsysAction::SvcSave { file } => {
            let snap = support::snapshot_services()?;
            std::fs::write(&file, serde_json::to_string_pretty(&snap)?)?;
            println!("✔ Snapshot de {} serviços salvo em {}.", snap.len(), file.display());
        }
        WinsysAction::SvcDiff { before, after } => {
            let b: Vec<support::SvcSnapshot> = serde_json::from_str(&std::fs::read_to_string(&before)?)?;
            let a: Vec<support::SvcSnapshot> = serde_json::from_str(&std::fs::read_to_string(&after)?)?;
            let diffs = support::diff_snapshots(&b, &a);
            if diffs.is_empty() {
                println!("✔ Nenhuma diferença entre snapshots.");
                return Ok(());
            }
            println!("\n=== DIFF DE SERVIÇOS ===");
            for d in &diffs {
                let mark = match d.kind {
                    support::SvcDiffKind::Added => "[+]",
                    support::SvcDiffKind::Removed => "[-]",
                    support::SvcDiffKind::Changed => "[~]",
                };
                println!("{} {:<40} {}", mark, d.name, d.detail);
            }
            println!();
        }
        WinsysAction::BootAudit => {
            println!("\n=== RUN KEYS ===");
            for item in support::boot_run_keys() {
                println!("[{}] {} = {}", item.source, item.name, item.command);
            }
            println!("\n=== TAREFAS AGENDADAS (prontas/executando) ===");
            for item in support::boot_scheduled_tasks().await {
                println!("[{}] {} ({})", item.source, item.name, item.command);
            }
            println!();
        }
    }
    Ok(())
}
