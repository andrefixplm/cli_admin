# Especificação Técnica de Design & PRD / Technical Design Specification & PRD
**Projeto / Project:** CLI Admin — Expansão de Ferramentas de Infraestrutura, Diagnóstico de Rede, Bancos de Dados e Gerenciamento de Arquivos
**Data / Date:** 2026-09-29
**Autor / Author:** Antigravity (Pair Programming with Andre)
**Versão / Version:** 2.0.0

---

## 1. Visão Geral / Executive Overview

### 1.1 Português (PT-BR)
Esta especificação técnica detalha a expansão modular do **CLI Admin** para incorporar quatro novos conjuntos de ferramentas essenciais para administração e suporte a ambientes corporativos Windows, Teamcenter PLM, CAD (SolidWorks/NX) e infraestrutura de servidores:

1. **Test-Connection de Portas de Rede (`net test-conn`):** Equivalente nativo e de alta velocidade ao cmdlet `Test-NetConnection -Port xxxx` do PowerShell, fornecendo resolução DNS, latência ICMP Ping (RTT), tempo de handshake TCP (Socket Connect) e diagnóstico detalhado de falhas (Porta Fechada/RST vs Bloqueio de Firewall/Timeout).
2. **Mapeador Avançado de Processo/Serviço por Porta (`net who` / `net find`):** Equivalente avançado ao `netstat -ano | findstr`, identificando não apenas o PID e executável, mas também o **Nome do Serviço Windows associado** (ex: `MSSQLSERVER`, `RpcSs`, `LanmanServer`), caminho completo no disco e conta de usuário executora.
3. **Suíte de Teste de Conexão de Bancos de Dados & Gestão de Utilitários (`db test` / `db shell` / `db export`):** Testes de conectividade em 3 níveis (Porta TCP + Handshake de Protocolo TNS/TDS/Postgres + Query de Validação) para **Microsoft SQL Server**, **Oracle Database** e **PostgreSQL**, com repositório externalizado e versionado de utilitários de cliente (`sqlplus.exe`, `expdp.exe`, `impdp.exe`, `psql.exe`, `pg_dump.exe`, `sqlcmd.exe`), tudo configurável via `config.toml`.
4. **Gerenciador de Arquivos Híbrido (`fm`):** Navegador de arquivos nativo integrado à TUI em `ratatui` (navegação por setas, exibição de tamanhos, permissões, pré-visualização de texto/configs/logs e atalhos de sistema) combinado com suporte/launcher para utilitários externos de terminal sob licença **MIT** (ex: **Broot** ou **LF**).

A interface de usuário em terminal (TUI) é reestruturada em **5 Abas Consolidadas por Domínio** para máxima ergonomia e clareza visual.

### 1.2 English (EN-US)
This technical specification details the modular expansion of **CLI Admin** to introduce four essential tool suites for enterprise Windows infrastructure, Teamcenter PLM, CAD (SolidWorks/NX), and server administration:

1. **Network Port Test-Connection (`net test-conn`):** Native high-performance alternative to PowerShell's `Test-NetConnection -Port xxxx`, providing DNS resolution, ICMP Ping RTT, TCP socket handshake latency, and root-cause failure diagnostics (Closed/RST vs Firewall/Timeout).
2. **Advanced Process & Windows Service Port Inspector (`net who` / `net find`):** Superior alternative to `netstat -ano | findstr`, resolving not only PID and binary names, but also the **hosting Windows Service Name** (e.g., `MSSQLSERVER`, `RpcSs`), full executable path, and user account.
3. **Database Connectivity & Client Utilities Suite (`db test` / `db shell` / `db export`):** 3-tier database connectivity verification (TCP Port + Protocol Handshake TNS/TDS/Postgres + Test Query Execution) for **Microsoft SQL Server**, **Oracle Database**, and **PostgreSQL**, coupled with a versioned, externalized repository of vendor client utilities (`sqlplus`, `expdp`, `psql`, `pg_dump`, `sqlcmd`) configured in `config.toml`.
4. **Hybrid Terminal File Manager (`fm`):** Native `ratatui` interactive file navigator (arrow keys, file metadata, text/config preview, explorer/editor integration) paired with a launcher for external **MIT-licensed** terminal file managers (such as **Broot** or **LF**).

