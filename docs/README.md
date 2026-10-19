# CLI Admin V3

Ferramenta para Windows com **interface interativa (TUI)** e **comandos de linha de comando** para:

- gerenciar máquinas virtuais do VMware Workstation;
- acompanhar CPU, RAM, disco, GPU e VRAM por processo;
- listar e encerrar processos e portas, com proteção dos processos críticos do sistema;
- copiar arquivos e pastas pela TUI;
- diagnosticar rede e coletar informações de suporte;
- expor um **servidor MCP** para agentes de IA, com aprovação humana para qualquer encerramento.

Versão **3.0.0**. Visão de escopo e decisões em [Definicoes_do_Projeto.md](Definicoes_do_Projeto.md). Estado atual em [STATUS.md](STATUS.md).

## Requisitos

- Windows 10 ou 11.
- Rust com edição 2024 (toolchain estável recente).
- Para VMware: VMware Workstation instalado (`vmrun.exe`).
- Para GPU por processo: driver NVIDIA (`nvidia-smi`) ou contadores de GPU do Windows (`typeperf`).
- Para algumas ações (processos de outros usuários, serviços, disco): terminal como administrador.

## Instalação e build

```powershell
cd D:\W\DEV\GENERAL_TOOLS\CLI_ADMIN_V3
cargo build --release --offline                       # cli_admin_v3.exe
cargo build --release --offline --features mcp        # + cli_admin_mcp.exe
```

Sem a feature `mcp`, o `rmcp` não é compilado e o executável principal não tem nenhuma dependência nova.

## Execução

```powershell
.\target\release\cli_admin_v3.exe                     # TUI (sem subcomando)
.\target\release\cli_admin_v3.exe top -n 15 --sort vram
.\target\release\cli_admin_v3.exe export ping --format csv
```

### Comandos

| Comando | Para que serve |
|---|---|
| `vm` | Gerenciar VMs do VMware: iniciar, parar, pausar, suspender, snapshots, cadastrar |
| `top` | Lista de processos ordenada por consumo (CPU, RAM, disco, GPU, VRAM) |
| `kill` | Pesquisar e encerrar processos. Flags: `--soft` (sem `/F`), `--sem-arvore` (sem `/T`) |
| `net` | Portas ativas e encerramento do dono de uma porta |
| `notes` | Notas locais em Markdown |
| `netdiag` | Ping, probe de porta, mapa da LAN, throughput, DNS |
| `winsys` | Disco, snapshot de serviços, boot |
| `export` | Exportar a lista de processos (CSV ou JSON) com a linha de comando |
| `mcp pendentes` | Listar os planos do agente que aguardam aprovação |
| `mcp aprovar <id>` | Aprovar um plano (vale uma execução e expira) |

Use `--help` em cada comando para ver as opções completas.

### TUI

Abas: **1** VMware · **2** TOP · **3** Processos · **4** Rede · **5** Arquivos · **6** Suporte.
`Tab` e `Shift+Tab` alternam as abas, `?` mostra a ajuda, `q` sai.

Atalhos da aba **Arquivos** (5):

| Tecla | Ação |
|---|---|
| `↑↓` `j` `k` `PgUp` `PgDn` `g` `G` | Mover o cursor |
| `Enter` `→` `l` | Entrar na pasta |
| `Backspace` `←` `h` | Subir (na raiz, mostra as unidades) |
| `:` | Digitar um caminho |
| `/` | Filtrar por texto |
| `.` | Mostrar ou ocultar itens ocultos |
| `s` | Trocar a ordenação (nome, tamanho, data) |
| `Espaço` | Marcar ou desmarcar e descer |
| `y` | Guardar marcados (ou o item do cursor) para copiar |
| `p` | Colar na pasta atual (mostra o plano antes de copiar) |
| `n` | Criar nova pasta |
| `r` | Recarregar |
| `Esc` | Cancelar a cópia em andamento, ou limpar marcas e filtro |

Cópias nunca sobrescrevem: conflito de nome vira `nome (2).ext`. Links e junções não são seguidos.

## Configuração

O arquivo `config.toml` é procurado nesta ordem: `--config` › `./config.toml` › pasta do executável › `%APPDATA%\cli_admin\config.toml`. Se nenhum existir, um modelo padrão é criado em `./config.toml`. Chaves ausentes usam o padrão, então um arquivo parcial continua válido.

Seções: `[general]`, `[vmware]`, `[metrics]`, `[security]`, `[notes]`, `[netdiag]`, `[support]`, `[mcp]`.

## Logs e auditoria

- Log da aplicação: arquivo indicado em `general.log_file` (padrão `admin.log`). A TUI nunca escreve log no terminal.
- Auditoria do explorador: `audit/auditoria.log`.
- Auditoria do MCP: `audit/mcp.log` (chave `mcp.auditoria`).
- Planos do MCP aguardando aprovação: `mcp/pendentes/` (chave `mcp.pasta_planos`).

Caminhos relativos são resolvidos a partir do diretório atual. Para o servidor MCP e o `mcp aprovar` enxergarem os mesmos planos, use **caminhos absolutos** nessas chaves.

## Testes

```powershell
cargo test --offline                      # 126 testes
cargo test --offline --features mcp       # 133 testes
cargo test --offline --test unit_tests    # um conjunto específico
```

Os testes cobrem lógica pura e, quando possível, o comportamento de ponta a ponta (explorador com teclas simuladas, servidor MCP por stdio, fluxo de aprovação com um processo de teste). O que depende de hardware real (VMware, GPU, terminal) fica de fora da suíte.

## Estrutura de pastas

```text
src/
  main.rs            CLI (clap) e despacho dos subcomandos
  bin/cli_admin_mcp.rs   servidor MCP por stdio (feature mcp)
  lib.rs             biblioteca (pacote cli_admin)
  config.rs          configuração TOML, com um struct por seção
  logger.rs          log em arquivo
  audit.rs           trilha de auditoria compartilhada
  texto.rs           truncar e barras de texto
  modules/
    vmware/          vmrun, descoberta de VMs
    top/             métricas do sistema, GPU (PDH e nvidia-smi)
    process/         lista, filtro, escudo de protegidos, encerramento, exportação
    net/             portas TCP/UDP (IP Helper)
    fm/              explorador: listagem, unidades, plano e cópia
    support/         diagnóstico de rede, serviços, disco, boot, notas
  mcp/               política, planos, limites, ferramentas e servidor (rmcp)
  tui/               app (estado), ui (telas), fm e fm_ui (aba Arquivos), event
tests/               testes de integração e de protocolo
docs/                esta documentação e os planos/specs em superpowers/
```

Mais detalhes em [MCP.md](MCP.md) e nos specs de `docs/superpowers/specs/`.
