# Especificação Técnica de Design / Technical Design Specification
**Projeto / Project:** CLI Admin — Rust High-Performance Multi-Tool & Interactive TUI
**Data / Date:** 2026-09-04
**Autor / Author:** Antigravity (Pair Programming with Andre)
**Versão / Version:** 1.0.0

---

## 1. Visão Geral / Overview

### 1.1 Português (PT-BR)
O **CLI Admin** é uma aplicação utilitária modular de alta performance escrita em **Rust**, desenvolvida para gerenciar máquinas virtuais do VMware Workstation, monitorar recursos de hardware em tempo real (CPU, RAM, Disco I/O, GPU e VRAM por processo) e fornecer um gerenciador de processos interativo com busca fuzzy e finalização segura (*taskkill*). 
A ferramenta opera em dois modos:
1. **Modo CLI Não-Interativo:** Comandos rápidos e scriptáveis (ex: `admin vm start <alias>`, `admin top --sort vram`, `admin kill <filtro>`).
2. **Modo TUI Interativo (Terminal User Interface):** Um painel visual completo construído com `ratatui` e `crossterm`, organizado em abas com suporte total a setas direcionais, atalhos rápidos (`j`/`k`, `Tab`, `Espaço`) e filtro de busca dinâmico em tempo real.

Todas as configurações (caminho do `vmrun.exe`, diretórios de busca de VMs, aliases, taxas de atualização e processos protegidos) são totalmente externalizadas em arquivo `config.toml` hierárquico com geração automática de template caso o arquivo não exista.

### 1.2 English (EN-US)
**CLI Admin** is a modular, high-performance utility application written in **Rust**, designed to manage VMware Workstation virtual machines, monitor hardware resource usage in real time (per-process CPU, RAM, Disk I/O, GPU, and VRAM), and provide an interactive process manager with fuzzy filtering and safe termination (*taskkill*).
The application operates in two distinct modes:
1. **Non-Interactive CLI Mode:** Fast, scriptable commands for automation (e.g., `admin vm start <alias>`, `admin top --sort vram`, `admin kill <filter>`).
2. **Interactive TUI Mode (Terminal User Interface):** A comprehensive visual dashboard built with `ratatui` and `crossterm`, structured in tabs with full arrow key navigation, keyboard shortcuts (`j`/`k`, `Tab`, `Space`), and real-time dynamic search filtering.

All configurations (`vmrun.exe` path, VM scan directories, aliases, refresh rates, and protected processes) are completely externalized in a hierarchical `config.toml` file, with automatic generation of a default template if absent.

---

## 2. Arquitetura do Sistema / System Architecture

```
                                  +---------------------------------------+
                                  |            admin.exe (Rust)           |
                                  +---------------------------------------+
                                                      |
                  +-----------------------------------+-----------------------------------+
                  |                                   |                                   |
                  v                                   v                                   v
       [ CLI Dispatcher (clap) ]           [ TUI Engine (ratatui) ]            [ Core Engine & Config ]
                  |                                   |                                   |
                  +-----------------+                 |                 +-----------------+
                                    |                 |                 |
                                    v                 v                 v
                 +-------------------------------------------------------------+
                 |                     Subsystem Modules                       |
                 |                                                             |
                 |  1. VMware Workstation Controller (`modules::vmware`)       |
                 |     - vmrun CLI wrapper (start, stop, pause, snapshots)     |
                 |     - Hybrid discovery (.vmx scan + alias mapping)          |
                 |                                                             |
                 |  2. TOP Metrics Engine (`modules::top`)                     |
                 |     - sysinfo (CPU, RAM, Disk I/O per PID)                  |
                 |     - Windows PDH / DXGI (GPU Engine % & VRAM per PID)      |
                 |     - NVML Auto-detection (NVIDIA Temp, Power, VRAM)        |
                 |                                                             |
                 |  3. Process Manager & Killer (`modules::process`)           |
                 |     - Win32 / sysinfo process listing                       |
                 |     - Fuzzy search & multi-select tagging                   |
                 |     - Safe kill (protected process shield & tree kill)      |
                 +-------------------------------------------------------------+
                                    |                 |                 |
                                    v                 v                 v
                 [ External Config: config.toml ]  [ Logging: admin.log ]  [ Windows OS / Drivers ]
```

---

## 3. Especificação dos Módulos / Module Specifications

### 3.1 Módulo 1: VMware Workstation Controller (`modules::vmware`)