The Terminal User Interface (TUI) is reorganized into **5 Consolidated Domain Tabs** for optimal ergonomics and screen real estate.

---

## 2. Nova Arquitetura da TUI (5 Abas Consolidadas) / TUI Domain Layout

Para evitar sobrecarga de abas no cabeçalho e proporcionar uma navegação fluida, o painel interativo adota 5 domínios temáticos:

```
+---------------------------------------------------------------------------------------------------+
| CLI Admin v2.0 | [1] 🖥️ VMware | [2] 📊 Recursos & Proc | [3] 🌐 Rede | [4] 🗄️ DBs & PLM | [5] 📁 Arquivos & Sup |
+---------------------------------------------------------------------------------------------------+
| CPU: [|||||||||  28%]  RAM: [|||||||||||||| 35.8/63.5 GB]  VRAM: [|||||||| 1.2/4.0 GB]  NVIDIA RTX 500  61°C  14W |
+---------------------------------------------------------------------------------------------------+
|                                                                                                   |
|                                       PAINEL DA ABA ATIVA                                         |
|                                                                                                   |
+---------------------------------------------------------------------------------------------------+
| Mensagem de Status / Notificação: Operação concluída com sucesso.                                 |
| [Tab] Próxima Aba | [Shift+Tab] Aba Anterior | [?] Ajuda Completa | [q] Sair                      |
+---------------------------------------------------------------------------------------------------+
```

### Detalhamento das 5 Abas:

| Aba | Título | Conteúdo & Sub-visões |
|---|---|---|
| **[1]** | **🖥️ VMware** | Inventário de máquinas virtuais, detecção de VMs online/offline, ciclo de vida (`start`, `stop`, `pause`, `unpause`, `suspend`, `reset`), snapshots e cadastro de VMs (`add`). |
| **[2]** | **📊 Recursos & Proc** | Painel unificado de telemetria de hardware (TOP CPU, RAM, Disco, GPU%, VRAM) integrado com o Gerenciador de Processos (busca fuzzy `/`, seleção múltipla, `ProcessShield` anti-tela azul, exportação CSV/JSON e painel inferior de detalhes do processo). |
| **[3]** | **🌐 Rede & Portas** | Sockets ativos TCP/UDP com mapeamento de **Serviço Windows + PID**, sub-tela de teste `Test-NetConnection`, busca textual de portas e finalização do processo dono da porta. |
| **[4]** | **🗄️ Bancos & PLM** | Teste de conexão em 3 níveis para SQL Server, Oracle e Postgres, seletor de versões de utilitários (`sqlplus`, `expdp`, `psql`, `sqlcmd`), launcher de console interativa e monitor de licenças FlexNet / SolidNetWork License (SNL). |
| **[5]** | **📁 Arquivos & Sup** | Navegador de arquivos nativo com preview de texto/configs + atalho para disparar o **Broot (MIT)**, diagnósticos CAD (SolidWorks/NX), série de pings, tabela ARP da LAN, auditoria de boot e notas de engenharia. |

---

## 3. Especificação dos 4 Novos Módulos / Detailed Subsystem Specifications

---

### 3.1 Ferramenta 1: Test-Connection de Portas de Rede (`modules::net::test_conn`)

