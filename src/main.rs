use cli_admin::config::{AppConfig, VmConfigEntry};
use cli_admin::logger;
use cli_admin::modules::process::{ProcessFilter, ProcessItem, ProcessKiller, ProcessShield};
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
    name = "cli_admin",
    author = "Andre PLM",
    version = "1.0.0",
    about = "Ferramenta modular de administração de VMware Workstation, TOP Métricas e Process Killer",
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

    /// Força o encerramento imediato (/F)
    #[arg(short, long, default_value_t = true)]
    force: bool,

    /// Finaliza a árvore de processos filhos (/T)
    #[arg(short, long, default_value_t = true)]
    tree: bool,

    /// Não solicita confirmação antes de finalizar processos
    #[arg(short = 'y', long)]
    yes: bool,

    /// Modo simulação: exibe os processos correspondentes sem finalizá-los
    #[arg(long)]
    dry_run: bool,
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

    info!("Iniciando CLI Admin v1.0.0 com configuração: {:?}", config_path);

    // Se nenhum subcomando for fornecido ou se --interactive for passado, abrir o TUI
    match cli.command {
        None => {
            tui::run_tui(config, config_path).await?;
        }
        Some(cmd) => match cmd {
            Commands::Vm(args) => handle_vm_cli(config, &config_path, args).await?,
            Commands::Top(args) => handle_top_cli(config, args).await?,
            Commands::Kill(args) => handle_kill_cli(config, args).await?,
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
                Err(e) => eprintln!("✖ Erro ao iniciar máquina virtual: {}", e),
            }
        }
        VmAction::Stop { target, hard } => {
            let path = discovery.resolve_vm_path(&target).await?;
            let mode = if hard { StopMode::Hard } else { StopMode::Soft };
            println!("Parando VM '{}' no modo {}...", target, mode.as_str());
            match client.stop(&path, mode).await {
                Ok(_) => println!("✔ Máquina virtual parada com sucesso."),
                Err(e) => eprintln!("✖ Erro ao parar máquina virtual: {}", e),
            }
        }
        VmAction::Pause { target } => {
            let path = discovery.resolve_vm_path(&target).await?;
            println!("Pausando VM '{}'...", target);
            match client.pause(&path).await {
                Ok(_) => println!("✔ Máquina virtual pausada com sucesso."),
                Err(e) => eprintln!("✖ Erro ao pausar máquina virtual: {}", e),
            }
        }
        VmAction::Unpause { target } => {
            let path = discovery.resolve_vm_path(&target).await?;
            println!("Retomando VM '{}'...", target);
            match client.unpause(&path).await {
                Ok(_) => println!("✔ Máquina virtual retomada com sucesso."),
                Err(e) => eprintln!("✖ Erro ao retomar máquina virtual: {}", e),
            }
        }
        VmAction::Suspend { target, hard } => {
            let path = discovery.resolve_vm_path(&target).await?;
            let mode = if hard { StopMode::Hard } else { StopMode::Soft };
            println!("Suspendendo VM '{}'...", target);
            match client.suspend(&path, mode).await {
                Ok(_) => println!("✔ Máquina virtual suspensa com sucesso."),
                Err(e) => eprintln!("✖ Erro ao suspender máquina virtual: {}", e),
            }
        }
        VmAction::Reset { target, hard } => {
            let path = discovery.resolve_vm_path(&target).await?;
            let mode = if hard { StopMode::Hard } else { StopMode::Soft };
            println!("Reiniciando VM '{}'...", target);
            match client.reset(&path, mode).await {
                Ok(_) => println!("✔ Máquina virtual reiniciada com sucesso."),
                Err(e) => eprintln!("✖ Erro ao reiniciar máquina virtual: {}", e),
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
                    Err(e) => eprintln!("✖ Erro ao listar snapshots: {}", e),
                }
            }
            SnapshotAction::Create { target, name } => {
                let path = discovery.resolve_vm_path(&target).await?;
                println!("Criando snapshot '{}' na VM '{}'...", name, target);
                match client.create_snapshot(&path, &name).await {
                    Ok(_) => println!("✔ Snapshot criado com sucesso!"),
                    Err(e) => eprintln!("✖ Erro ao criar snapshot: {}", e),
                }
            }
            SnapshotAction::Revert { target, name } => {
                let path = discovery.resolve_vm_path(&target).await?;
                println!("Revertendo VM '{}' para snapshot '{}'...", target, name);
                match client.revert_snapshot(&path, &name).await {
                    Ok(_) => println!("✔ Máquina virtual revertida com sucesso!"),
                    Err(e) => eprintln!("✖ Erro ao reverter snapshot: {}", e),
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
                Err(e) => eprintln!("✖ Erro ao salvar configuração: {}", e),
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
            if p.name.len() > 28 { &p.name[..28] } else { &p.name },
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
            ProcessItem {
                pid: p.pid,
                name: p.name,
                cpu_pct: p.cpu_pct,
                memory_bytes: p.memory_bytes,
                is_protected,
                selected: false,
                exe_path: p.exe_path,
                user: p.user,
                start_time: p.start_time,
                threads: p.threads,
                parent_pid: p.parent_pid,
                gpu_pct: p.gpu_pct,
                vram_bytes: p.vram_bytes,
                disk_read_bytes_sec: p.disk_read_bytes_sec,
                disk_write_bytes_sec: p.disk_write_bytes_sec,
            }
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

    let mut pids_to_kill = Vec::new();
    let mut has_critical = false;

    for p in matches {
        pids_to_kill.push(p.pid);
        let status = if p.is_protected {
            has_critical = true;
            "🛡️ CRÍTICO"
        } else {
            "NORMAL"
        };
        println!("{:<8} {:<14} {:<32} {:<10.1}", p.pid, status, p.name, p.memory_mb());
    }

    println!();

    if has_critical {
        eprintln!("🚨 ALERTA: A lista contém processos vitais do Windows protegidos por configuração!");
    }

    if args.dry_run {
        println!("\n✔ [DRY-RUN] Simulação concluída com sucesso. Nenhum processo foi finalizado.");
        return Ok(());
    }

    if !args.yes {
        print!("Deseja realmente finalizar os {} processo(s) acima? (s/N): ", pids_to_kill.len());
        use std::io::{self, Write};
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed = input.trim().to_lowercase();
        if trimmed != "s" && trimmed != "sim" && trimmed != "y" && trimmed != "yes" {
            println!("Operação cancelada pelo usuário.");
            return Ok(());
        }
    }

    println!("Finalizando processos...");
    let results = ProcessKiller::kill_batch(&pids_to_kill, args.force, args.tree).await;

    for (pid, res) in results {
        match res {
            Ok(msg) => println!("✔ PID {}: {}", pid, msg),
            Err(err) => eprintln!("✖ PID {}: Erro: {}", pid, err),
        }
    }

    Ok(())
}
