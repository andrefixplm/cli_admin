# AGENTS.md

Fonte única de instruções para agentes de codificação neste projeto. `CLAUDE.md` e `QWEN.md` apenas apontam para cá. O estado atual do projeto (pendências, decisões em aberto, ambiente) fica em `docs/STATUS.md`.

## Visão geral

Binário Rust **Windows-only** `cli_admin_v3` (pacote e biblioteca continuam `cli_admin`). Reúne, num só executável:

- gerenciamento de VMs do VMware Workstation (via `vmrun.exe`);
- monitor de CPU, RAM, disco, GPU e VRAM por processo;
- encerramento seguro de processos, portas de rede e explorador de arquivos;
- ferramentas de suporte Windows (ping, LAN, disco, boot, notas);
- gerenciamento de serviços do Windows.

Há duas entradas: a CLI scriptável (`clap`) e a TUI (`ratatui` + `crossterm`), que abre quando não há subcomando (ou com `-i`/`--interactive`). Um servidor MCP opcional, `cli_admin_mcp`, só é compilado com a feature `mcp`.

## Comandos

```bash
cargo build                              # desenvolvimento -> target/debug/cli_admin_v3.exe
cargo build --release                    # -> target/release/cli_admin_v3.exe
cargo test                               # suíte padrão
cargo test <nome>                        # teste único, ex.: cargo test has_alias
cargo run --bin cli_admin_v3             # TUI (sem subcomando)
cargo run -- top -n 15 --sort vram       # subcomando CLI via cargo

# Servidor MCP (só existe com a feature; sem ela o binário não é compilado):
cargo build --release --features mcp     # -> target/release/cli_admin_mcp.exe
cargo test --features mcp                # inclui os testes do protocolo MCP
```

Não há CI de lint nem de formatação; `cargo fmt` e `cargo clippy` são usados sob demanda. Builds offline funcionam (`--offline`, dependências já estão no `Cargo.lock`). Em máquina com o disco `C:` cheio, `CARGO_PROFILE_DEV_DEBUG=0` reduz o build de dev.

## Arquitetura

`src/main.rs` faz o despacho da CLI, com um `Commands` por subcomando: `vm`, `top`, `kill`, `net`, `notes`, `netdiag`, `winsys`, `servicos`, `export`, `mcp`. `src/lib.rs` re-exporta `audit`, `config`, `logger`, `mcp`, `modules`, `texto` e `tui`.

Os módulos ficam em `src/modules/`, cada um com `mod.rs` re-exportando sua API. Mudanças em superfície pública precisam ser conferidas nos dois consumidores: `src/main.rs` (CLI) e `src/tui/app.rs` (`App`).