#### PT-BR
- **Objetivo:** Orquestrar máquinas virtuais registradas e locais sem depender da interface gráfica do VMware.
- **Integração:** Envoltório (*wrapper*) assíncrono para o binário `vmrun.exe` da VMware (localizado via `config.toml` ou autodetectado em `C:\Program Files (x86)\VMware\VMware Workstation\vmrun.exe`).
- **Descoberta Híbrida:**
  1. Varredura de diretórios configurados no `config.toml` (ex: `D:\VMs`) procurando recursivamente por arquivos `*.vmx`.
  2. Mapeamento de apelidos amigáveis definidos no arquivo (ex: `tc14` -> `D:\VMs\TC14_PROD\TC14.vmx`).
  3. Consulta do estado em tempo de execução via `vmrun -T ws list` para identificar VMs ativas (*Running*), pausadas ou paradas.
- **Operações Suportadas:**
  - `start <alias|path>`: Iniciar VM com opções `--gui` ou `--nogui` (padrão configurável).
  - `stop <alias|path>`: Desligar VM com modos `--soft` (desligamento limpo do guest OS) ou `--hard` (corte de energia imediato).
  - `pause <alias|path>` / `unpause <alias|path>`: Pausar e retomar execução.
  - `suspend <alias|path>`: Suspender máquina virtual.
  - `reset <alias|path>`: Reiniciar (soft/hard).
  - `snapshot <subcomando>`:
    - `list <alias>`: Listar árvores de snapshots da VM.
    - `create <alias> <nome>`: Criar snapshot instantâneo.
    - `revert <alias> <nome>`: Reverter para snapshot anterior.

#### EN-US
- **Goal:** Orchestrate local and registered virtual machines without requiring the VMware GUI.
- **Integration:** Asynchronous wrapper around VMware's `vmrun.exe` binary (located via `config.toml` or auto-detected in standard VMware install paths).
- **Hybrid Discovery:**
  1. Scans configured paths (e.g. `D:\VMs`) recursively for `*.vmx` files.
  2. Maps friendly aliases configured in `config.toml` (e.g. `tc14` -> `D:\VMs\TC14_PROD\TC14.vmx`).
  3. Queries running state via `vmrun -T ws list` to detect active (*Running*), paused, or stopped status.
- **Supported Operations:**
  - `start`, `stop`, `pause`, `unpause`, `suspend`, `reset`, and `snapshot` (list, create, revert).

---

### 3.2 Módulo 2: TOP Metrics & Monitoramento de Hardware (`modules::top`)

#### PT-BR
- **Objetivo:** Coletar e ordenar em tempo real os processos com maior consumo de hardware no Windows, incluindo métricas avançadas de GPU e VRAM por processo.
- **Fontes de Métricas:**
  - **CPU, RAM, Disco I/O:** Biblioteca `sysinfo` (leitura direta das estruturas do kernel do Windows com overhead mínimo de CPU).
  - **GPU Engine % e VRAM por PID:**
    - Primário: Contadores de Desempenho do Windows (*PDH - Performance Data Helper*) para as classes `\GPU Engine(*)\Utilization Percentage` e `\GPU Process Memory(*)\Local Usage` (VRAM local dedicada). Funciona de forma universal em placas NVIDIA, AMD e Intel.
    - Opcional/Detecção Automática: `nvml-wrapper` para placas NVIDIA, adicionando telemetria global da GPU (temperatura em °C, potência em Watts, clock e memória total utilizada).
- **Recursos de Exibição:**
  - Tabela dinâmica com ordenação configurável por tecla de atalho (`c`: CPU, `m`: RAM, `d`: Disco I/O, `g`: GPU %, `v`: VRAM).
  - Atualização contínua (*tick rate* configurável, ex: 1000ms).
  - Barra de progresso gráfica para uso geral do sistema (CPU Total, Memória RAM Total e VRAM Global).

#### EN-US
- **Goal:** Real-time collection and sorting of top resource-consuming processes on Windows, including per-process GPU and VRAM metrics.
- **Metric Sources:**
  - **CPU, RAM, Disk I/O:** `sysinfo` crate (direct Windows kernel structure reads with minimal CPU overhead).
  - **GPU Engine % and VRAM per PID:**
    - Primary: Windows Performance Data Helper (PDH) querying `\GPU Engine(*)\Utilization Percentage` and `\GPU Process Memory(*)\Local Usage`. Universally supports NVIDIA, AMD, and Intel GPUs.
    - Optional / Auto-detection: `nvml-wrapper` for NVIDIA GPUs, providing hardware telemetry (temperature in °C, wattage, clocks, and total allocated VRAM).
- **Display Features:**
  - Interactive table with instant keyboard sort hotkeys (`c`: CPU, `m`: RAM, `d`: Disk I/O, `g`: GPU, `v`: VRAM).
  - Configurable tick rate (default 1000ms).
  - System summary progress bars for Total CPU, System RAM, and Global VRAM.

---

### 3.3 Módulo 3: Gerenciador de Processos e Process Killer (`modules::process`)