#### PT-BR
- **Objetivo:** Fornecer diagnóstico de portas de rede idêntico ou superior ao cmdlet do PowerShell `Test-NetConnection <host> -Port <port>` com execução instantânea (sub-milissegundos) em Rust nativo.
- **Métricas & Dados Coletados:**
  1. **ComputerName:** Hostname ou IP informado.
  2. **RemoteAddress:** IP(s) resolvidos via DNS (IPv4 e IPv6).
  3. **Interface / SourceAddress:** Adaptador de rede local utilizado para o roteamento.
  4. **PingSucceeded:** Sucesso/falha no teste ICMP.
  5. **PingRTT:** Tempo de ida e volta (Round Trip Time em ms).
  6. **TcpTestSucceeded:** `true` se o socket TCP realizou o 3-way handshake com sucesso, `false` caso contrário.
  7. **ConnectLatencyMs:** Tempo em milissegundos para estabelecer a conexão TCP.
  8. **Causa Raiz da Falha:**
     - *Connection Refused (RST):* O host remoto está acessível, mas a porta não tem nenhum serviço escutando.
     - *Timed Out (Drop):* Sem resposta dentro do limite configurado (típico de bloqueio de Firewall corporativo ou rota inexistente).
     - *Host Unreachable:* Falha de roteamento local ou gateway.
     - *DNS Failure:* O hostname não pôde ser resolvido.
- **Atalhos de Portas Comuns (`--port <nome>`):**
  - `oracle` → 1521
  - `sqlserver` / `mssql` → 1433
  - `postgres` → 5432
  - `tc` / `teamcenter` → 1521, 28000, 8080
  - `snl` / `flexnet` → 25734, 25735
  - `rdp` → 3389
  - `ssh` → 22
  - `http` / `https` → 80, 443

#### EN-US
- **Goal:** Provide network port diagnostics identical or superior to PowerShell's `Test-NetConnection <host> -Port <port>` with sub-millisecond native Rust execution.
- **Collected Metrics:**
  - Resolved IP(s), Local source adapter/IP, ICMP Ping RTT, TCP Handshake Latency in ms, and Root-Cause Diagnostics (Connection Refused vs Firewall Timeout Drop vs DNS failure).
  - Presets for popular enterprise & PLM ports (`oracle`, `sqlserver`, `postgres`, `snl`, `tc`, `rdp`).

---

### 3.2 Ferramenta 2: Mapeador de Processo e Serviço por Porta (`modules::net::finder`)

#### PT-BR
- **Objetivo:** Superar as limitações do comando `netstat -ano | findstr <termo>`, permitindo saber exatamente **qual processo e qual serviço Windows** é dono de uma porta local ou conexão remota.
- **O Problema Resolvido:** No Windows, quando portas comuns (como 135, 445, 1433 ou 8080) são escutadas por `svchost.exe` ou `services.exe`, o `netstat` apenas exibe o PID numérico. O analista precisa manualmente cruzar com `tasklist /svc`.
- **Implementação Técnica em Rust:**
  1. Varredura de sockets TCP/UDP via `GetExtendedTcpTable` e `GetExtendedUdpTable` da Windows IP Helper API.
  2. Mapeamento de PID para nome do executável e caminho completo em disco (`sysinfo` + Win32 `QueryFullProcessImageNameW`).
  3. Mapeamento de PID para **Nome de Exibição do Serviço Windows** via Win32 Service Manager API (`EnumServicesStatusExW` com `SC_ENUM_PROCESS_INFO`).
  4. Resolução da conta de usuário dona do processo (ex: `NT AUTHORITY\SYSTEM`, `LOCAL SERVICE`, ou usuário de domínio).
- **Subcomandos CLI:**
  - `cli_admin net who --port <porta>`: Exibe um card visual completo com o executável, serviço Windows, caminho, usuário, status e uptime do dono daquela porta.
  - `cli_admin net find <termo>`: Filtro instantâneo case-insensitive que busca simultaneamente em porta, nome do processo, nome do serviço Windows e IP remoto.
  - `cli_admin net kill --port <porta>`: Finaliza o processo responsável pela porta (com confirmação e escudo de segurança).

#### EN-US
- **Goal:** Overcome `netstat -ano | findstr` limitations by directly mapping ports to **Windows Service Display Names** (e.g., `MSSQLSERVER`, `RpcSs`), full binary paths, and user accounts.
- **Commands:** `net who --port <port>`, `net find <query>`, and `net kill --port <port>`.