- **`vmware`** — um método por subcomando do `vmrun` (`client.rs`), executado de forma assíncrona via tokio. `discovery.rs` varre `scan_directories` atrás de `*.vmx`, mescla os aliases de `[[vmware.vms]]` e cruza com `vmrun -T ws list` para obter Running/Stopped. Caminho inacessível (disco externo ou rede fora) vira `Offline`. Identificadores são resolvidos por alias, depois nome amigável, depois nome do arquivo (`VmDiscovery::resolve_vm_path`).
- **`top`** — `system.rs` (`SystemMetricsManager`) usa `sysinfo` e é atualizado de forma síncrona. GPU e VRAM por PID vêm de duas fontes opcionais, controladas por `metrics.enable_gpu_metrics` e `enable_nvml`: `gpu_pdh.rs` roda `typeperf` a cada 2 s numa task em background, e o parse do CSV é frágil (divergência de formato devolve mapa vazio sem erro); `gpu_nvml.rs` chama `nvidia-smi` uma vez e devolve `None` sem driver NVIDIA.
- **`process`** — `killer.rs` executa `taskkill /PID <pid> [/F] [/T]`. `separar_protegidos` remove os protegidos da lista imediatamente antes do `taskkill`. `filter.rs` faz busca fuzzy (`SkimMatcherV2`). `export.rs` gera CSV (RFC 4180, com BOM UTF-8 para o Excel) e JSON com timestamp.
- **`net`** — conexões TCP/UDP associadas a processos (`scanner.rs`, via APIs do Windows). Subcomandos `net list` e `net kill`; também alimenta a aba Rede.
- **`fm`** — motor do explorador de arquivos: `listing.rs`, `drives.rs`, `copy.rs` (cópia sem sobrescrever, cancelável, com progresso) e `names.rs`. O estado e as teclas da aba ficam em `tui/fm.rs`; o desenho, em `tui/fm_ui.rs`. Mover, renomear e apagar estão fora do escopo da primeira versão.
- **`support`** — `netdiag.rs` (ping em série com estatísticas, probe de porta, mapa da LAN via ARP, varredura de sub-rede, throughput com `ntttcp` externo, DNS), `notes.rs` (notas de campo em Markdown), `winsvc.rs` (listagem de serviços via SCM e leitura de registro) e `winsys.rs` (disco por pasta com cancelamento, snapshot e diff de serviços, auditoria de boot).
- **`services`** — gerenciamento de serviços do Windows, usado pela CLI `servicos`, pela aba `[7]` e pelo MCP:
  - `scm.rs`: a camada Win32 (estado, detalhe, dependências, dependentes ativos, controle, tipo de início, criação). Única parte com `unsafe`; handles fecham em `Drop` (`ScHandle`).
  - `models.rs`: tipos (`Acao`, `StartType`, `Passo`, `SvcDetail`), transições por estado (`valida_transicao`), espera (`passo_espera`), ordem de dependências (`ordem`), resolução de alvo por nome parcial (`resolver_alvo`), validação de caminho de criação e tradução de erros Win32.
  - `shield.rs`: `ServiceShield` (ver Segurança).
  - `ops.rs`: o fluxo em duas fases. `planejar` não altera nada e devolve o plano ou o motivo da recusa; `executar` aplica o plano, espera cada passo e devolve um `Relatorio`. `registrar` grava a auditoria.

## Segurança: os escudos (não enfraquecer)

- **`ProcessShield`** (`modules/process/shield.rs`) — conjunto case-insensitive formado por `security.protected_processes` e pela lista fixa do kernel NT (system, registry, smss, csrss, wininit, services, lsass, winlogon), com as variantes `.exe` e nome nu. PID 0 e 4 são sempre protegidos. O escudo vale também no momento da execução, não só na tela: `separar_protegidos` remove protegidos antes do `taskkill`, inclusive com `-y`.
- **`ServiceShield`** (`modules/services/shield.rs`) — espaço de nomes próprio, separado do de processos. Lista fixa de críticos: RpcSs, RpcEptMapper, DcomLaunch, LSM, SamSs, EventLog, Winmgmt, BFE, Netlogon, LanmanServer, mais `services.protected_names`. Bloqueia parar, reiniciar, pausar e desabilitar. Permite iniciar e continuar: bloquear a subida deixaria a máquina tão inutilizável quanto derrubá-la. Ao parar um serviço, o escudo é aplicado à cadeia inteira de dependentes; se um deles for crítico, a operação toda é recusada. O escudo é checado antes de qualquer consulta ao SCM.
- **Auditoria** (`src/audit.rs`) — uma linha por evento, em cinco campos separados por TAB: quando, usuário, ação, resultado e mensagem. Arquivo padrão: `audit/auditoria.log`. Quem chama não pode colocar segredos na mensagem. Gravam linha as operações de serviços (inclusive recusas, cancelamentos e dry-runs) e as cópias do explorador.

## Config (`src/config.rs`) e logging (`src/logger.rs`)

- Seções: `general`, `vmware`, `metrics`, `security`, `notes`, `netdiag`, `support`, `mcp`, `services`. Todo campo tem `#[serde(default)]`, então TOML parcial ou editado à mão continua parseando. Chave nova precisa de default.
- Precedência: `--config` > `./config.toml` > `./admin.toml` > `<exe_dir>/config.toml` > `%APPDATA%/cli_admin/config.toml`. Se nenhum existir, o template padrão é escrito em `./config.toml`.
- `[services]`: `protected_names` (padrão vazio, soma-se à lista fixa) e `timeout_padrao_s` (padrão 30, espera máxima por passo).
- `vm add` anexa `[[vmware.vms]]` ao fim do arquivo sem reescrevê-lo, preservando comentários. `notes.dir` expande `~` e `%VAR%`.
- `[mcp]` tem padrões restritivos: escrita desligada (`allow_apply = false`), arquivos só dentro de `raizes_permitidas` (padrão: perfil do usuário), `caminhos_negados` (`.ssh`, `.aws`, `*.pem`, `.env` e similares), rede desligada, limites de chamadas e validade dos planos.
- Logging vai só para arquivo (`tracing-appender`, rotativo). Vai para stdout apenas com subcomando CLI **e** `--debug`. A TUI nunca loga em stdout, porque isso corrompe o alternate-screen. No servidor MCP, stdout é do protocolo: nenhum log pode ir para lá.