#### PT-BR
- **Objetivo:** Proporcionar uma forma ágil e segura de pesquisar processos em execução e finalizá-los, substituindo a complexidade de `tasklist` e `taskkill`.
- **Experiência de Uso (UX):**
  - **Filtro Instantâneo (Fuzzy Filter):** O usuário digita qualquer termo (ex: `chrom`, `java`, `nx`, `tc`) e a lista é filtrada instantaneamente sem perder a posição do cursor.
  - **Navegação:** Setas do teclado (`Cima`/`Baixo`), teclas estilo vim (`j`/`k`), `PageUp`/`PageDown`, `Home`/`End`.
  - **Seleção Múltipla:** Tecla `Espaço` ou `Tab` para marcar/desmarcar múltiplos processos simultaneamente.
  - **Escudo de Segurança (Protected Process Shield):**
    - Lista de processos críticos do Windows (ex: `csrss.exe`, `svchost.exe`, `winlogon.exe`, `explorer.exe`, `system`, `services.exe`, `lsass.exe`) protegidos contra encerramento acidental.
    - Modal de confirmação com destaque em vermelho e aviso visual caso o usuário force a seleção de um processo crítico.
  - **Estratégia de Terminação:**
    - Finalização Padrão (Graceful): Solicitação de encerramento amigável.
    - Finalização Forçada (*Force Kill*): Chamada via Win32 `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` ou `taskkill /F /PID <pid>`.
    - Opção de Finalização em Árvore (*Tree Kill*): Finaliza o processo pai e todos os processos filhos (`/T`).

#### EN-US
- **Goal:** Provide a fast, reliable, and safe way to find and terminate processes, replacing the manual hassle of `tasklist` and `taskkill`.
- **User Experience (UX):**
  - **Instant Fuzzy Filter:** Live filtering as the user types (e.g., `chrom`, `java`, `nx`, `tc`).
  - **Navigation:** Arrow keys (`Up`/`Down`), Vim keys (`j`/`k`), `PageUp`/`PageDown`.
  - **Multi-Selection:** `Space` or `Tab` to tag/untag multiple processes for batch kill.
  - **Protected Process Shield:**
    - Critical Windows processes (`csrss.exe`, `svchost.exe`, `winlogon.exe`, etc.) are flagged with warning icons and protected against accidental termination.
    - Confirmation modal requiring explicit acknowledgment before terminating any system-critical process.
  - **Termination Methods:**
    - Graceful termination vs Force Kill (`TerminateProcess` / `taskkill /F /PID <pid>`).
    - Tree Kill option (`/T`) to eliminate child process trees.

---

### 3.4 Módulo 4: Interface TUI Unificada (`tui::dashboard`)

#### Layout do Terminal:
```
+------------------------------------------------------------------------------------------+
| CLI Admin v1.0.0 | [1] 🖥️  VMware VMs | [2] 📊 TOP Metrics | [3] 🎯 Process Manager | ? Help |
+------------------------------------------------------------------------------------------+
| System: CPU: [||||||||||  32%]  RAM: [||||||||||||||| 14.2/32 GB]  VRAM: [|||||||| 6.1/16 GB] |
+------------------------------------------------------------------------------------------+
|  ALIAS / VM NAME     STATUS     PATH                            UPTIME      ACTIONS      |
| > tc14_server        RUNNING    D:\VMs\TC14\tc14.vmx            04h 12m     [P] [S] [R]  |
|   win11_cad          STOPPED    D:\VMs\Win11\win11.vmx          --          [>]          |
|   ubuntu_dev         PAUSED     D:\VMs\Ubuntu\ubuntu.vmx        01h 05m     [>] [S]      |
|                                                                                          |
+------------------------------------------------------------------------------------------+
| [Enter] Ação | [P] Pausar | [S] Parar | [>] Iniciar | [R] Snapshot | [/] Buscar | [q] Sair   |
+------------------------------------------------------------------------------------------+
```

---

## 4. Configuração e Externalização / Configuration & Externalization

O arquivo `config.toml` é lido na seguinte ordem de prioridade:
1. Caminho passado por `--config <caminho>`
2. Diretório corrente de execução: `./config.toml`
3. Diretório do executável: `<exe_dir>/config.toml`
4. Diretório padrão de configurações do usuário: `%APPDATA%\cli_admin\config.toml`

Se nenhum arquivo for encontrado, a aplicação cria automaticamente um `config.toml` comentado com os valores padrão.