---

### 3.3 Ferramenta 3: Suíte de Teste de Bancos de Dados & Gestão de Utilitários (`modules::database`)

#### PT-BR
- **Objetivo:** Validar conectividade de bancos de dados corporativos utilizados por sistemas PLM (Teamcenter, 3DEXPERIENCE), ERP e ferramentas CAD, além de gerenciar utilitários de cliente de múltiplas versões sem poluir o `PATH` global da máquina.
- **SGBDs Suportados:**
  1. **Oracle Database** (padrão porta 1521, TNS)
  2. **Microsoft SQL Server** (padrão porta 1433, TDS)
  3. **PostgreSQL** (padrão porta 5432)
- **Estrutura de Diretórios para Utilitários Versionados:**
  ```
  cli_admin/
  └── tools/
      └── db_clients/
          ├── oracle/
          │   ├── instantclient_19c/
          │   │   ├── sqlplus.exe
          │   │   ├── expdp.exe
          │   │   ├── impdp.exe
          │   │   ├── tnsping.exe
          │   │   └── oci.dll
          │   └── instantclient_21c/
          ├── postgres/
          │   ├── v14/
          │   │   ├── psql.exe
          │   │   ├── pg_dump.exe
          │   │   └── pg_restore.exe
          │   └── v16/
          └── sqlserver/
              ├── v17/
              │   ├── sqlcmd.exe
              │   └── bcp.exe
              └── v18/
  ```
- **Níveis de Teste de Conexão:**
  - **Nível 1 (TCP Socket Handshake):** Testa se a porta está aberta e medindo a latência de conexão em milissegundos.
  - **Nível 2 (Protocol Handshake):**
    - Oracle: Envia pacote de handshake TNS Connect ou executa `tnsping` com cálculo de tempo.
    - PostgreSQL: Envia pacote de inicialização de protocolo (*StartupMessage v3.0*) e valida resposta do listener.
    - SQL Server: Envia pacote de handshake pré-login TDS (*TDS Pre-Login Handshake*) para validar se é uma instância ativa de SQL Server.
  - **Nível 3 (Autenticação & Execução de Query):** Invoca de forma controlada o utilitário configurado (`sqlplus`, `psql`, `sqlcmd`) passando credenciais seguras e executando uma query de teste rápida (`SELECT 1` ou `SELECT * FROM v$version` ou `SELECT @@VERSION`).
- **Ações Rápidas via CLI e TUI:**
  - `cli_admin db test <nome_ou_tipo>`: Executa os 3 níveis de teste e exibe banner/versão do banco.
  - `cli_admin db shell <nome>`: Abre o console interativo do utilitário configurado (`sqlplus`, `psql`, `sqlcmd`) já autenticado na instância selecionada.
  - `cli_admin db export <nome> --out <dir>`: Wrapper ergonômico para disparo do `expdp` (Oracle) ou `pg_dump` (PostgreSQL) com parâmetros recomendados para backup rápido.

#### EN-US
- **Goal:** Robust 3-tier connectivity validation (TCP socket + protocol handshake TNS/TDS/Postgres + test query execution) for **Oracle**, **SQL Server**, and **PostgreSQL**, managing multiple versions of vendor client tools (`sqlplus`, `expdp`, `psql`, `sqlcmd`) in an externalized directory structure.
- **Commands:** `db test <instance>`, `db shell <instance>`, `db export <instance>`.

---

### 3.4 Ferramenta 4: Gerenciador de Arquivos Híbrido (`modules::file_manager`)

