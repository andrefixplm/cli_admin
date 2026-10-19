# Expansão de Ferramentas de Infraestrutura, Diagnóstico de Rede, Bancos de Dados e Arquivos Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar no `cli_admin` quatro novas suítes de ferramentas (Test-Connection de portas, Mapeador de Processo/Serviço Windows por porta, Suíte de Bancos de Dados SQL/Oracle/Postgres com utilitários versionados e Gerenciador de Arquivos Híbrido com Broot MIT), consolidando a TUI em 5 abas de domínio.

**Architecture:** Módulos desacoplados em `src/modules/` (`net::test_conn`, `net::services`, `database`, `file_manager`), integrados ao `src/config.rs` e consumidos por `src/main.rs` (CLI) e `src/tui/` (TUI com 5 abas de domínio).

**Tech Stack:** Rust 2024, `ratatui 0.29`, `crossterm 0.28`, `tokio 1.43`, `windows 0.58` (Win32 APIs: NetworkManagement, Services, Registry, Foundation), `serde`, `toml`, `fuzzy-matcher`.

**Spec:** [docs/superpowers/specs/2026-09-29-infra-tools-expansion-design.md](file:///d:/W/DEV/GENERAL_TOOLS/CLI_ADMIN/docs/superpowers/specs/2026-09-29-infra-tools-expansion-design.md)

## Global Constraints
- Sistema operacional exclusivo: Windows 10/11 x64 (`#[cfg(windows)]`).
- Idioma: Código e comentários em Português (pt-BR); nomes de símbolos em Inglês.
- Externalização: Sem credenciais ou caminhos absolutos hardcoded; tudo configurável em `config.toml`.
- Licenciamento: Apenas ferramentas com licença permissiva (MIT / Apache 2.0).

---

### Task 1: Scaffolding de Configurações e Pastas de Ferramentas

**Files:**
- Modify: `src/config.rs:13-35, 190-250`
- Modify: `config.toml:80-140`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Produces: `DatabaseConfig`, `ClientToolConfig`, `DbInstanceConfig`, `FileManagerConfig` em `src/config.rs`.

- [ ] **Step 1: Escrever teste unitário para parsing das novas seções de config**
Em `tests/unit_tests.rs`, adicionar:
```rust
#[test]
fn test_database_and_fm_config() {
    let toml_str = r#"
    [database]
    tools_dir = "./tools/db_clients"
    default_timeout_ms = 4000

    [[database.client_tools]]
    db_type = "oracle"
    version = "19c"
    path = "./tools/db_clients/oracle/19c"
    sql_client = "sqlplus.exe"

    [[database.instances]]
    name = "TEST_DB"
    db_type = "postgres"
    host = "127.0.0.1"
    port = 5432
    user = "postgres"

    [file_manager]
    default_start_dir = "C:\\"
    external_tool = "./tools/file_manager/broot.exe"
    preview_max_lines = 80
    "#;
    let cfg: AppConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.database.tools_dir, "./tools/db_clients");
    assert_eq!(cfg.database.client_tools.len(), 1);
    assert_eq!(cfg.database.instances[0].name, "TEST_DB");
    assert_eq!(cfg.file_manager.preview_max_lines, 80);
}
```

- [ ] **Step 2: Executar teste para verificar que falha**
Run: `cargo test --test unit_tests test_database_and_fm_config`
Expected: FAIL (campos `database` e `file_manager` inexistentes).

- [ ] **Step 3: Implementar structs em `src/config.rs` e atualizar `config.toml`**
Adicionar structs `DatabaseConfig`, `ClientToolConfig`, `DbInstanceConfig`, `FileManagerConfig` com `#[derive(Debug, Clone, Serialize, Deserialize)]` e defaults no `src/config.rs`.
Adicionar as seções padrão comentadas no `config.toml`.
Criar pastas:
`tools/db_clients/oracle/`, `tools/db_clients/postgres/`, `tools/db_clients/sqlserver/`, `tools/file_manager/`.

- [ ] **Step 4: Executar teste para verificar aprovação**
Run: `cargo test --test unit_tests test_database_and_fm_config`
Expected: PASS.

---

### Task 2: Ferramenta de Test-Connection de Portas de Rede

**Files:**
- Create: `src/modules/net/test_conn.rs`
- Modify: `src/modules/net/mod.rs`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Produces: `test_net_connection(host: &str, port: u16, timeout_ms: u64) -> NetConnectionTestResult` em `modules::net`.

- [ ] **Step 1: Escrever teste unitário para NetConnectionTestResult e diagnóstico de erro**
Em `tests/unit_tests.rs`:
```rust
#[test]
fn test_net_conn_result_format() {
    let res = NetConnectionTestResult {
        target: "localhost".to_string(),
        port: 80,
        resolved_ips: vec!["127.0.0.1".to_string()],
        ping_succeeded: true,
        ping_rtt_ms: Some(1),
        tcp_succeeded: true,
        tcp_latency_ms: Some(2),
        local_adapter_ip: Some("127.0.0.1".to_string()),
        failure_reason: None,
    };
    assert!(res.tcp_succeeded);
    assert!(res.failure_reason.is_none());
}
```

- [ ] **Step 2: Executar teste para verificar que falha**
Run: `cargo test --test unit_tests test_net_conn_result_format`
Expected: FAIL (tipo inexistente).

- [ ] **Step 3: Implementar `src/modules/net/test_conn.rs`**
Implementar resolução DNS com `tokio::net::lookup_host`, ping ICMP via `modules::support::netdiag::ping_series`, conexão TCP com medição de `Instant::now()` e classificação de erro (`ErrorKind::ConnectionRefused` vs `TimedOut`).
Re-exportar em `src/modules/net/mod.rs`.

- [ ] **Step 4: Executar testes**
Run: `cargo test --test unit_tests test_net_conn_result_format`
Expected: PASS.

---

### Task 3: Mapeador de Processos e Serviços Windows por Porta

**Files:**
- Create: `src/modules/net/services.rs`
- Modify: `src/modules/net/mod.rs`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Produces: `map_pids_to_service_names() -> HashMap<u32, Vec<String>>`, `inspect_port_owner(port: u16) -> Option<PortOwnerDetails>`.

- [ ] **Step 1: Escrever teste unitário para estrutura de PortOwnerDetails**
Em `tests/unit_tests.rs`:
```rust
#[test]
fn test_port_owner_details() {
    let d = PortOwnerDetails {
        port: 1433,
        proto: "TCP".to_string(),
        pid: 4076,
        process_name: "sqlservr.exe".to_string(),
        exe_path: Some("C:\\Program Files\\Microsoft SQL Server\\sqlservr.exe".to_string()),
        service_names: vec!["MSSQLSERVER".to_string()],
        user: Some("NT SERVICE\\MSSQLSERVER".to_string()),
        is_protected: false,
    };
    assert_eq!(d.port, 1433);
    assert_eq!(d.service_names[0], "MSSQLSERVER");
}
```

- [ ] **Step 2: Executar teste para verificar que falha**
Run: `cargo test --test unit_tests test_port_owner_details`
Expected: FAIL.

- [ ] **Step 3: Implementar `src/modules/net/services.rs`**
Usar Win32 API `EnumServicesStatusExW` com `SC_ENUM_PROCESS_INFO` para associar cada PID ativo aos nomes de serviços que ele hospeda.
Buscar caminho do executável via Win32 `QueryFullProcessImageNameW`.
Re-exportar em `src/modules/net/mod.rs`.

- [ ] **Step 4: Executar testes**
Run: `cargo test --test unit_tests test_port_owner_details`
Expected: PASS.

---

### Task 4: Módulo de Bancos de Dados — Handshake de Rede e Modelos

**Files:**
- Create: `src/modules/database/models.rs`
- Create: `src/modules/database/handshake.rs`
- Create: `src/modules/database/mod.rs`
- Modify: `src/modules/mod.rs`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Produces: `probe_db_handshake(db_type: DbType, host: &str, port: u16, timeout_ms: u64) -> Result<HandshakeResult, String>` em `modules::database`.

- [ ] **Step 1: Escrever teste unitário para criação de pacotes de handshake TNS e Postgres**
Em `tests/unit_tests.rs`:
```rust
#[test]
fn test_database_handshake_packets() {
    let pg_pkt = build_postgres_startup_packet("user_test", "db_test");
    assert!(pg_pkt.len() > 8);
    // Protocol version 3.0 (196608 em big-endian = [0x00, 0x03, 0x00, 0x00])
    assert_eq!(&pg_pkt[4..8], &[0x00, 0x03, 0x00, 0x00]);

    let tns_pkt = build_oracle_tns_connect_packet();
    assert!(tns_pkt.len() >= 58);
    // TNS Connect Type = 1
    assert_eq!(tns_pkt[4], 1);
}
```

- [ ] **Step 2: Executar teste para verificar que falha**
Run: `cargo test --test unit_tests test_database_handshake_packets`
Expected: FAIL.

- [ ] **Step 3: Implementar `handshake.rs` e `models.rs` em `src/modules/database/`**
Implementar construtores de pacotes de handshake de baixo nível (TNS connect para Oracle, StartupMessage para PostgreSQL, TDS pre-login para SQL Server).
Implementar `probe_db_handshake` assíncrono via `tokio::net::TcpStream`.
Re-exportar em `src/modules/database/mod.rs` e `src/modules/mod.rs`.

- [ ] **Step 4: Executar testes**
Run: `cargo test --test unit_tests test_database_handshake_packets`
Expected: PASS.

---

### Task 5: Módulo de Bancos de Dados — Launcher de Utilitários Versionados

**Files:**
- Create: `src/modules/database/launcher.rs`
- Modify: `src/modules/database/mod.rs`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Produces: `run_test_query(tool: &ClientToolConfig, instance: &DbInstanceConfig) -> Result<String, String>`, `spawn_interactive_shell(...)`, `spawn_export(...)`.

- [ ] **Step 1: Escrever teste unitário para montagem de argumentos do cliente CLI**
Em `tests/unit_tests.rs`:
```rust
#[test]
fn test_db_launcher_arg_generation() {
    let psql_args = build_psql_args("localhost", 5432, "postgres", "db1", Some("SELECT 1;"));
    assert!(psql_args.contains(&"-h".to_string()));
    assert!(psql_args.contains(&"localhost".to_string()));

    let sqlcmd_args = build_sqlcmd_args("localhost", 1433, "sa", "Password123!", Some("SELECT 1"));
    assert!(sqlcmd_args.contains(&"-S".to_string()));
}
```

- [ ] **Step 2: Executar teste para verificar que falha**
Run: `cargo test --test unit_tests test_db_launcher_arg_generation`
Expected: FAIL.

- [ ] **Step 3: Implementar `launcher.rs`**
Implementar montagem de argumentos específicos para `sqlplus`, `psql`, `sqlcmd`, `pg_dump`, `expdp`.
Executar comandos não-interativos via `tokio::process::Command` para o teste de query (`SELECT 1`).
Adicionar método para abrir terminal interativo conectado.
Re-exportar em `src/modules/database/mod.rs`.

- [ ] **Step 4: Executar testes**
Run: `cargo test --test unit_tests test_db_launcher_arg_generation`
Expected: PASS.

---

### Task 6: Gerenciador de Arquivos — Motor de Navegação e Leitor de Preview

**Files:**
- Create: `src/modules/file_manager/models.rs`
- Create: `src/modules/file_manager/explorer.rs`
- Create: `src/modules/file_manager/preview.rs`
- Create: `src/modules/file_manager/mod.rs`
- Modify: `src/modules/mod.rs`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Produces: `FileExplorerState`, `read_file_preview(path: &Path, max_lines: usize) -> FilePreviewResult`.

- [ ] **Step 1: Escrever teste unitário para listagem de pastas e leitura de preview seguro**
Em `tests/unit_tests.rs`:
```rust
#[test]
fn test_file_explorer_and_preview() {
    let tmp = std::env::temp_dir().join(format!("fm_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("test.txt"), "Linha 1\nLinha 2\nLinha 3\n").unwrap();

    let mut explorer = FileExplorerState::new(&tmp);
    explorer.refresh();
    assert_eq!(explorer.items.len(), 1);
    assert_eq!(explorer.items[0].name, "test.txt");

    let prev = read_file_preview(&tmp.join("test.txt"), 2).unwrap();
    assert_eq!(prev.lines.len(), 2);
    assert_eq!(prev.total_lines, 3);

    let _ = std::fs::remove_dir_all(&tmp);
}
```

- [ ] **Step 2: Executar teste para verificar que falha**
Run: `cargo test --test unit_tests test_file_explorer_and_preview`
Expected: FAIL.

- [ ] **Step 3: Implementar `explorer.rs` e `preview.rs`**
Implementar varredura de diretório com `std::fs::read_dir`, ordenando pastas primeiro e arquivos por nome/tamanho.
Implementar leitor seguro com limite de linhas e detecção de arquivos binários (não exibir caracteres ilegíveis).
Re-exportar em `src/modules/file_manager/mod.rs` e `src/modules/mod.rs`.

- [ ] **Step 4: Executar testes**
Run: `cargo test --test unit_tests test_file_explorer_and_preview`
Expected: PASS.

---

### Task 7: Gerenciador de Arquivos — Launcher Externo Broot (MIT)

**Files:**
- Create: `src/modules/file_manager/launcher.rs`
- Modify: `src/modules/file_manager/mod.rs`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Produces: `launch_external_file_manager(tool_path: &Path, start_dir: &Path) -> Result<(), String>`.

- [ ] **Step 1: Implementar `launcher.rs`**
Adicionar função `launch_external_file_manager`:
- Suspende o modo raw do terminal via Crossterm.
- Spawna `Command::new(tool_path).arg(start_dir).status()`.
- Restaura o modo raw e tela alternada imediatamente após a saída do Broot.
Re-exportar em `src/modules/file_manager/mod.rs`.

- [ ] **Step 2: Compilar com `cargo check`**
Run: `cargo check`
Expected: 0 errors.

---

### Task 8: Consolidação da Interface TUI em 5 Abas de Domínio

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/mod.rs`

**Interfaces:**
- Atualizar `ActiveTab` para:
  `Vmware = 0`, `ResourcesAndProcesses = 1`, `Network = 2`, `Databases = 3`, `FilesAndSupport = 4`.

- [ ] **Step 1: Atualizar `ActiveTab` e estado do `App` em `src/tui/app.rs`**
Integrar `FileExplorerState`, lista de conexões enriquecidas com serviços Windows, e instâncias de banco de dados.
Adicionar atalhos de teclado para as 5 abas (`1`, `2`, `3`, `4`, `5`).

- [ ] **Step 2: Atualizar renderização em `src/tui/ui.rs`**
Desenhar abas:
- Aba 1: VMware
- Aba 2: Recursos TOP + Processos unificados
- Aba 3: Rede & Portas com Mapeador de Serviços Windows
- Aba 4: Bancos de Dados & Licenças SNL/FlexNet
- Aba 5: Gerenciador de Arquivos Nativo (duas colunas: lista + preview) com atalhos de suporte

- [ ] **Step 3: Compilar com `cargo check`**
Run: `cargo check`
Expected: 0 errors.

---

### Task 9: Integração dos Subcomandos CLI no `src/main.rs` & Verificação Final

**Files:**
- Modify: `src/main.rs`
- Modify: `README.md`
- Test: `tests/unit_tests.rs`

**Interfaces:**
- Adicionar comandos:
  - `cli_admin net test-conn <host> --port <port>`
  - `cli_admin net who --port <port>`
  - `cli_admin net find <query>`
  - `cli_admin db test [name]`
  - `cli_admin db shell <name>`
  - `cli_admin db export <name>`
  - `cli_admin fm [path]`

- [ ] **Step 1: Adicionar argumentos e handlers em `src/main.rs`**
Implementar o dispatch para cada novo subcomando.

- [ ] **Step 2: Executar suíte completa de testes unitários**
Run: `cargo test`
Expected: Todos os testes passando (30+ testes).

- [ ] **Step 3: Compilar binário de release otimizado**
Run: `cargo build --release`
Expected: `target/release/cli_admin.exe` gerado com sucesso.

- [ ] **Step 4: Atualizar documentação no `README.md`**
Registrar todos os novos comandos, atalhos das 5 abas e configuração de bancos de dados.