### Exemplo de `config.toml`:
```toml
# ==============================================================================
# CLI Admin - Arquivo de Configuração Principal (config.toml)
# ==============================================================================

[general]
refresh_rate_ms = 1000
log_level = "info"               # "trace", "debug", "info", "warn", "error"
log_file = "admin.log"
log_to_stdout = false

[vmware]
# Caminho para o executável vmrun.exe
vmrun_path = "C:\\Program Files (x86)\\VMware\\VMware Workstation\\vmrun.exe"
default_start_mode = "nogui"     # "gui" ou "nogui"
default_stop_mode = "soft"       # "soft" (desligamento guest) ou "hard" (power off)

# Diretórios para busca automática de arquivos .vmx
scan_directories = [
    "D:\\VMs",
    "C:\\Users\\andre\\Documents\\Virtual Machines"
]

# Aliases amigáveis para máquinas virtuais frequentes
[[vmware.vms]]
alias = "tc14"
name = "Teamcenter 14.3 Server"
path = "D:\\VMs\\TC14\\tc14.vmx"

[[vmware.vms]]
alias = "win11-nx"
name = "Windows 11 CAD & NX 2312"
path = "D:\\VMs\\Win11_NX\\win11_nx.vmx"

[metrics]
default_sort = "vram"            # "cpu", "ram", "io", "gpu", "vram"
top_process_count = 50
enable_nvml = true               # Ativa suporte a telemetria dedicada NVIDIA quando disponível

[security]
# Processos do sistema protegidos contra encerramento acidental
protected_processes = [
    "csrss.exe",
    "svchost.exe",
    "explorer.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "smss.exe",
    "system",
    "registry"
]
require_kill_confirmation = true
```

---

## 5. Estrutura de Diretórios do Projeto em Rust / Crate Layout

```
cli_admin/
├── Cargo.toml
├── build.rs                        # Configurações de compilação do Windows
├── config.toml                     # Arquivo de configuração padrão local
├── docs/
│   └── superpowers/
│       └── specs/
│           └── 2026-09-04-cli-admin-design.md
└── src/
    ├── main.rs                     # Ponto de entrada, CLI parser (clap) e inicialização da TUI
    ├── config.rs                   # Carregamento hierárquico, desserialização Serde e geração de template
    ├── logger.rs                   # Configuração de tracing / tracing-appender rotativo (admin.log)
    ├── modules/
    │   ├── mod.rs
    │   ├── vmware/
    │   │   ├── mod.rs              # Orquestrador do VMware
    │   │   ├── client.rs           # Wrapper assíncrono para vmrun.exe
    │   │   ├── discovery.rs        # Scanner de .vmx e mapeador de aliases
    │   │   └── models.rs           # Enums e structs de VM (VmStatus, VmInfo, Snapshot)
    │   ├── top/
    │   │   ├── mod.rs              # Orquestrador de métricas
    │   │   ├── system.rs           # Coleta de CPU, RAM, Disco com `sysinfo`
    │   │   ├── gpu_pdh.rs          # Coleta universal de GPU% e VRAM via Windows PDH
    │   │   ├── gpu_nvml.rs         # Telemetria opcional de GPU NVIDIA (temperatura, watts)
    │   │   └── models.rs           # Estruturas de métricas e enums de ordenação
    │   └── process/
    │       ├── mod.rs              # Gerenciador de processos
    │       ├── killer.rs           # Execução de terminação (Win32 API / taskkill)
    │       ├── filter.rs           # Motor de busca fuzzy
    │       └── shield.rs           # Verificação de segurança contra processos críticos
    └── tui/
        ├── mod.rs                  # Loop de renderização e eventos Crossterm
        ├── app.rs                  # Estado global da aplicação TUI (abas, inputs, seleções)
        ├── event.rs                # Manipulador de teclado, setas e ticks assíncronos
        ├── ui.rs                   # Renderizador principal do layout Ratatui
        ├── views/
        │   ├── vm_view.rs          # Aba de VMs VMware
        │   ├── top_view.rs         # Aba de TOP métricas (RAM, GPU, IO, VRAM)
        │   └── process_view.rs     # Aba do Process Killer interativo com busca
        └── components/
            ├── header.rs           # Barra superior com resumo de sistema e abas
            ├── footer.rs           # Barra de atalhos e status
            └── dialog.rs           # Modais de confirmação de segurança (Kill / Snapshot)
```

---

## 6. Dependências Recomendadas em Rust / Cargo Dependencies

- **CLI & Argument Parsing:** `clap` (com feature `derive`)
- **TUI & Eventos:** `ratatui`, `crossterm`
- **Métricas de Sistema:** `sysinfo`
- **Windows APIs & Contadores de Desempenho:** `windows` (features para PDH, ProcessThreads, Win32 Security)
- **GPU Dedicada (Opcional):** `nvml-wrapper`
- **Busca Fuzzy:** `nucleo` ou `fuzzy-matcher`
- **Configuração & Serialização:** `serde`, `serde_derive`, `toml`, `dirs`
- **Assincronismo & Tasks:** `tokio` (com macros e runtime completo)
- **Logging Estruturado:** `tracing`, `tracing-subscriber`, `tracing-appender`