#### PT-BR
- **Objetivo:** Fornecer navegação rápida de arquivos, inspeção de logs, edição de configs e manipulação de diretórios sem sair do terminal, com licenciamento estritamente permissivo (MIT / Apache 2.0).
- **Abordagem Híbrida:**
  1. **Navegador Nativo TUI (Ratatui):**
     - Tela integrada na Aba `[5] Arquivos & Suporte`.
     - Layout com duas colunas: Lista de Arquivos/Diretórios à esquerda e Painel de Pré-visualização (*Preview*) à direita.
     - Navegação total com setas do teclado (`Cima`/`Baixo` para selecionar, `Enter` para entrar em pastas, `Backspace` ou `Esc` para subir diretório).
     - Preview dinâmico de arquivos de texto, logs (`admin.log`, `*.log`), scripts (`.bat`, `.ps1`), configurações (`.toml`, `.json`, `.cfg`, `.env`, `.xml`) com numeração de linhas.
     - Atalhos rápidos:
       - `o`: Revelar arquivo/pasta no Windows Explorer (`explorer.exe /select,<path>`).
       - `e`: Abrir arquivo no editor de texto configurado ou Bloco de Notas (`notepad.exe`).
       - `c`: Copiar caminho completo do arquivo para a área de transferência do Windows.
       - `d`: Excluir arquivo (com modal de confirmação de segurança).
       - `f`: Filtrar arquivos da pasta atual via busca fuzzy.
  2. **Launcher / Integração com Ferramenta MIT Externa (Broot):**
     - O **Broot** (`canop/broot`) é uma ferramenta em Rust sob licença **MIT** com visualização em árvore, busca inteligente e manipulação avançada de arquivos.
     - O executável `broot.exe` pode ser colocado na pasta `./tools/file_manager/broot.exe` (ou referenciado no `config.toml`).
     - Subcomando direto: `cli_admin fm [caminho]`.
     - Atalho na TUI: Pressionar `F` suspende temporariamente a TUI do `cli_admin`, abre o Broot na pasta selecionada e, ao sair do Broot, restaura o `cli_admin` perfeitamente no mesmo estado.

#### EN-US
- **Goal:** Fast, ergonomic file navigation, log inspection, and configuration viewing without leaving the terminal, adhering strictly to **MIT** licensing.
- **Hybrid Implementation:**
  - **Native Ratatui View:** Two-column explorer with live text/log/config preview, arrow navigation, and system shortcuts (Explorer reveal, Editor open, Clipboard copy).
  - **Broot (MIT) Launcher:** Integrated launcher for `broot.exe` via `cli_admin fm` or keyboard shortcut `F`.

---

## 4. Estrutura do Arquivo de Configuração (`config.toml`)