## Servidor MCP (`src/mcp/`, feature `mcp`)

Protocolo Model Context Protocol (revisão 2026-07-28), transporte stdio, sem porta de rede. Documentação completa e registro no cliente em `docs/MCP.md`.

- `policy.rs`, `tools.rs` e `approval.rs` não dependem do SDK: são funções puras sobre dados, testáveis sem rede. `policy.rs` redige segredos em linhas de comando (melhor esforço, não garantia), aplica raízes permitidas e negações com `canonicalize` (junção que aponta para fora é recusada) e tem o limitador de chamadas. O relógio entra como parâmetro.
- `server.rs` é a única parte que usa `rmcp` (pinado em `=3.5.1`, opcional). `elevation.rs` recusa iniciar como Administrador, salvo com `--permitir-elevado`, e sai com código `2`.

**Ferramentas:**

| Tipo | Ferramentas |
|---|---|
| Somente leitura | `processos_listar`, `portas_listar`, `fs_listar` (nomes e tamanhos, nunca conteúdo), `servicos_listar`, `servico_detalhar` |
| Planejamento (não altera nada) | `processo_planejar_encerramento`, `servico_planejar_acao` (só `start` e `stop`) |
| Aplicação (só com `allow_apply = true`) | `plano_aplicar`, `servico_aplicar` |

Não existe ferramenta de comando livre, SQL ou leitura de arquivo. Reiniciar, pausar, continuar, mudar tipo de início e criar serviço ficam só na CLI e na TUI.

**Fluxo com aprovação humana:**
1. O agente chama uma ferramenta de planejamento. O servidor grava um plano JSON em `mcp.pasta_planos` (`mcp/pendentes`).
2. Um humano, em outro terminal, revisa e aprova: `cli_admin_v3 mcp pendentes` e `cli_admin_v3 mcp aprovar <id>`.
3. O agente chama a ferramenta de aplicação com os mesmos argumentos. O plano é conferido (aprovado, não usado, dentro da validade, argumentos idênticos) e marcado como usado **antes** de agir. Logo, cada plano roda no máximo uma vez.

Tipos de plano: `processo_encerrar` e `servico_acao`. Para serviços, a aplicação **recalcula o plano do zero** e compara, passo a passo, com o que foi aprovado. Se o estado do serviço ou a cadeia de dependentes mudou, a aplicação é recusada.

## TUI (`src/tui/`)

Sete abas: `[1] VMware`, `[2] TOP`, `[3] Processos`, `[4] Rede`, `[5] Arquivos`, `[6] Suporte`, `[7] Serviços`.

- `mod.rs`: raw-mode, alternate-screen e o loop de desenho e eventos, com tick de 150 ms. Métricas respeitam `general.refresh_rate_ms`. Trabalho lento roda em thread e é consultado a cada tick (cálculo de disco, operação de serviço).
- `event.rs` aceita só key-press, porque o Windows também envia key-release.
- `app.rs` (`App`, ~2000 linhas): estado de todas as abas e `ModalState` (confirmação, criação de snapshot, formulários, ajuda). Cada aba faz o poll do seu módulo em `App::tick()`.
- `ui.rs`: desenho das abas e dos modais. `fm.rs` e `fm_ui.rs`: o explorador.

**Atalhos globais:** `1`–`7` e `Tab`/`Shift+Tab` trocam de aba; `?` abre a ajuda; `q` sai. `Ctrl+C` não sai: na aba Processos ele limpa a busca.

**Aba Processos:** `/` busca fuzzy, `Espaço` marca vários, `K`, `Delete` ou `Enter` pedem confirmação de encerramento (vermelho se houver crítico), `e`/`E` exportam CSV/JSON. `n`, `C`, `M` e `V` ordenam.

**Aba Suporte:** seções com `[` e `]`; o disco só é calculado com `r`, em segundo plano; `Esc` cancela.

