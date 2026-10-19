# AGENTS.md

Binário Rust **Windows-only** (`cli_admin`): gerenciador de VMware Workstation (via `vmrun.exe`), monitor de sistema/processos em tempo real (CPU, RAM, Disco I/O, GPU % e VRAM por processo) e finalizador seguro de processos. Entrada dupla: CLI scriptável (subcomandos `clap`) ou TUI interativa (`ratatui` + `crossterm`) quando executado sem subcomando (ou com `-i`/`--interactive`).

## Comandos

```bash
cargo build                 # build de desenvolvimento
cargo build --release       # -> target/release/cli_admin.exe
cargo test                  # todos os testes em tests/unit_tests.rs
cargo test <nome>           # teste único, ex: cargo test test_process_shield
cargo run -- top -n 15 --sort vram   # executar subcomando CLI via cargo
.\target\release\cli_admin.exe       # TUI (sem subcomando)
```

Não há CI de lint/format — `cargo fmt` / `cargo clippy` apenas ad hoc. Os testes cobrem apenas lógica pura (defaults de config, `ProcessShield`, `ProcessFilter`, modelos/matemática de TOP e VMware, parsing de `StartMode`/`StopMode`); não existem testes async/IO (exigem APIs do Windows, `vmrun.exe`, processos vivos).

## Arquitetura

Três subsistemas independentes em `src/modules/`, cada um com `mod.rs` re-exportando sua API. Dois consumidores: `src/main.rs` (dispatch CLI) e `src/tui/app.rs` (`App`) — verificar ambos os pontos de chamada ao alterar a superfície pública de um módulo. `src/lib.rs` apenas re-exporta `config`, `logger`, `modules` e `tui`.

- **`modules::vmware`** — um método por subcomando do `vmrun` em `client.rs` (start/stop/pause/unpause/suspend/reset/snapshots), cada um disparando um subprocesso async via tokio. `discovery.rs`: varre `scan_directories` atrás de `*.vmx` (via `walkdir`) + mescla aliases de `[[vmware.vms]]` + cruza com `vmrun -T ws list` para Running/Stopped; caminho inacessível (disco externo/rede offline) vira status `Offline`. Ordem de resolução de identificador: alias → nome amigável → file stem (`VmDiscovery::resolve_vm_path`). Caminhos são normalizados/canonicalizados antes da comparação.
- **`modules::top`** — `system.rs` (`SystemMetricsManager`) encapsula `sysinfo`, atualizado sincronamente a cada poll. GPU/VRAM por PID vêm de duas fontes opcionais, controladas por `metrics.enable_gpu_metrics` / `enable_nvml`: `gpu_pdh.rs` dispara uma task tokio em background que executa `typeperf` a cada 2s (o parse do CSV PDH é frágil — qualquer divergência de formato silenciosamente produz mapas vazios; cache atrás de `Arc<RwLock<…>>` para a UI nunca bloquear); `gpu_nvml.rs` é uma chamada única a `nvidia-smi`, retornando `None` sem driver NVIDIA.
- **`modules::process`** — o listing reutiliza `SystemMetricsManager::get_top_processes`. `killer.rs` executa `taskkill /PID <pid> [/F] [/T]`. `filter.rs` faz fuzzy-match via `SkimMatcherV2`.

## Segurança: `ProcessShield` (`modules/process/shield.rs`)

Não enfraquecer. Conjunto case-insensitive = `security.protected_processes` + lista hardcoded do kernel NT (system, registry, smss, csrss, wininit, services, lsass, winlogon); ambas as variantes `.exe`/nome nu indexadas; PID 0/4 sempre protegidos. É o que impede a aba Process Killer de dar tela azul na máquina. `classify` separa candidatos a kill entre seguros e protegidos.

## Config (`src/config.rs`) e logging (`src/logger.rs`)

- Precedência do `config.toml`: `--config` > `./config.toml` > `<exe_dir>/config.toml` > `%APPDATA%/cli_admin/config.toml`. Arquivo ausente → template padrão escrito em `./config.toml`. Todas as seções usam `#[serde(default)]` por campo — TOML parcial/editado à mão deve continuar parseando.
- `vm add` anexa a entrada `[[vmware.vms]]` ao final do arquivo sem reescrevê-lo, preservando comentários e formatação.
- Logging é somente em arquivo (`tracing-appender`, arquivo rotativo) por padrão. Logging em stdout somente quando um subcomando CLI **e** `--debug` são passados juntos — a TUI nunca loga em stdout (corrompe o render do alternate-screen).

## TUI (`src/tui/`)

`mod.rs` cuida de raw-mode/alternate-screen + loop draw/event (tick de 150ms; métricas respeitam `general.refresh_rate_ms`). `event.rs` filtra apenas key-press (Windows repete key-release). `app.rs` (`App`, ~970 linhas) guarda o estado das três abas + `ModalState` (confirmação de kill, criação de snapshot, ajuda); cada `ActiveTab` (Vmware/TopMetrics/ProcessManager) faz poll do respectivo módulo em `App::tick()`. `ui.rs` contém todo o desenho. Atalhos relevantes: `1/2/3` trocam abas, `?` ajuda, `q`/`Ctrl+C` sai; aba 3: `/` busca fuzzy, `Espaço` seleciona múltiplos, `k`/`Delete` abre o modal de confirmação (vermelho se houver processo crítico), `e`/`E` exportam a lista filtrada para CSV/JSON com timestamp.

## Convenções

- Código e comentários em **português (pt-BR)** — manter ao editar.
- `Cargo.toml` usa edition 2024; dependências agrupadas por finalidade com comentários de seção.
- O que não pode ser testado de forma determinística (subprocessos, APIs do Windows) fica deliberadamente fora da suíte de testes — lógica pura vai para `tests/unit_tests.rs`.

## Versão 3 (`cli_admin_v3`)

- Executável: `cli_admin_v3.exe` (pacote e biblioteca continuam `cli_admin`).
- Linha de comando completa visível na aba Processos e na exportação. Depende de permissão do Windows para processos de outros usuários e do sistema.
- Encerramento: processo protegido é removido da lista antes do `taskkill`, inclusive com `-y` e na TUI. Flags: `--soft` (sem /F) e `--sem-arvore` (sem /T).
- Falha de VM sai com código `1`.
- Licenças (FlexNet/SNL) não fazem parte da ferramenta: aba, comandos `lic` e `sw borrow` foram removidos.
- Disco (aba Suporte): só calcula por tecla `[r]`, em segundo plano, com `[Esc]` para cancelar e gráfico de barras.
- Detalhes e lista completa de mudanças: `docs/V3_MUDANCAS.md`.