```toml
# ==============================================================================
# CLI Admin v2.0 - Arquivo de Configuração Principal
# ==============================================================================

[general]
refresh_rate_ms = 1000
log_level = "info"
log_file = "admin.log"
log_to_stdout = false

# ------------------------------------------------------------------------------
# 1. Ferramentas de Rede & Portas
# ------------------------------------------------------------------------------
[netdiag]
probe_timeout_ms = 2000
ping_count = 4
sweep_concurrency = 64

# Portas padrão e presets amigáveis
[netdiag.presets]
oracle = [1521]
sqlserver = [1433]
postgres = [5432]
teamcenter = [1521, 28000, 8080]
flexnet = [25734, 25735]
rdp = [3389]
web = [80, 443]

# ------------------------------------------------------------------------------
# 2. Bancos de Dados & Utilitários de Cliente (SQL / Oracle / Postgres)
# ------------------------------------------------------------------------------
[database]
# Diretório raiz onde residem as pastas de ferramentas de clientes
tools_dir = "./tools/db_clients"
default_timeout_ms = 5000

# Definições de versões de utilitários disponíveis no disco
[[database.client_tools]]
db_type = "oracle"
version = "19c"
path = "./tools/db_clients/oracle/instantclient_19c"
sql_client = "sqlplus.exe"
export_tool = "expdp.exe"
import_tool = "impdp.exe"
ping_tool = "tnsping.exe"

[[database.client_tools]]
db_type = "postgres"
version = "16"
path = "./tools/db_clients/postgres/v16"
sql_client = "psql.exe"
export_tool = "pg_dump.exe"
import_tool = "pg_restore.exe"

[[database.client_tools]]
db_type = "sqlserver"
version = "v18"
path = "./tools/db_clients/sqlserver/v18"
sql_client = "sqlcmd.exe"
export_tool = "bcp.exe"

# Perfis de instâncias conhecidas para teste e acesso rápido
[[database.instances]]
name = "TC_ORACLE_PROD"
db_type = "oracle"
host = "192.168.1.100"
port = 1521
service_name = "TC"
user = "infodba"
# // CUIDADO: NUNCA USAR CREDENCIAIS REAIS EM PRODUÇÃO - AMBIENTE ISOLADO DE DESENVOLVIMENTO
password = "infodba_secret"
preferred_client_version = "19c"

[[database.instances]]
name = "LOCAL_POSTGRES"
db_type = "postgres"
host = "127.0.0.1"
port = 5432
database_name = "teamcenter_db"
user = "postgres"
password = "postgres_local"
preferred_client_version = "16"

[[database.instances]]
name = "LOCAL_SQLSERVER"
db_type = "sqlserver"
host = "127.0.0.1"
port = 1433
database_name = "master"
user = "sa"
password = "Password123!"
preferred_client_version = "v18"

# ------------------------------------------------------------------------------
# 3. Gerenciador de Arquivos (Nativo + Broot MIT)
# ------------------------------------------------------------------------------
[file_manager]
default_start_dir = "."
external_tool = "./tools/file_manager/broot.exe"
preview_max_lines = 100
text_editor = "notepad.exe"
show_hidden_files = false
```

---

## 5. Estrutura de Código Rust Planejada / Codebase Modular Layout

```
cli_admin/
├── tools/
│   ├── db_clients/               # Binários de clientes versionados (Oracle, Postgres, SQL)
│   └── file_manager/             # Executável do Broot (MIT)
├── config.toml                   # Configuração com seções [database], [file_manager]
└── src/
    ├── modules/
    │   ├── net/
    │   │   ├── mod.rs            # Re-exportações do subsistema de rede
    │   │   ├── scanner.rs        # Scanner de sockets TCP/UDP
    │   │   ├── models.rs         # Modelos NetConnection e TcpState
    │   │   ├── test_conn.rs      # [NOVO] Test-NetConnection (DNS + Ping + TCP Handshake)
    │   │   └── services.rs       # [NOVO] Mapeador de PIDs para Nomes de Serviços Windows
    │   ├── database/             # [NOVO] Módulo completo de Banco de Dados
    │   │   ├── mod.rs            # Orquestrador de DBs
    │   │   ├── models.rs         # Structs de Instância, Credenciais, Níveis de Teste
    │   │   ├── handshake.rs      # Handshake de rede TNS, TDS e Postgres
    │   │   ├── launcher.rs       # Invocador de sqlplus, psql, sqlcmd, expdp
    │   │   └── discovery.rs      # Scanner de executáveis na pasta ./tools/db_clients
    │   └── file_manager/         # [NOVO] Módulo de Gerenciamento de Arquivos
    │       ├── mod.rs            # Orquestrador de Arquivos
    │       ├── explorer.rs       # Motor de listagem, navegação, ordenação e filtros
    │       ├── preview.rs        # Leitor seguro de pré-visualização de texto/configs/logs
    │       └── launcher.rs       # Launcher externo para Broot (MIT)
    └── tui/
        ├── app.rs                # Estado atualizado com 5 abas de domínio
        ├── ui.rs                 # Renderizador central atualizado
        └── views/
            ├── vm_view.rs        # Aba 1: VMware
            ├── top_proc_view.rs  # Aba 2: Recursos TOP + Processos consolidados
            ├── net_view.rs       # Aba 3: Rede, Portas e Test-Connection
            ├── db_view.rs        # Aba 4: Bancos de Dados & Licenças
            └── file_view.rs      # Aba 5: File Explorer Nativo + Suporte
```