**Aba Serviços:** `/` filtra, `j`/`k` movem, `r` recarrega, `s` inicia, `t` para, `R` reinicia, `p` pausa, `c` continua, `i` escolhe o tipo de início (a, t, m, d) e `n` abre o formulário de criação (Tab troca de campo, espaço troca o tipo). Toda ação mostra o plano e pede confirmação. A lista carrega na primeira vez que a aba é aberta.

## Testes

Só lógica determinística. O que depende de subprocesso, de API do Windows ou de processo vivo fica fora da suíte, de propósito. Os testes que tocam o sistema existem, mas nunca alteram um serviço ou processo real.

- `tests/unit_tests.rs` — defaults de config, escudos, filtro, modelos de TOP e VMware, `StartMode`/`StopMode`, exportação, e renderização da UI em `TestBackend`.
- `tests/fm_state_tests.rs` e `tests/fm_ui_tests.rs` — explorador, com cenários em pasta temporária.
- `tests/servicos_tests.rs` — aba Serviços (renderização, filtro, modais, recusa de crítico) e configuração `[services]`.
- `tests/mcp_policy_tests.rs` e `tests/mcp_tools_tests.rs` — políticas e ferramentas do MCP.
- `tests/mcp_protocol_tests.rs` — sobe o binário `cli_admin_mcp` por stdio. Só compila com `--features mcp`. O fluxo de aplicação de serviço só roda sem privilégio de Administrador: com privilégio o teste não faz nada, para não parar um serviço real.
- Módulos `#[cfg(test)]` em `src/` — `config`, `texto`, `audit`, `modules::services` (transições, ordem, resolução, escudo, caminho de criação), `mcp::approval` e `mcp::tools`.

Contagem observada em 2026-10-20: 151 testes passam na suíte padrão e 160 com `--features mcp`. A contagem muda; confirme com `cargo test`.

## Convenções

- Código, comentários, nomes de teste e mensagens de erro em **português (pt-BR)**. Mantenha ao editar.
- Edition 2024. `Cargo.toml` agrupa dependências por finalidade, com comentário de seção.
- Nunca fatie `String` por índice de byte. `src/texto.rs` tem `truncar` e `barra`; cortar no meio de um caractere acentuado causa panic.
- Lógica pura fica em funções testáveis. Coleta de dados (sysinfo, sockets, subprocessos, SCM) fica com quem chama. Esse é o padrão de `mcp::tools` e `mcp::policy`.
- Operações destrutivas seguem o mesmo fluxo: planejar, mostrar o plano, confirmar, executar, auditar. Oferecem `--dry-run` e `-y`.
- Finais de linha: arquivos no repositório são LF; a working tree usa CRLF. Ao editar, mantenha o que o arquivo já tem.

## Subcomandos de serviços (`servicos`)

```
servicos list [--filtro X] [--estado running|stopped|paused]
servicos detalhe <nome>                  # estado, caminho, conta, dependências, dependentes
servicos start|stop|restart|pause|continue <nome> [--dry-run] [-y] [--sem-dependentes] [--timeout N]
servicos set-start <nome> auto|atrasado|manual|desabilitado [--dry-run] [-y]
servicos criar <nome> --exe C:\caminho\app.exe [--display X] [--start tipo] [--dry-run] [-y]
```

`<nome>` aceita nome exato ou parte do nome; consulta ambígua é recusada. Parar um serviço para os dependentes ativos antes dele, por padrão. Com `--sem-dependentes`, a operação é recusada se houver dependentes. `criar` aceita só caminho absoluto de um `.exe` existente; caminho com espaço é gravado entre aspas. O serviço criado roda como LocalSystem, que é o padrão do SCM.

## Versão 3

- Executável: `cli_admin_v3.exe`. Pacote e biblioteca continuam `cli_admin`.
- A linha de comando completa aparece na aba Processos, na exportação e no MCP, sempre redigida. Processos de outros usuários e do sistema exigem Administrador para mostrar a linha de comando.
- Licenças (FlexNet/SNL) não fazem parte da ferramenta: aba, comandos `lic` e `sw borrow` foram removidos.
- Mudanças e histórico detalhado: `docs/V3_MUDANCAS.md`. Especificações e planos de design: `docs/superpowers/`.
