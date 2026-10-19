# Aba Bancos (teste de conexão e ferramentas de cliente) — Plano de implementação

> **Para agentes:** SUB-SKILL OBRIGATÓRIA: usar `superpowers:subagent-driven-development` (recomendado) ou `superpowers:executing-plans` para executar este plano tarefa por tarefa. Os passos usam a sintaxe de caixa de seleção (`- [ ]`).

**Objetivo:** aba **[6] Bancos** e subcomando `db` para testar a conexão (3 níveis) com Oracle, PostgreSQL e SQL Server e abrir os clientes que o usuário colocou em `tools\db_clients\`.

**Arquitetura:** TCP e protocolo em Rust (sem dependência nova); login pelo **cliente externo** do fabricante, com a senha **fora** dos argumentos; descoberta de ferramentas por varredura da pasta; tudo que demora roda em segundo plano.

**Tecnologias:** Rust (edição 2024), `tokio`, `walkdir`, `serde`/`toml`, ratatui 0.29 (todos já são dependências). **Nenhuma dependência nova.**

**Spec:** `docs/superpowers/specs/2026-10-17-aba-banco-de-dados-design.md`

**Pré-requisito:** o plano do explorador (`2026-10-17-explorador-arquivos.md`) cria `src/audit.rs` e a aba `[5] Arquivos`. Este plano **assume** a numeração final: Arquivos `[5]`, **Bancos `[6]`**, Suporte `[7]`. Se for executado antes do explorador, Bancos fica `[5]` e Suporte `[6]`; ajuste a Tarefa 9.

## Restrições globais

- `cargo build --offline` e `cargo test --offline`; projeto em `D:\W\DEV\GENERAL_TOOLS\CLI_ADMIN_V3`. Pacote e biblioteca: `cli_admin`; executável: `cli_admin_v3`.
- Comentários e mensagens em **pt-BR**, com acentos.
- **A senha nunca aparece em argumento de linha de comando, em `config.toml`, em log ou em tela.** Vem de uma variável de ambiente cujo **nome** está na configuração.
- **Nenhuma consulta SQL digitada pelo usuário** nesta versão. Só consultas fixas de versão.
- **A ferramenta não baixa nem distribui binários de fabricante.** Só descobre o que o usuário copiou para a pasta.
- Todo processo filho tem tempo limite e é encerrado ao estourar; a saída é truncada em 2 KB e mascarada.
- Nada de E/S lenta na thread da interface: testes de nível 3 e a descoberta rodam em segundo plano.
- **Commits só se o usuário pedir**, adicionando apenas caminhos de `GENERAL_TOOLS/CLI_ADMIN_V3` (a branch atual tem muitas alterações não relacionadas).
- **Questões em aberto do spec (§2) não bloqueiam este plano:** o "SPDB" é uma entrada `[[database.tools]]` no `config.toml`, sem código novo.

## Mapa de arquivos

| Arquivo | Ação | Responsabilidade |
|---|---|---|
| `src/config.rs` | modificar | `DatabaseConfig`, `DbInstance`, `DbToolEntry` |
| `src/modules/database/mod.rs` | criar | Reexporta; função `testar` |
| `src/modules/database/model.rs` | criar | `DbKind`, `Estado`, `Etapa`, `ResultadoTeste`, `InstanciaResolvida` |
| `src/modules/database/tools.rs` | criar | `Ferramenta`, `descobrir`, `resolver_tools_dir` |
| `src/modules/database/probe.rs` | criar | Nível 1 e nível 2 |
| `src/modules/database/client.rs` | criar | Nível 3: `montar`, `executar`, `abrir_console`, `abrir_grafica` |
| `src/modules/mod.rs` | modificar | `pub mod database;` |
| `src/tui/db.rs` | criar | `DbState`: estado, teclas, testes em segundo plano |
| `src/tui/db_ui.rs` | criar | `draw_db_tab` |
| `src/tui/app.rs`, `src/tui/ui.rs`, `src/tui/mod.rs` | modificar | Ligação da aba |
| `src/main.rs` | modificar | Subcomando `db list/tools/test` |
| `tools/README.md`, `.gitignore`, `config.toml` | criar/modificar | Pasta de ferramentas e exemplo |
| `tests/db_tests.rs` | criar | Integração: servidor falso, comando, tela |

---

### Tarefa 1: Configuração

**Arquivos:** modificar `src/config.rs`; testes em `tests/db_tests.rs` (criar).

**Interfaces:**
- Produz: `AppConfig.database: DatabaseConfig { tools_dir: String, timeout_ms: u64, instances: Vec<DbInstance>, tools: Vec<DbToolEntry> }`; `DbInstance { name, kind, host, port: Option<u16>, service: Option<String>, user: Option<String>, password_env: Option<String>, tool: Option<String> }`; `DbToolEntry { id, label, kind, exe, gui: bool, args: Vec<String> }`.

- [ ] **Passo 1: Teste que falha** (`tests/db_tests.rs`)

```rust
use cli_admin::config::AppConfig;

const AMOSTRA: &str = r#"
[database]
tools_dir = "tools/db_clients"
timeout_ms = 3000

[[database.instances]]
name = "TC_ORACLE"
kind = "oracle"
host = "srv-oracle01"
port = 1521
service = "ORCLPDB"
user = "infodba"
password_env = "TC_ORACLE_PWD"

[[database.tools]]
id = "spdb"
label = "SPDB"
kind = "oracle"
exe = "spdb/spdb.exe"
gui = true
"#;

#[test]
fn config_de_banco_le_instancias_e_ferramentas() {
    let cfg: AppConfig = toml::from_str(AMOSTRA).unwrap();
    assert_eq!(cfg.database.timeout_ms, 3000);
    assert_eq!(cfg.database.instances.len(), 1);
    let i = &cfg.database.instances[0];
    assert_eq!(i.name, "TC_ORACLE");
    assert_eq!(i.password_env.as_deref(), Some("TC_ORACLE_PWD"));
    assert_eq!(cfg.database.tools[0].id, "spdb");
    assert!(cfg.database.tools[0].gui);
}

#[test]
fn config_sem_secao_de_banco_usa_padroes() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.database.tools_dir, "tools/db_clients");
    assert_eq!(cfg.database.timeout_ms, 8000);
    assert!(cfg.database.instances.is_empty());
}

#[test]
fn config_nao_tem_campo_para_senha() {
    // A senha só pode vir do ambiente: `DbInstance` não deve aceitar `password`.
    let cfg: AppConfig = toml::from_str(
        r#"
[[database.instances]]
name = "x"
kind = "postgres"
host = "h"
password = "NAO-DEVE-SER-LIDA"
"#,
    )
    .unwrap();
    let serializado = toml::to_string(&cfg).unwrap();
    assert!(!serializado.contains("NAO-DEVE-SER-LIDA"), "a senha do arquivo deve ser ignorada");
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test db_tests` → erro de compilação (`cfg.database`).

- [ ] **Passo 3: Implementação** (em `src/config.rs`)

Em `AppConfig` e em `impl Default`: acrescentar `#[serde(default)] pub database: DatabaseConfig,` e `database: DatabaseConfig::default(),`. Depois, junto das outras seções:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Pasta com os clientes dos fabricantes. Relativa à pasta do executável, ou absoluta.
    #[serde(default = "default_tools_dir")]
    pub tools_dir: String,
    /// Tempo limite de cada etapa do teste, em ms.
    #[serde(default = "default_db_timeout")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub instances: Vec<DbInstance>,
    /// Ferramentas fora da tabela padrão (é aqui que entra, por exemplo, o SPDB).
    #[serde(default)]
    pub tools: Vec<DbToolEntry>,
}

fn default_tools_dir() -> String {
    "tools/db_clients".to_string()
}
fn default_db_timeout() -> u64 {
    8000
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            tools_dir: default_tools_dir(),
            timeout_ms: default_db_timeout(),
            instances: Vec::new(),
            tools: Vec::new(),
        }
    }
}

/// Uma instância de banco. **Não existe campo de senha:** só o nome da variável de ambiente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbInstance {
    pub name: String,
    /// oracle | postgres | sqlserver
    pub kind: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    /// Oracle: service name do PDB (não SID). Postgres e SQL Server: nome do banco.
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub user: Option<String>,
    /// NOME da variável de ambiente que guarda a senha.
    #[serde(default)]
    pub password_env: Option<String>,
    /// Cliente preferido, como aparece na descoberta (ex.: "oracle/19c").
    #[serde(default)]
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbToolEntry {
    pub id: String,
    pub label: String,
    pub kind: String,
    /// Caminho do executável, relativo a `tools_dir`.
    pub exe: String,
    #[serde(default)]
    pub gui: bool,
    #[serde(default)]
    pub args: Vec<String>,
}
```

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --test db_tests` → 3 testes passam.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): configuração de bancos e ferramentas`.

---

### Tarefa 2: Modelo (tipos do teste)

**Arquivos:** criar `src/modules/database/mod.rs`, `src/modules/database/model.rs`; modificar `src/modules/mod.rs`.

**Interfaces:**
- Produz: `DbKind { Oracle, Postgres, SqlServer }` (`FromStr`, `porta_padrao()`, `rotulo()`); `Estado { Ok, Falha, Indisponivel }`; `Etapa { estado, ms: u128, detalhe: String }` com construtores `ok/falha/indisponivel`; `ResultadoTeste { n1, n2, n3: Option<Etapa> }`; `InstanciaResolvida { nome, kind, host, porta, servico: Option<String>, usuario: Option<String>, senha_env: Option<String>, ferramenta: Option<String> }`; `resolver(&DbInstance) -> Result<InstanciaResolvida, String>`.

- [ ] **Passo 1: Testes que falham** (no fim de `model.rs`)

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::config::DbInstance;

    fn inst(kind: &str) -> DbInstance {
        DbInstance {
            name: "x".into(),
            kind: kind.into(),
            host: "h".into(),
            port: None,
            service: None,
            user: None,
            password_env: None,
            tool: None,
        }
    }

    #[test]
    fn tipos_e_portas_padrao() {
        assert_eq!("oracle".parse::<DbKind>().unwrap().porta_padrao(), 1521);
        assert_eq!("PostgreSQL".parse::<DbKind>().unwrap().porta_padrao(), 5432);
        assert_eq!("mssql".parse::<DbKind>().unwrap().porta_padrao(), 1433);
        assert!("mysql".parse::<DbKind>().is_err());
    }

    #[test]
    fn resolver_usa_a_porta_padrao_e_recusa_tipo_ou_host_invalido() {
        let r = resolver(&inst("postgres")).unwrap();
        assert_eq!(r.porta, 5432);
        assert!(resolver(&inst("mysql")).is_err());
        let mut sem_host = inst("oracle");
        sem_host.host = "  ".into();
        assert!(resolver(&sem_host).is_err());
    }

    #[test]
    fn etapa_e_resultado() {
        assert_eq!(Etapa::ok(5, "x").estado, Estado::Ok);
        assert_eq!(Etapa::falha(5, "x").estado, Estado::Falha);
        let r = ResultadoTeste { n1: Some(Etapa::ok(1, "")), n2: Some(Etapa::indisponivel("")), n3: None };
        assert!(r.sem_falhas(), "indisponível não conta como falha");
        let r2 = ResultadoTeste { n1: Some(Etapa::falha(1, "")), n2: None, n3: None };
        assert!(!r2.sem_falhas());
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib model` → erro de compilação.

- [ ] **Passo 3: Implementação** (`src/modules/database/model.rs`)

```rust
//! Tipos do teste de conexão. Sem E/S.

use crate::config::DbInstance;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DbKind {
    Oracle,
    Postgres,
    SqlServer,
}

impl DbKind {
    pub fn porta_padrao(self) -> u16 {
        match self {
            DbKind::Oracle => 1521,
            DbKind::Postgres => 5432,
            DbKind::SqlServer => 1433,
        }
    }

    pub fn rotulo(self) -> &'static str {
        match self {
            DbKind::Oracle => "Oracle",
            DbKind::Postgres => "PostgreSQL",
            DbKind::SqlServer => "SQL Server",
        }
    }
}

impl FromStr for DbKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_lowercase().as_str() {
            "oracle" => Ok(DbKind::Oracle),
            "postgres" | "postgresql" | "pg" => Ok(DbKind::Postgres),
            "sqlserver" | "mssql" => Ok(DbKind::SqlServer),
            outro => Err(format!("tipo de banco desconhecido: '{outro}' (use oracle, postgres ou sqlserver)")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    Ok,
    Falha,
    /// Não foi possível testar (sem cliente, sem senha, nível ainda não implementado). Não é falha.
    Indisponivel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Etapa {
    pub estado: Estado,
    pub ms: u128,
    pub detalhe: String,
}

impl Etapa {
    pub fn ok(ms: u128, detalhe: impl Into<String>) -> Self {
        Self { estado: Estado::Ok, ms, detalhe: detalhe.into() }
    }
    pub fn falha(ms: u128, detalhe: impl Into<String>) -> Self {
        Self { estado: Estado::Falha, ms, detalhe: detalhe.into() }
    }
    pub fn indisponivel(detalhe: impl Into<String>) -> Self {
        Self { estado: Estado::Indisponivel, ms: 0, detalhe: detalhe.into() }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResultadoTeste {
    pub n1: Option<Etapa>,
    pub n2: Option<Etapa>,
    pub n3: Option<Etapa>,
}

impl ResultadoTeste {
    /// `true` se nenhuma etapa executada falhou. Etapa indisponível não conta como falha.
    pub fn sem_falhas(&self) -> bool {
        [&self.n1, &self.n2, &self.n3]
            .into_iter()
            .flatten()
            .all(|e| e.estado != Estado::Falha)
    }
}

/// Instância já validada, com o tipo convertido e a porta resolvida.
#[derive(Debug, Clone)]
pub struct InstanciaResolvida {
    pub nome: String,
    pub kind: DbKind,
    pub host: String,
    pub porta: u16,
    pub servico: Option<String>,
    pub usuario: Option<String>,
    pub senha_env: Option<String>,
    pub ferramenta: Option<String>,
}

pub fn resolver(i: &DbInstance) -> Result<InstanciaResolvida, String> {
    let kind: DbKind = i.kind.parse()?;
    if i.host.trim().is_empty() {
        return Err(format!("instância '{}' sem host", i.name));
    }
    Ok(InstanciaResolvida {
        nome: i.name.clone(),
        kind,
        host: i.host.trim().to_string(),
        porta: i.port.unwrap_or_else(|| kind.porta_padrao()),
        servico: i.service.clone(),
        usuario: i.user.clone(),
        senha_env: i.password_env.clone(),
        ferramenta: i.tool.clone(),
    })
}
```

`src/modules/database/mod.rs`:

```rust
pub mod model;

pub use model::{DbKind, Estado, Etapa, InstanciaResolvida, ResultadoTeste, resolver};
```

`src/modules/mod.rs`: acrescentar `pub mod database;`.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib model` → 3 testes passam.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): modelo do teste de conexão de bancos`.

---

### Tarefa 3: Descoberta das ferramentas

**Arquivos:** criar `src/modules/database/tools.rs`; modificar `mod.rs`.

**Interfaces:**
- Consome: `DbKind`, `config::DbToolEntry`.
- Produz: `Ferramenta { id, rotulo, kind: DbKind, caminho: PathBuf, versao: String, gui: bool, args: Vec<String> }`; `descobrir(raiz: &Path, extras: &[DbToolEntry]) -> Vec<Ferramenta>`; `resolver_tools_dir(valor: &str) -> PathBuf`.

- [ ] **Passo 1: Testes que falham** (no fim de `tools.rs`)

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::config::DbToolEntry;

    fn arvore(nome: &str) -> PathBuf {
        let raiz = std::env::temp_dir().join(format!("cli_admin_tools_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        for rel in [
            r"oracle\19c\sqlplus.exe",
            r"oracle\sqlcl\bin\sql.exe",
            r"postgres\16\bin\psql.exe",
            r"postgres\pgadmin\PGADMIN4.EXE",
            r"sqlserver\18\sqlcmd.exe",
            r"spdb\spdb.exe",
            r"oracle\19c\leia-me.txt",
        ] {
            let p = raiz.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        raiz
    }

    #[test]
    fn descobre_as_ferramentas_conhecidas_e_ignora_o_resto() {
        let raiz = arvore("conhecidas");
        let f = descobrir(&raiz, &[]);
        let ids: Vec<&str> = f.iter().map(|x| x.rotulo.as_str()).collect();
        assert!(ids.contains(&"SQL*Plus") && ids.contains(&"SQLcl") && ids.contains(&"psql"));
        assert!(ids.contains(&"sqlcmd") && ids.contains(&"pgAdmin 4"));
        assert_eq!(f.len(), 5, "o spdb.exe e o txt não entram sem configuração: {ids:?}");
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn versao_vem_da_pasta_e_gui_do_tipo() {
        let raiz = arvore("versao");
        let f = descobrir(&raiz, &[]);
        let psql = f.iter().find(|x| x.rotulo == "psql").unwrap();
        assert_eq!(psql.versao, "16");
        assert!(!psql.gui);
        let sqlplus = f.iter().find(|x| x.rotulo == "SQL*Plus").unwrap();
        assert_eq!(sqlplus.versao, "19c");
        let pg = f.iter().find(|x| x.rotulo == "pgAdmin 4").unwrap();
        assert!(pg.gui);
        assert_eq!(pg.versao, "—", "sem pasta de versão");
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn ferramenta_extra_da_configuracao_entra() {
        let raiz = arvore("extra");
        let extra = DbToolEntry {
            id: "spdb".into(),
            label: "SPDB".into(),
            kind: "oracle".into(),
            exe: r"spdb\spdb.exe".into(),
            gui: true,
            args: vec![],
        };
        let f = descobrir(&raiz, &[extra]);
        let spdb = f.iter().find(|x| x.id == "spdb").expect("SPDB deve aparecer");
        assert!(spdb.gui);
        assert!(spdb.caminho.ends_with("spdb.exe"));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn extra_cujo_executavel_nao_existe_e_ignorado() {
        let raiz = arvore("extra_falta");
        let extra = DbToolEntry {
            id: "fantasma".into(),
            label: "Fantasma".into(),
            kind: "postgres".into(),
            exe: "nao/existe.exe".into(),
            gui: false,
            args: vec![],
        };
        assert!(descobrir(&raiz, &[extra]).iter().all(|x| x.id != "fantasma"));
        let _ = std::fs::remove_dir_all(&raiz);
    }

    #[test]
    fn pasta_inexistente_devolve_lista_vazia() {
        assert!(descobrir(Path::new(r"Z:\nao\existe"), &[]).is_empty());
    }

    #[test]
    fn caminho_absoluto_e_mantido() {
        assert_eq!(resolver_tools_dir(r"D:\ferramentas"), PathBuf::from(r"D:\ferramentas"));
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib tools` → erro de compilação.

- [ ] **Passo 3: Implementação** (`src/modules/database/tools.rs`)

```rust
//! Descoberta dos clientes de banco na pasta de ferramentas. Não executa nenhum binário
//! (descobrir versão rodando o programa custa tempo e pode abrir janela).

use super::model::DbKind;
use crate::config::DbToolEntry;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Ferramenta {
    pub id: String,
    pub rotulo: String,
    pub kind: DbKind,
    pub caminho: PathBuf,
    pub versao: String,
    pub gui: bool,
    pub args: Vec<String>,
}

/// (arquivo em minúsculas, tipo, gráfica?, rótulo)
const CONHECIDAS: [(&str, DbKind, bool, &str); 6] = [
    ("sqlplus.exe", DbKind::Oracle, false, "SQL*Plus"),
    ("sql.exe", DbKind::Oracle, false, "SQLcl"),
    ("psql.exe", DbKind::Postgres, false, "psql"),
    ("sqlcmd.exe", DbKind::SqlServer, false, "sqlcmd"),
    ("pgadmin4.exe", DbKind::Postgres, true, "pgAdmin 4"),
    ("sqldeveloper.exe", DbKind::Oracle, true, "SQL Developer"),
];

/// Pasta que parece versão: começa com dígito (`19c`, `16`, `21.3`) ou `v` seguido de dígito.
fn parece_versao(nome: &str) -> bool {
    let mut c = nome.chars();
    match c.next() {
        Some(p) if p.is_ascii_digit() => true,
        Some('v') | Some('V') => c.next().map(|x| x.is_ascii_digit()).unwrap_or(false),
        _ => false,
    }
}

fn versao_de(raiz: &Path, arquivo: &Path) -> String {
    arquivo
        .ancestors()
        .skip(1)
        .take_while(|a| *a != raiz)
        .filter_map(|a| a.file_name().map(|n| n.to_string_lossy().to_string()))
        .find(|n| parece_versao(n))
        .unwrap_or_else(|| "—".to_string())
}

/// Procura até 5 níveis abaixo de `raiz` e junta as entradas extras da configuração.
pub fn descobrir(raiz: &Path, extras: &[DbToolEntry]) -> Vec<Ferramenta> {
    let mut saida = Vec::new();
    if raiz.is_dir() {
        for ent in WalkDir::new(raiz).max_depth(5).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            if !ent.file_type().is_file() {
                continue;
            }
            let nome = ent.file_name().to_string_lossy().to_lowercase();
            if let Some((_, kind, gui, rotulo)) = CONHECIDAS.iter().find(|c| c.0 == nome) {
                saida.push(Ferramenta {
                    id: format!("{}:{}", nome, ent.path().display()),
                    rotulo: rotulo.to_string(),
                    kind: *kind,
                    caminho: ent.path().to_path_buf(),
                    versao: versao_de(raiz, ent.path()),
                    gui: *gui,
                    args: Vec::new(),
                });
            }
        }
    }
    for e in extras {
        let caminho = raiz.join(&e.exe);
        let Ok(kind) = e.kind.parse::<DbKind>() else { continue };
        if caminho.is_file() {
            saida.push(Ferramenta {
                id: e.id.clone(),
                rotulo: e.label.clone(),
                kind,
                versao: versao_de(raiz, &caminho),
                caminho,
                gui: e.gui,
                args: e.args.clone(),
            });
        }
    }
    saida.sort_by(|a, b| (a.kind as u8, &a.rotulo, &a.versao).cmp(&(b.kind as u8, &b.rotulo, &b.versao)));
    saida
}

/// Absoluto: como está. Relativo: junto do executável; se não existir, a partir da pasta atual.
pub fn resolver_tools_dir(valor: &str) -> PathBuf {
    let p = PathBuf::from(valor);
    if p.is_absolute() {
        return p;
    }
    if let Some(ao_lado) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(&p))) {
        if ao_lado.exists() {
            return ao_lado;
        }
    }
    p
}
```

`DbKind` precisa de `#[repr(u8)]`? A ordenação usa `a.kind as u8`: para enum sem campos isso é permitido (cast). OK.

`mod.rs`: `pub mod tools;` e `pub use tools::{Ferramenta, descobrir, resolver_tools_dir};`.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib tools` → 6 testes passam.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): descoberta de clientes de banco na pasta de ferramentas`.

---

### Tarefa 4: Níveis 1 e 2

**Arquivos:** criar `src/modules/database/probe.rs`; modificar `mod.rs`; testes em `tests/db_tests.rs`.

**Interfaces:**
- Consome: `support::probe_port(host, port, timeout_ms) -> ProbeResult`; `Etapa`.
- Produz: `pub async fn nivel1(host: &str, porta: u16, timeout_ms: u64) -> Etapa`; `pub const PEDIDO_SSL_POSTGRES: [u8; 8]`; `pub async fn nivel2_postgres(host: &str, porta: u16, timeout_ms: u64) -> Etapa`.

- [ ] **Passo 1: Testes que falham** (acrescentar a `tests/db_tests.rs`)

```rust
use cli_admin::modules::database::probe::{PEDIDO_SSL_POSTGRES, nivel1, nivel2_postgres};
use cli_admin::modules::database::Estado;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Servidor falso: lê 8 bytes, devolve `resposta` e confere que o pedido era o SSLRequest.
/// (Sem aceitar várias conexões, o teste da Tarefa 7 falha: o nível 1 consome a primeira.)
async fn servidor_falso(resposta: Option<u8>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = l.local_addr().unwrap().port();
    // Aceita VÁRIAS conexões: o nível 1 só conecta e fecha; o nível 2 conecta e fala.
    tokio::spawn(async move {
        while let Ok((mut s, _)) = l.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 8];
                if s.read_exact(&mut buf).await.is_ok() {
                    assert_eq!(buf, PEDIDO_SSL_POSTGRES, "pedido SSLRequest incorreto");
                    if let Some(r) = resposta {
                        let _ = s.write_all(&[r]).await;
                    }
                }
            });
        }
    });
    porta
}

#[test]
fn pedido_ssl_tem_o_codigo_80877103() {
    assert_eq!(&PEDIDO_SSL_POSTGRES[..4], &8u32.to_be_bytes());
    assert_eq!(u32::from_be_bytes(PEDIDO_SSL_POSTGRES[4..].try_into().unwrap()), 80877103);
}

#[tokio::test]
async fn nivel1_porta_aberta_e_fechada() {
    let porta = servidor_falso(None).await;
    assert_eq!(nivel1("127.0.0.1", porta, 2000).await.estado, Estado::Ok);
    // porta que acabou de ser liberada: ninguém escuta
    let livre = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    assert_eq!(nivel1("127.0.0.1", livre, 500).await.estado, Estado::Falha);
}

#[tokio::test]
async fn nivel2_postgres_aceita_s_e_n() {
    for resp in [b'S', b'N'] {
        let porta = servidor_falso(Some(resp)).await;
        let e = nivel2_postgres("127.0.0.1", porta, 2000).await;
        assert_eq!(e.estado, Estado::Ok, "{}", e.detalhe);
    }
}

#[tokio::test]
async fn nivel2_postgres_recusa_resposta_estranha_e_silencio() {
    let porta = servidor_falso(Some(b'X')).await;
    assert_eq!(nivel2_postgres("127.0.0.1", porta, 2000).await.estado, Estado::Falha);
    let porta = servidor_falso(None).await; // fecha sem responder
    assert_eq!(nivel2_postgres("127.0.0.1", porta, 800).await.estado, Estado::Falha);
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test db_tests` → erro de compilação (`probe`).

- [ ] **Passo 3: Implementação** (`src/modules/database/probe.rs`)

```rust
//! Nível 1 (TCP) e nível 2 (protocolo) do teste de conexão. Nenhuma credencial é usada.

use super::model::Etapa;
use crate::modules::support::probe_port;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn nivel1(host: &str, porta: u16, timeout_ms: u64) -> Etapa {
    let r = probe_port(host, porta, timeout_ms).await;
    if r.open {
        Etapa::ok(r.elapsed_ms, format!("{}:{} responde", r.host, r.port))
    } else {
        Etapa::falha(r.elapsed_ms, format!("{}:{} não responde (fechada, bloqueada ou host errado)", r.host, r.port))
    }
}

/// `SSLRequest` do PostgreSQL: tamanho (8) e código 80877103, ambos em 32 bits big-endian.
pub const PEDIDO_SSL_POSTGRES: [u8; 8] = [0, 0, 0, 8, 0x04, 0xD2, 0x16, 0x2F];

/// O servidor responde um único byte: `S` (aceita TLS) ou `N` (não aceita). Isso prova que há
/// um PostgreSQL do outro lado, sem enviar usuário nem senha.
pub async fn nivel2_postgres(host: &str, porta: u16, timeout_ms: u64) -> Etapa {
    let inicio = Instant::now();
    let limite = Duration::from_millis(timeout_ms);
    let r = tokio::time::timeout(limite, async {
        let mut s = tokio::net::TcpStream::connect((host, porta)).await.map_err(|e| e.to_string())?;
        s.write_all(&PEDIDO_SSL_POSTGRES).await.map_err(|e| e.to_string())?;
        let mut b = [0u8; 1];
        s.read_exact(&mut b).await.map_err(|e| format!("sem resposta: {e}"))?;
        Ok::<u8, String>(b[0])
    })
    .await;
    let ms = inicio.elapsed().as_millis();
    match r {
        Ok(Ok(b @ (b'S' | b'N'))) => Etapa::ok(
            ms,
            format!("PostgreSQL respondeu '{}' ({})", b as char, if b == b'S' { "aceita TLS" } else { "sem TLS" }),
        ),
        Ok(Ok(outro)) => Etapa::falha(ms, format!("resposta inesperada (0x{outro:02X}): não parece PostgreSQL")),
        Ok(Err(e)) => Etapa::falha(ms, e),
        Err(_) => Etapa::falha(ms, "tempo esgotado esperando resposta do servidor"),
    }
}
```

`mod.rs`: `pub mod probe;`.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --test db_tests` → 7 testes passam (3 da Tarefa 1 + 4 novos).
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): testes de TCP e de protocolo PostgreSQL`.

---

### Tarefa 5: Nível 3 — montar o comando sem a senha nos argumentos

**Arquivos:** criar `src/modules/database/client.rs`; modificar `mod.rs`.

**Interfaces:**
- Consome: `DbKind`, `InstanciaResolvida`.
- Produz: `ComandoExterno { programa: PathBuf, args: Vec<String>, env: Vec<(String, String)>, stdin: Option<String> }`; `pub fn montar(inst: &InstanciaResolvida, programa: &Path, senha: &str, timeout_ms: u64) -> Result<ComandoExterno, String>`; `pub fn ler_senha(nome_da_variavel: &str) -> Result<String, String>`; `pub fn reconhece_erro(saida: &str) -> bool`.

- [ ] **Passo 1: Testes que falham** (no fim de `client.rs`)

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::modules::database::model::{DbKind, Estado, InstanciaResolvida};

    fn inst(kind: DbKind) -> InstanciaResolvida {
        InstanciaResolvida {
            nome: "t".into(),
            kind,
            host: "srv".into(),
            porta: kind.porta_padrao(),
            servico: Some("meubanco".into()),
            usuario: Some("andre".into()),
            senha_env: Some("X".into()),
            ferramenta: None,
        }
    }

    const SENHA: &str = "S3nh@-Super-Secreta";

    #[test]
    fn a_senha_nunca_vai_nos_argumentos() {
        for (kind, exe) in [
            (DbKind::Oracle, r"C:\t\sqlplus.exe"),
            (DbKind::Postgres, r"C:\t\psql.exe"),
            (DbKind::SqlServer, r"C:\t\sqlcmd.exe"),
        ] {
            let c = montar(&inst(kind), Path::new(exe), SENHA, 5000).unwrap();
            assert!(c.args.iter().all(|a| !a.contains(SENHA)), "senha em argumento ({kind:?}): {:?}", c.args);
            assert!(
                c.env.iter().any(|(_, v)| v == SENHA) || c.stdin.as_deref().is_some_and(|s| s.contains(SENHA)),
                "a senha precisa chegar por ambiente ou entrada padrão ({kind:?})"
            );
        }
    }

    #[test]
    fn oracle_usa_entrada_padrao_e_service_name() {
        let c = montar(&inst(DbKind::Oracle), Path::new(r"C:\t\sqlplus.exe"), SENHA, 5000).unwrap();
        assert!(c.args.contains(&"-L".to_string()), "-L evita pedir a senha de novo");
        assert!(c.args.iter().any(|a| a == "andre@//srv:1521/meubanco"));
        assert!(c.stdin.unwrap().starts_with(&format!("{SENHA}\n")), "a senha é a primeira linha");
        assert!(c.env.is_empty());
    }

    #[test]
    fn postgres_usa_variavel_de_ambiente_e_nao_pergunta() {
        let c = montar(&inst(DbKind::Postgres), Path::new(r"C:\t\psql.exe"), SENHA, 5000).unwrap();
        assert!(c.args.contains(&"-w".to_string()), "-w: nunca pedir senha");
        assert!(c.env.contains(&("PGPASSWORD".to_string(), SENHA.to_string())));
        assert!(c.stdin.is_none());
    }

    #[test]
    fn sqlserver_usa_variavel_de_ambiente() {
        let c = montar(&inst(DbKind::SqlServer), Path::new(r"C:\t\sqlcmd.exe"), SENHA, 5000).unwrap();
        assert!(c.env.contains(&("SQLCMDPASSWORD".to_string(), SENHA.to_string())));
        assert!(c.args.iter().any(|a| a == "srv,1433"));
    }

    #[test]
    fn exige_usuario() {
        let mut i = inst(DbKind::Oracle);
        i.usuario = None;
        assert!(montar(&i, Path::new("x.exe"), SENHA, 5000).is_err());
    }

    #[test]
    fn rejeita_valores_que_parecem_opcao_ou_tem_quebra_de_linha() {
        // um host ou nome de banco não pode virar uma opção do cliente nem injetar linhas
        let mut i = inst(DbKind::Postgres);
        i.host = "-oProxyCommand=calc".into();
        assert!(montar(&i, Path::new("psql.exe"), SENHA, 5000).is_err());
        let mut j = inst(DbKind::Oracle);
        j.servico = Some("srv\nDROP USER x".into());
        assert!(montar(&j, Path::new("sqlplus.exe"), SENHA, 5000).is_err());
    }

    #[test]
    fn ler_senha_so_devolve_o_nome_no_erro() {
        let e = ler_senha("CLI_ADMIN_VARIAVEL_QUE_NAO_EXISTE_123").unwrap_err();
        assert!(e.contains("CLI_ADMIN_VARIAVEL_QUE_NAO_EXISTE_123"));
    }

    #[test]
    fn reconhece_mensagens_de_erro_dos_clientes() {
        assert!(reconhece_erro("ORA-01017: invalid username/password; logon denied"));
        assert!(reconhece_erro("SP2-0640: Not connected"));
        assert!(reconhece_erro("psql: error: connection to server failed: FATAL:  password authentication failed"));
        assert!(reconhece_erro("Msg 18456, Level 14 ... Login failed for user 'x'."));
        assert!(!reconhece_erro("PostgreSQL 16.2 on x86_64-pc-windows"));
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib client` → erro de compilação.

- [ ] **Passo 3: Implementação** (início de `src/modules/database/client.rs`)

```rust
//! Nível 3: login e consulta de versão pelo **cliente externo** do fabricante.
//! A senha chega por variável de ambiente do processo filho ou pela entrada padrão,
//! **nunca** por argumento (argumentos aparecem na lista de processos).

use super::model::{DbKind, Etapa, InstanciaResolvida};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ComandoExterno {
    pub programa: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub stdin: Option<String>,
}

/// Valor que vai para um argumento: sem quebra de linha e sem cara de opção (`-x`).
fn seguro(campo: &str, valor: &str) -> Result<(), String> {
    if valor.is_empty() || valor.starts_with('-') || valor.chars().any(|c| c.is_control()) {
        return Err(format!("valor inválido em '{campo}'"));
    }
    Ok(())
}

pub fn ler_senha(nome_da_variavel: &str) -> Result<String, String> {
    match std::env::var(nome_da_variavel) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(format!("a variável de ambiente {nome_da_variavel} não está definida ou está vazia")),
    }
}

/// Monta o comando de consulta fixa de versão. Função pura: não executa nada.
pub fn montar(
    inst: &InstanciaResolvida,
    programa: &Path,
    senha: &str,
    timeout_ms: u64,
) -> Result<ComandoExterno, String> {
    let usuario = inst.usuario.as_deref().ok_or("a instância não tem 'user'")?;
    seguro("user", usuario)?;
    seguro("host", &inst.host)?;
    let servico = inst.servico.as_deref().unwrap_or("");
    if !servico.is_empty() {
        seguro("service", servico)?;
    }
    let segundos = (timeout_ms / 1000).max(1).to_string();

    let mut c = ComandoExterno { programa: programa.to_path_buf(), args: vec![], env: vec![], stdin: None };
    match inst.kind {
        DbKind::Oracle => {
            if servico.is_empty() {
                return Err("Oracle exige 'service' (service name do PDB, não o SID)".into());
            }
            // -S silencioso, -L tenta o login uma vez só. Sem senha no argumento, o cliente a lê da entrada padrão.
            c.args = vec!["-S".into(), "-L".into(), format!("{usuario}@//{}:{}/{servico}", inst.host, inst.porta)];
            c.stdin = Some(format!(
                "{senha}\nWHENEVER SQLERROR EXIT FAILURE\nSET HEADING OFF FEEDBACK OFF\nSELECT banner FROM v$version WHERE ROWNUM = 1;\nEXIT;\n"
            ));
        }
        DbKind::Postgres => {
            let banco = if servico.is_empty() { "postgres" } else { servico };
            c.args = vec![
                "-w".into(),
                "-h".into(),
                inst.host.clone(),
                "-p".into(),
                inst.porta.to_string(),
                "-U".into(),
                usuario.into(),
                "-d".into(),
                banco.into(),
                "-t".into(),
                "-A".into(),
                "-c".into(),
                "SELECT version()".into(),
            ];
            c.env = vec![("PGPASSWORD".into(), senha.into()), ("PGCONNECT_TIMEOUT".into(), segundos)];
        }
        DbKind::SqlServer => {
            c.args = vec![
                "-S".into(),
                format!("{},{}", inst.host, inst.porta),
                "-U".into(),
                usuario.into(),
                "-b".into(),
                "-h".into(),
                "-1".into(),
                "-l".into(),
                segundos,
                "-Q".into(),
                "SET NOCOUNT ON; SELECT @@VERSION".into(),
            ];
            if !servico.is_empty() {
                c.args.extend(["-d".into(), servico.into()]);
            }
            c.env = vec![("SQLCMDPASSWORD".into(), senha.into())];
        }
    }
    Ok(c)
}

/// Mensagens típicas de falha dos três clientes. Os clientes nem sempre devolvem código de saída
/// diferente de zero quando o login falha, então a saída também é conferida.
pub fn reconhece_erro(saida: &str) -> bool {
    ["ORA-", "SP2-", "FATAL", "Login failed", "Msg 18456", "could not connect", "connection to server"]
        .iter()
        .any(|m| saida.contains(m))
}
```

`mod.rs`: `pub mod client;`.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib client` → 8 testes passam.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): comando de login por cliente externo sem senha em argumento`.

---

### Tarefa 6: Nível 3 — executar com tempo limite, truncar e mascarar

**Arquivos:** modificar `src/modules/database/client.rs`; testes no `mod testes` do mesmo arquivo.

**Interfaces:**
- Consome: `ComandoExterno`, `Etapa`, `texto::truncar`.
- Produz: `pub async fn executar(cmd: &ComandoExterno, timeout_ms: u64, segredo: &str) -> Etapa`; `pub fn abrir_console(...)` e `pub fn abrir_grafica(...)` ficam para a Tarefa 8.

- [ ] **Passo 1: Testes que falham** (dentro de `mod testes`, usando `cmd.exe` como cliente de mentira). **Use `cmd /D`**: sem o `/D`, o `cmd` executa o *AutoRun* do usuário (por exemplo `doskey`), que escreve na saída e muda o código de saída. Isso derrubou estes testes numa máquina real durante a validação do plano.

```rust
    fn falso(args: &[&str]) -> ComandoExterno {
        ComandoExterno {
            programa: PathBuf::from("cmd.exe"),
            args: args.iter().map(|s| s.to_string()).collect(),
            env: vec![],
            stdin: None,
        }
    }

    #[tokio::test]
    async fn executar_devolve_a_saida_e_mascara_a_senha() {
        let e = executar(&falso(&["/D", "/C", "echo", "PostgreSQL 16 senha=S3nh@Secreta ok"]), 5000, "S3nh@Secreta").await;
        assert_eq!(e.estado, Estado::Ok, "{}", e.detalhe);
        assert!(e.detalhe.contains("PostgreSQL 16"));
        assert!(!e.detalhe.contains("S3nh@Secreta"), "a senha não pode aparecer: {}", e.detalhe);
        assert!(e.detalhe.contains("***"));
    }

    #[tokio::test]
    async fn executar_estoura_o_tempo_e_encerra() {
        // ping.exe DIRETO, sem cmd: matar o processo encerra tudo. Com `cmd /C ping`, o ping filho
        // sobra por ~30 s segurando os canais e a suíte inteira passa de 1 s para ~30 s.
        let inicio = Instant::now();
        let mut longo = falso(&["-n", "30", "127.0.0.1"]);
        longo.programa = PathBuf::from("ping.exe");
        let e = executar(&longo, 400, "x").await;
        assert_eq!(e.estado, Estado::Falha);
        assert!(e.detalhe.contains("tempo"), "{}", e.detalhe);
        assert!(inicio.elapsed() < Duration::from_secs(5), "deveria cortar perto do limite");
    }

    #[tokio::test]
    async fn executar_trunca_saida_longa() {
        let longa = "A".repeat(5000);
        let e = executar(&falso(&["/D", "/C", "echo", &longa]), 5000, "x").await;
        assert!(e.detalhe.chars().count() <= 2048, "{} caracteres", e.detalhe.chars().count());
    }

    #[tokio::test]
    async fn executar_trata_mensagem_de_erro_na_saida_como_falha() {
        let e = executar(&falso(&["/D", "/C", "echo", "ORA-01017: invalid username/password"]), 5000, "x").await;
        assert_eq!(e.estado, Estado::Falha, "saída com ORA- é falha mesmo com código 0");
    }

    #[tokio::test]
    async fn programa_sem_pasta_nao_quebra_o_path_do_filho() {
        // 'cmd.exe' sem diretório: o PATH do filho não pode ganhar uma entrada vazia na frente,
        // senão o cmd não acha o 'ping' (regressão encontrada ao validar este plano)
        let e = executar(&falso(&["/D", "/C", "ping", "-n", "1", "127.0.0.1"]), 8000, "x").await;
        assert_eq!(e.estado, Estado::Ok, "{}", e.detalhe);
    }

    #[tokio::test]
    async fn executar_programa_inexistente_e_falha_clara() {
        let mut c = falso(&[]);
        c.programa = PathBuf::from(r"C:\nao\existe\sqlplus.exe");
        let e = executar(&c, 2000, "x").await;
        assert_eq!(e.estado, Estado::Falha);
    }
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib client` → erro de compilação (`executar`).

- [ ] **Passo 3: Implementação** (acrescentar a `client.rs`, antes de `mod testes`)

```rust
const LIMITE_SAIDA: usize = 2048;

fn mascarar(texto: &str, segredo: &str) -> String {
    if segredo.is_empty() { texto.to_string() } else { texto.replace(segredo, "***") }
}

/// Executa o cliente com tempo limite. A saída (stdout + stderr) é mascarada e truncada.
/// `segredo` é a senha, para ser removida de qualquer texto devolvido.
pub async fn executar(cmd: &ComandoExterno, timeout_ms: u64, segredo: &str) -> Etapa {
    let inicio = Instant::now();
    let mut c = Command::new(&cmd.programa);
    c.args(&cmd.args)
        .envs(cmd.env.iter().map(|(k, v)| (k, v)))
        .stdin(if cmd.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Clientes Oracle (Instant Client) precisam da própria pasta no PATH para achar as DLLs.
    // `.filter(...)`: um programa sem pasta tem `parent()` vazio, e um PATH com entrada vazia na
    // frente (";C:\...") faz o cmd.exe deixar de achar qualquer programa (visto na validação do plano).
    if let Some(pasta) = cmd.programa.parent().filter(|p| !p.as_os_str().is_empty()) {
        let atual = std::env::var("PATH").unwrap_or_default();
        c.env("PATH", format!("{};{atual}", pasta.display()));
    }
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }

    let mut filho = match c.spawn() {
        Ok(f) => f,
        Err(e) => return Etapa::falha(inicio.elapsed().as_millis(), format!("não foi possível iniciar o cliente: {e}")),
    };
    if let (Some(texto), Some(mut stdin)) = (cmd.stdin.clone(), filho.stdin.take()) {
        let _ = stdin.write_all(texto.as_bytes()).await;
        drop(stdin);
    }

    let espera = tokio::time::timeout(Duration::from_millis(timeout_ms), filho.wait_with_output()).await;
    let ms = inicio.elapsed().as_millis();
    match espera {
        Err(_) => Etapa::falha(ms, "tempo esgotado: o cliente foi encerrado"),
        Ok(Err(e)) => Etapa::falha(ms, format!("erro ao esperar o cliente: {e}")),
        Ok(Ok(saida)) => {
            let texto = format!(
                "{}{}",
                String::from_utf8_lossy(&saida.stdout),
                String::from_utf8_lossy(&saida.stderr)
            );
            let texto = crate::texto::truncar(mascarar(texto.trim(), segredo).trim(), LIMITE_SAIDA);
            if saida.status.success() && !reconhece_erro(&texto) {
                Etapa::ok(ms, texto)
            } else {
                Etapa::falha(ms, texto)
            }
        }
    }
}
```

Observação: `tokio::process::Command` tem `creation_flags` próprio no Windows, então **não** importe `CommandExt` aqui (geraria aviso de import sem uso). Se o compilador acusar que o método não existe, importe `std::os::windows::process::CommandExt` apenas nesse caso. Já o `abrir_console`/`abrir_grafica` da Tarefa 8 usam `std::process::Command` e importam `CommandExt` dentro do bloco `#[cfg(windows)]`.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib client` → 13 testes passam. Atenção: com `wait_with_output` e `kill_on_drop`, o cancelamento por tempo descarta o futuro e encerra o processo; o teste de tempo confirma isso.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): execução do cliente de banco com tempo limite e máscara de senha`.

---

### Tarefa 7: Orquestração dos três níveis

**Arquivos:** modificar `src/modules/database/mod.rs`; testes em `tests/db_tests.rs`.

**Interfaces:**
- Consome: `probe::{nivel1, nivel2_postgres}`, `client::{montar, executar, ler_senha}`, `Ferramenta`.
- Produz: `pub async fn testar(inst: &InstanciaResolvida, ferramentas: &[Ferramenta], timeout_ms: u64, ate_nivel: u8) -> ResultadoTeste`; `pub fn escolher_cliente<'a>(inst: &InstanciaResolvida, f: &'a [Ferramenta]) -> Option<&'a Ferramenta>`.

- [ ] **Passo 1: Testes que falham** (acrescentar a `tests/db_tests.rs`)

```rust
use cli_admin::modules::database::{Ferramenta, DbKind, InstanciaResolvida, escolher_cliente, testar};
use std::path::PathBuf;

fn instancia(kind: DbKind, porta: u16) -> InstanciaResolvida {
    InstanciaResolvida {
        nome: "t".into(),
        kind,
        host: "127.0.0.1".into(),
        porta,
        servico: Some("db".into()),
        usuario: Some("u".into()),
        senha_env: Some("CLI_ADMIN_TESTE_SENHA_NAO_DEFINIDA".into()),
        ferramenta: None,
    }
}

fn ferramenta(kind: DbKind, rotulo: &str, gui: bool, versao: &str) -> Ferramenta {
    Ferramenta {
        id: rotulo.into(),
        rotulo: rotulo.into(),
        kind,
        caminho: PathBuf::from(format!(r"C:\t\{rotulo}.exe")),
        versao: versao.into(),
        gui,
        args: vec![],
    }
}

#[tokio::test]
async fn porta_fechada_para_no_nivel_1() {
    let livre = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let r = testar(&instancia(DbKind::Postgres, livre), &[], 500, 3).await;
    assert_eq!(r.n1.as_ref().unwrap().estado, Estado::Falha);
    assert!(r.n2.is_none() && r.n3.is_none(), "não testa os níveis seguintes sem TCP");
    assert!(!r.sem_falhas());
}

#[tokio::test]
async fn postgres_sem_cliente_nem_senha_fica_indisponivel_no_nivel_3() {
    let porta = servidor_falso(Some(b'N')).await;
    let r = testar(&instancia(DbKind::Postgres, porta), &[], 2000, 3).await;
    assert_eq!(r.n1.as_ref().unwrap().estado, Estado::Ok);
    assert_eq!(r.n2.as_ref().unwrap().estado, Estado::Ok);
    let n3 = r.n3.as_ref().unwrap();
    assert_eq!(n3.estado, Estado::Indisponivel);
    assert!(n3.detalhe.contains("psql") || n3.detalhe.contains("cliente"), "{}", n3.detalhe);
    assert!(r.sem_falhas());
}

#[tokio::test]
async fn nivel_2_de_oracle_e_sqlserver_ainda_nao_existe() {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let porta = l.local_addr().unwrap().port();
    let r = testar(&instancia(DbKind::Oracle, porta), &[], 1000, 2).await;
    assert_eq!(r.n2.as_ref().unwrap().estado, Estado::Indisponivel);
}

#[test]
fn escolher_cliente_prefere_console_do_tipo_e_respeita_a_preferencia() {
    let fs = vec![
        ferramenta(DbKind::Oracle, "SQL Developer", true, "23"),
        ferramenta(DbKind::Oracle, "SQLcl", false, "24"),
        ferramenta(DbKind::Oracle, "SQL*Plus", false, "19c"),
        ferramenta(DbKind::Postgres, "psql", false, "16"),
    ];
    let mut i = instancia(DbKind::Oracle, 1521);
    assert_eq!(escolher_cliente(&i, &fs).unwrap().rotulo, "SQLcl", "o primeiro console do tipo");
    i.ferramenta = Some("SQL*Plus".into());
    assert_eq!(escolher_cliente(&i, &fs).unwrap().rotulo, "SQL*Plus");
    let pg = instancia(DbKind::SqlServer, 1433);
    assert!(escolher_cliente(&pg, &fs).is_none());
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test db_tests` → erro de compilação (`testar`).

- [ ] **Passo 3: Implementação** (`src/modules/database/mod.rs`, substituir o conteúdo)

```rust
pub mod client;
pub mod model;
pub mod probe;
pub mod tools;

pub use model::{DbKind, Estado, Etapa, InstanciaResolvida, ResultadoTeste, resolver};
pub use tools::{Ferramenta, descobrir, resolver_tools_dir};

/// Cliente de console para a instância: a preferência (`tool`, casando com o rótulo, o id ou
/// a versão) ou o primeiro de console do mesmo tipo. Ferramentas gráficas nunca são escolhidas aqui.
pub fn escolher_cliente<'a>(inst: &InstanciaResolvida, f: &'a [Ferramenta]) -> Option<&'a Ferramenta> {
    let do_tipo = || f.iter().filter(|x| x.kind == inst.kind && !x.gui);
    if let Some(pref) = &inst.ferramenta {
        let p = pref.to_lowercase();
        if let Some(achada) = do_tipo().find(|x| {
            x.rotulo.to_lowercase() == p || x.id.to_lowercase() == p || x.versao.to_lowercase() == p
        }) {
            return Some(achada);
        }
    }
    do_tipo().next()
}

/// Roda até `ate_nivel` (1 a 3). Para no primeiro nível que falhar no TCP.
pub async fn testar(
    inst: &InstanciaResolvida,
    ferramentas: &[Ferramenta],
    timeout_ms: u64,
    ate_nivel: u8,
) -> ResultadoTeste {
    let mut r = ResultadoTeste::default();
    let n1 = probe::nivel1(&inst.host, inst.porta, timeout_ms).await;
    let tcp_ok = n1.estado == Estado::Ok;
    r.n1 = Some(n1);
    if !tcp_ok || ate_nivel < 2 {
        return r;
    }

    r.n2 = Some(match inst.kind {
        DbKind::Postgres => probe::nivel2_postgres(&inst.host, inst.porta, timeout_ms).await,
        outro => Etapa::indisponivel(format!("protocolo de {} ainda não implementado (fase 2)", outro.rotulo())),
    });
    if ate_nivel < 3 {
        return r;
    }

    r.n3 = Some(nivel3(inst, ferramentas, timeout_ms).await);
    r
}

async fn nivel3(inst: &InstanciaResolvida, ferramentas: &[Ferramenta], timeout_ms: u64) -> Etapa {
    let Some(cliente) = escolher_cliente(inst, ferramentas) else {
        return Etapa::indisponivel(format!(
            "nenhum cliente de {} encontrado em tools/db_clients",
            inst.kind.rotulo()
        ));
    };
    let Some(nome_var) = inst.senha_env.as_deref() else {
        return Etapa::indisponivel("a instância não define 'password_env'");
    };
    let senha = match client::ler_senha(nome_var) {
        Ok(s) => s,
        Err(e) => return Etapa::indisponivel(e),
    };
    match client::montar(inst, &cliente.caminho, &senha, timeout_ms) {
        Ok(cmd) => client::executar(&cmd, timeout_ms, &senha).await,
        Err(e) => Etapa::falha(0, e),
    }
}
```

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --test db_tests` → 11 testes passam.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): orquestra os três níveis do teste de banco`.

---

### Tarefa 8: Abrir o cliente (console ou gráfico)

**Arquivos:** modificar `src/modules/database/client.rs`.

**Interfaces:**
- Produz: `pub fn abrir_console(inst: &InstanciaResolvida, cliente: &Ferramenta) -> Result<(), String>`; `pub fn abrir_grafica(cliente: &Ferramenta) -> Result<(), String>`; `pub fn argumentos_console(inst: &InstanciaResolvida) -> Result<Vec<String>, String>` (pura).

- [ ] **Passo 1: Testes que falham** (dentro de `mod testes`)

```rust
    #[test]
    fn argumentos_do_console_nao_levam_senha() {
        let o = argumentos_console(&inst(DbKind::Oracle)).unwrap();
        assert_eq!(o, vec!["andre@//srv:1521/meubanco".to_string()]);
        let p = argumentos_console(&inst(DbKind::Postgres)).unwrap();
        assert!(p.contains(&"-h".to_string()) && !p.contains(&"-w".to_string()), "no console o psql pode perguntar a senha");
        let s = argumentos_console(&inst(DbKind::SqlServer)).unwrap();
        assert!(s.contains(&"srv,1433".to_string()));
        for args in [o, p, s] {
            assert!(args.iter().all(|a| !a.contains("S3nh")), "nenhum segredo");
        }
    }

    #[test]
    fn argumentos_do_console_validam_os_campos() {
        let mut i = inst(DbKind::Postgres);
        i.usuario = Some("-x".into());
        assert!(argumentos_console(&i).is_err());
    }
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib client` → erro de compilação.

- [ ] **Passo 3: Implementação** (acrescentar a `client.rs`)

```rust
use super::tools::Ferramenta;

/// Argumentos para abrir o cliente **interativo**: o usuário digita a senha no próprio cliente.
pub fn argumentos_console(inst: &InstanciaResolvida) -> Result<Vec<String>, String> {
    let usuario = inst.usuario.as_deref().ok_or("a instância não tem 'user'")?;
    seguro("user", usuario)?;
    seguro("host", &inst.host)?;
    let servico = inst.servico.as_deref().unwrap_or("");
    if !servico.is_empty() {
        seguro("service", servico)?;
    }
    Ok(match inst.kind {
        DbKind::Oracle => vec![format!("{usuario}@//{}:{}/{servico}", inst.host, inst.porta)],
        DbKind::Postgres => vec![
            "-h".into(),
            inst.host.clone(),
            "-p".into(),
            inst.porta.to_string(),
            "-U".into(),
            usuario.into(),
            "-d".into(),
            if servico.is_empty() { "postgres".into() } else { servico.into() },
        ],
        DbKind::SqlServer => {
            let mut a = vec!["-S".into(), format!("{},{}", inst.host, inst.porta), "-U".into(), usuario.into()];
            if !servico.is_empty() {
                a.extend(["-d".into(), servico.into()]);
            }
            a
        }
    })
}

/// Abre o cliente numa **nova janela de console** (o usuário digita a senha lá).
pub fn abrir_console(inst: &InstanciaResolvida, cliente: &Ferramenta) -> Result<(), String> {
    let mut args = argumentos_console(inst)?;
    args.splice(0..0, cliente.args.iter().cloned());
    let mut c = std::process::Command::new(&cliente.caminho);
    c.args(&args);
    if let Some(pasta) = cliente.caminho.parent().filter(|p| !p.as_os_str().is_empty()) {
        let atual = std::env::var("PATH").unwrap_or_default();
        c.env("PATH", format!("{};{atual}", pasta.display()));
        c.current_dir(pasta);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        c.creation_flags(CREATE_NEW_CONSOLE);
    }
    c.spawn().map(|_| ()).map_err(|e| format!("não foi possível abrir {}: {e}", cliente.rotulo))
}

/// Abre uma ferramenta gráfica (pgAdmin, SQL Developer, SPDB) desacoplada da TUI.
pub fn abrir_grafica(cliente: &Ferramenta) -> Result<(), String> {
    let mut c = std::process::Command::new(&cliente.caminho);
    c.args(&cliente.args);
    if let Some(pasta) = cliente.caminho.parent().filter(|p| !p.as_os_str().is_empty()) {
        c.current_dir(pasta);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        c.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    c.spawn().map(|_| ()).map_err(|e| format!("não foi possível abrir {}: {e}", cliente.rotulo))
}
```

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib client` → 15 testes passam. **Abrir de fato** uma janela é verificação manual (Tarefa 11); não há teste automático que abra janelas.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): abrir cliente de console e ferramentas gráficas`.

---

### Tarefa 9: A aba na TUI

**Arquivos:** criar `src/tui/db.rs`, `src/tui/db_ui.rs`; modificar `src/tui/mod.rs`, `src/tui/app.rs`, `src/tui/ui.rs`; testes em `tests/db_tests.rs`.

**Interfaces:**
- Consome: `modules::database::*`, `config::DatabaseConfig`.
- Produz: `DbState::novo(&DatabaseConfig) -> Self` (resolve instâncias e descobre ferramentas, **em segundo plano**), `poll()`, `tecla(KeyEvent, &DatabaseConfig)`, campos públicos `instancias: Vec<LinhaDb>`, `ferramentas: Vec<Ferramenta>`, `selecionada: usize`, `foco: FocoDb`, `mensagem: Option<String>`; `LinhaDb { instancia: InstanciaResolvida, resultado: Option<ResultadoTeste>, testando: bool }`; `draw_db_tab(f, area, &mut DbState)`; `ActiveTab::Databases = 5`, `Support = 6`; teclas globais `6` → Bancos, `7` → Suporte.

- [ ] **Passo 1: Testes que falham** (acrescentar a `tests/db_tests.rs`)

```rust
use cli_admin::config::DatabaseConfig;
use cli_admin::tui::db::{DbState, FocoDb};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

fn cfg_com_porta(porta: u16) -> DatabaseConfig {
    toml::from_str::<AppConfig>(&format!(
        r#"
[database]
tools_dir = "pasta_que_nao_existe"
timeout_ms = 1500

[[database.instances]]
name = "LOCAL_PG"
kind = "postgres"
host = "127.0.0.1"
port = {porta}
service = "db"
user = "u"
"#
    ))
    .unwrap()
    .database
}

fn esperar_db(s: &mut DbState, mut ok: impl FnMut(&DbState) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    loop {
        s.poll();
        if ok(s) {
            return;
        }
        assert!(Instant::now() < limite, "tempo esgotado");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[tokio::test]
async fn aba_lista_instancias_e_testa_com_t_sem_travar() {
    let porta = servidor_falso(Some(b'N')).await;
    let cfg = cfg_com_porta(porta);
    let mut s = DbState::novo(&cfg);
    assert_eq!(s.instancias.len(), 1);
    assert_eq!(s.foco, FocoDb::Instancias);

    let t0 = Instant::now();
    s.tecla(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE), &cfg);
    assert!(t0.elapsed() < Duration::from_millis(500), "a tecla não pode esperar o teste");
    assert!(s.instancias[0].testando);

    esperar_db(&mut s, |s| !s.instancias[0].testando);
    let r = s.instancias[0].resultado.as_ref().unwrap();
    assert_eq!(r.n1.as_ref().unwrap().estado, Estado::Ok);
    assert_eq!(r.n2.as_ref().unwrap().estado, Estado::Ok);
    assert_eq!(r.n3.as_ref().unwrap().estado, Estado::Indisponivel, "sem senha_env e sem cliente");
}

#[test]
fn instancia_invalida_aparece_com_o_erro_em_vez_de_sumir() {
    let mut cfg = DatabaseConfig::default();
    cfg.instances.push(cli_admin::config::DbInstance {
        name: "RUIM".into(),
        kind: "mysql".into(),
        host: "h".into(),
        port: None,
        service: None,
        user: None,
        password_env: None,
        tool: None,
    });
    let s = DbState::novo(&cfg);
    assert!(s.instancias.is_empty());
    assert!(s.avisos.iter().any(|a| a.contains("RUIM") && a.contains("mysql")));
}
```

(Exige `DbState.avisos: Vec<String>` com as instâncias recusadas.)

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test db_tests` → erro de compilação (`tui::db`).

- [ ] **Passo 3: Implementação**

`src/tui/db.rs` (estado, sem desenho):

```rust
//! Estado e teclas da aba Bancos. Testes de nível 3 e a descoberta de ferramentas rodam em
//! threads de fundo (cada uma com seu runtime tokio mínimo) e são consultados por `poll()`.

use crate::config::DatabaseConfig;
use crate::modules::database::{
    self, Ferramenta, InstanciaResolvida, ResultadoTeste, client, descobrir, escolher_cliente, resolver,
    resolver_tools_dir,
};
use crossterm::event::{KeyCode, KeyEvent};
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocoDb {
    Instancias,
    Ferramentas,
}

pub struct LinhaDb {
    pub instancia: InstanciaResolvida,
    pub resultado: Option<ResultadoTeste>,
    pub testando: bool,
}

pub struct DbState {
    pub instancias: Vec<LinhaDb>,
    pub ferramentas: Vec<Ferramenta>,
    pub selecionada: usize,
    pub selecionada_ferramenta: usize,
    pub foco: FocoDb,
    pub avisos: Vec<String>,
    pub mensagem: Option<String>,
    testes: Vec<(usize, mpsc::Receiver<ResultadoTeste>)>,
    busca: Option<mpsc::Receiver<Vec<Ferramenta>>>,
}

/// Executa um futuro numa thread própria, com runtime mínimo, e devolve o canal do resultado.
fn bloqueante<T: Send + 'static>(
    fut: impl std::future::Future<Output = T> + Send + 'static,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
            let _ = tx.send(rt.block_on(fut));
        }
    });
    rx
}

impl DbState {
    pub fn novo(cfg: &DatabaseConfig) -> Self {
        let mut s = DbState {
            instancias: Vec::new(),
            ferramentas: Vec::new(),
            selecionada: 0,
            selecionada_ferramenta: 0,
            foco: FocoDb::Instancias,
            avisos: Vec::new(),
            mensagem: None,
            testes: Vec::new(),
            busca: None,
        };
        for i in &cfg.instances {
            match resolver(i) {
                Ok(r) => s.instancias.push(LinhaDb { instancia: r, resultado: None, testando: false }),
                Err(e) => s.avisos.push(format!("{}: {e}", i.name)),
            }
        }
        s.reescanear(cfg);
        s
    }

    /// Varre a pasta de ferramentas em segundo plano.
    pub fn reescanear(&mut self, cfg: &DatabaseConfig) {
        let raiz = resolver_tools_dir(&cfg.tools_dir);
        let extras = cfg.tools.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(descobrir(&raiz, &extras));
        });
        self.busca = Some(rx);
    }

    pub fn poll(&mut self) {
        if let Some(rx) = &self.busca {
            if let Ok(f) = rx.try_recv() {
                self.ferramentas = f;
                self.busca = None;
            }
        }
        let mut prontos = Vec::new();
        self.testes.retain(|(idx, rx)| match rx.try_recv() {
            Ok(r) => {
                prontos.push((*idx, r));
                false
            }
            Err(mpsc::TryRecvError::Empty) => true,
            Err(mpsc::TryRecvError::Disconnected) => {
                prontos.push((*idx, ResultadoTeste::default()));
                false
            }
        });
        for (idx, r) in prontos {
            if let Some(l) = self.instancias.get_mut(idx) {
                l.resultado = Some(r);
                l.testando = false;
            }
        }
    }

    fn testar(&mut self, idx: usize, cfg: &DatabaseConfig) {
        let Some(l) = self.instancias.get_mut(idx) else { return };
        if l.testando {
            return;
        }
        l.testando = true;
        l.resultado = None;
        let (inst, ferr, t) = (l.instancia.clone(), self.ferramentas.clone(), cfg.timeout_ms);
        let rx = bloqueante(async move { database::testar(&inst, &ferr, t, 3).await });
        self.testes.push((idx, rx));
    }

    pub fn tecla(&mut self, key: KeyEvent, cfg: &DatabaseConfig) {
        match key.code {
            KeyCode::Tab => {
                self.foco = if self.foco == FocoDb::Instancias { FocoDb::Ferramentas } else { FocoDb::Instancias };
            }
            KeyCode::Up | KeyCode::Char('k') => self.mover(-1),
            KeyCode::Down | KeyCode::Char('j') => self.mover(1),
            KeyCode::Char('t') => {
                let i = self.selecionada;
                self.testar(i, cfg);
            }
            KeyCode::Char('T') => {
                for i in 0..self.instancias.len() {
                    self.testar(i, cfg);
                }
            }
            KeyCode::Char('r') => {
                self.reescanear(cfg);
                self.mensagem = Some("Reescaneando a pasta de ferramentas...".into());
            }
            KeyCode::Enter => self.abrir_console(),
            KeyCode::Char('g') => self.abrir_grafica(),
            _ => {}
        }
    }

    fn mover(&mut self, delta: isize) {
        let (alvo, tam) = match self.foco {
            FocoDb::Instancias => (&mut self.selecionada, self.instancias.len()),
            FocoDb::Ferramentas => (&mut self.selecionada_ferramenta, self.ferramentas.len()),
        };
        if tam > 0 {
            *alvo = (*alvo as isize + delta).clamp(0, tam as isize - 1) as usize;
        }
    }

    fn abrir_console(&mut self) {
        let Some(l) = self.instancias.get(self.selecionada) else { return };
        match escolher_cliente(&l.instancia, &self.ferramentas) {
            None => self.mensagem = Some(format!("Nenhum cliente de {} na pasta de ferramentas.", l.instancia.kind.rotulo())),
            Some(c) => {
                self.mensagem = Some(match client::abrir_console(&l.instancia, c) {
                    Ok(()) => format!("{} aberto em nova janela. Digite a senha lá.", c.rotulo),
                    Err(e) => e,
                })
            }
        }
    }

    fn abrir_grafica(&mut self) {
        let Some(f) = self.ferramentas.get(self.selecionada_ferramenta).filter(|f| f.gui) else {
            self.mensagem = Some("Escolha uma ferramenta gráfica no painel de ferramentas ([Tab]).".into());
            return;
        };
        self.mensagem = Some(match client::abrir_grafica(f) {
            Ok(()) => format!("{} iniciado.", f.rotulo),
            Err(e) => e,
        });
    }
}
```

`src/tui/db_ui.rs` (desenho): `draw_db_tab(f, area, db: &mut DbState)` com três regiões (instâncias: tabela `NOME | TIPO | DESTINO | L1 | L2 | L3`, onde cada nível mostra `●` verde (OK), `✖` vermelho (falha), `○` cinza (indisponível), `…` enquanto testa; detalhe do último teste da instância selecionada com `detalhe` de cada etapa e ms; ferramentas: tabela `FERRAMENTA | TIPO | VERSÃO | MODO | CAMINHO`), mais a mensagem e os avisos de instâncias recusadas. Segue o mesmo padrão de `fm_ui.rs`: funções `bloco`, `Table`, `Paragraph`. O texto dos três indicadores sai de uma função pura `pub fn simbolo(e: Option<&Etapa>, testando: bool) -> (&'static str, Color)`, coberta por teste unitário (OK → `●`, Falha → `✖`, Indisponível → `○`, `None` e testando → `…`, `None` parado → `·`).

Ligação:
- `src/tui/mod.rs`: `pub mod db; pub mod db_ui;`.
- `app.rs`: `ActiveTab::Databases = 5`, `Support = 6` (ajustar `next`/`prev`); campo `pub db: super::db::DbState` (`DbState::novo(&config.database)`); teclas globais `'6'` → Bancos e `'7'` → Suporte; despacho `ActiveTab::Databases => self.db.tecla(key, &self.config.database)`; `poll_background` chama `self.db.poll()`; `select_next`/`select_prev` ganham braço vazio para `Databases`.
- `ui.rs`: título `" [6] 🗄️ Bancos "` e Suporte `" [7] 🧰 Suporte "`; `ActiveTab::Databases => super::db_ui::draw_db_tab(f, chunks[1], &mut app.db)`; rodapé `"[t] Testar | [T] Testar todas | [Enter] Abrir cliente | [g] Ferramenta gráfica | [Tab] Foco | [r] Reescanear | [?] Ajuda | [q] Sair"`; bloco "Aba 6: Bancos" na ajuda.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline` → verde. Atualizar `fm_ui_tests.rs` e `v3_render_...`: Suporte passa de `[6]` para `[7]`.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): aba Bancos com teste em segundo plano`.

---

### Tarefa 10: Subcomando `db`

**Arquivos:** modificar `src/main.rs`.

**Interfaces:**
- Produz: `db list`, `db tools`, `db test <nome> [--nivel 1|2|3]` com código de saída `0` (nenhuma etapa executada falhou) e `1` (falha ou erro de uso).

- [ ] **Passo 1: Teste que falha** (acrescentar a `tests/db_tests.rs`, executando o binário compilado)

```rust
fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cli_admin_v3"))
}

fn config_temporaria(porta: u16, nome: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cli_admin_dbcli_{nome}_{}.toml", std::process::id()));
    std::fs::write(
        &p,
        format!(
            r#"
[database]
tools_dir = "pasta_que_nao_existe"
timeout_ms = 1500
[[database.instances]]
name = "PG"
kind = "postgres"
host = "127.0.0.1"
port = {porta}
service = "db"
user = "u"
"#
        ),
    )
    .unwrap();
    p
}

#[tokio::test]
async fn db_test_nivel_2_sai_com_zero_quando_ok_e_um_quando_falha() {
    let porta = servidor_falso(Some(b'N')).await;
    let cfg = config_temporaria(porta, "ok");
    let saida = std::process::Command::new(bin())
        .args(["--config"]).arg(&cfg)
        .args(["db", "test", "PG", "--nivel", "2"])
        .output().unwrap();
    assert!(saida.status.success(), "{}", String::from_utf8_lossy(&saida.stdout));
    let texto = String::from_utf8_lossy(&saida.stdout);
    assert!(texto.contains("N1") && texto.contains("N2"), "{texto}");

    let livre = {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        l.local_addr().unwrap().port()
    };
    let cfg2 = config_temporaria(livre, "falha");
    let saida = std::process::Command::new(bin())
        .args(["--config"]).arg(&cfg2)
        .args(["db", "test", "PG", "--nivel", "1"])
        .output().unwrap();
    assert_eq!(saida.status.code(), Some(1));
}

#[test]
fn db_test_instancia_inexistente_sai_com_um() {
    let cfg = config_temporaria(1, "inexistente");
    let saida = std::process::Command::new(bin())
        .args(["--config"]).arg(&cfg)
        .args(["db", "test", "NAO_EXISTE"])
        .output().unwrap();
    assert_eq!(saida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("NAO_EXISTE"));
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test db_tests db_test` → o subcomando `db` não existe (clap devolve erro de uso, código 2).

- [ ] **Passo 3: Implementação** (`src/main.rs`)

Variante em `Commands`: `/// Bancos de dados: instâncias, ferramentas e teste de conexão` / `Db(DbArgs)`. Tipos:

```rust
#[derive(Args, Debug)]
struct DbArgs {
    #[command(subcommand)]
    action: DbAction,
}

#[derive(Subcommand, Debug)]
enum DbAction {
    /// Lista as instâncias configuradas
    List,
    /// Lista as ferramentas de cliente encontradas na pasta de ferramentas
    Tools,
    /// Testa a conexão de uma instância (nível 1 TCP, 2 protocolo, 3 login pelo cliente)
    Test {
        nome: String,
        #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=3))]
        nivel: u8,
    },
}
```

Despacho: `Commands::Db(args) => handle_db_cli(config, args).await?,` e o tratador:

```rust
async fn handle_db_cli(config: AppConfig, args: DbArgs) -> Result<(), Box<dyn std::error::Error>> {
    use cli_admin::modules::database::{self, Estado, Etapa};
    let tools_dir = database::resolver_tools_dir(&config.database.tools_dir);
    match args.action {
        DbAction::List => {
            println!("{:<18} {:<11} {:<28} {}", "NOME", "TIPO", "DESTINO", "USUÁRIO");
            for i in &config.database.instances {
                match database::resolver(i) {
                    Ok(r) => println!(
                        "{:<18} {:<11} {:<28} {}",
                        r.nome,
                        r.kind.rotulo(),
                        format!("{}:{}", r.host, r.porta),
                        r.usuario.as_deref().unwrap_or("—")
                    ),
                    Err(e) => eprintln!("{}: {e}", i.name),
                }
            }
        }
        DbAction::Tools => {
            let f = database::descobrir(&tools_dir, &config.database.tools);
            if f.is_empty() {
                println!("Nenhuma ferramenta em {}. Veja tools/README.md.", tools_dir.display());
            }
            for x in f {
                println!(
                    "{:<14} {:<11} {:<8} {:<8} {}",
                    x.rotulo, x.kind.rotulo(), x.versao, if x.gui { "gráfica" } else { "console" }, x.caminho.display()
                );
            }
        }
        DbAction::Test { nome, nivel } => {
            let Some(inst) = config.database.instances.iter().find(|i| i.name.eq_ignore_ascii_case(&nome)) else {
                eprintln!("instância '{nome}' não encontrada em [[database.instances]]");
                std::process::exit(1);
            };
            let r = database::resolver(inst)?;
            let ferr = database::descobrir(&tools_dir, &config.database.tools);
            let res = database::testar(&r, &ferr, config.database.timeout_ms, nivel).await;
            let mostrar = |rotulo: &str, e: &Option<Etapa>| {
                if let Some(e) = e {
                    let marca = match e.estado {
                        Estado::Ok => "OK   ",
                        Estado::Falha => "FALHA",
                        Estado::Indisponivel => "N/D  ",
                    };
                    println!("[{marca}] {rotulo} ({} ms) {}", e.ms, e.detalhe);
                }
            };
            mostrar("N1 TCP", &res.n1);
            mostrar("N2 Protocolo", &res.n2);
            mostrar("N3 Login", &res.n3);
            if !res.sem_falhas() {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
```

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline` → verde.
- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): subcomando db (list, tools, test)`.

---

### Tarefa 11: Pasta de ferramentas, exemplo e documentação

**Arquivos:** criar `tools/README.md`; modificar `.gitignore` (criar se não existir), `config.toml`, `README.md`, `AGENTS.md`, `docs/V3_MUDANCAS.md`.

- [ ] **Passo 1:** `tools/README.md` (pt-BR), explicando: a pasta é **preenchida por você**; a ferramenta nunca baixa nem distribui binários; estrutura `db_clients/<fabricante>/<versão>/...`; nomes de arquivo reconhecidos (tabela do spec §6); como cadastrar uma ferramenta extra (`[[database.tools]]`, com o exemplo do SPDB); aviso de licença (Instant Client e SQL\*Plus: conferir os termos da Oracle antes de redistribuir; pgAdmin: licença PostgreSQL); lembrar que o Instant Client precisa das DLLs ao lado do executável.
- [ ] **Passo 2:** `.gitignore` do projeto: acrescentar

```
tools/db_clients/**
!tools/README.md
audit/
```

- [ ] **Passo 3:** `config.toml` de exemplo: acrescentar a seção `[database]` **comentada**, com uma instância de cada tipo e `password_env`; sem nenhuma senha.
- [ ] **Passo 4:** `README.md`, `AGENTS.md`, `docs/V3_MUDANCAS.md`: aba [6], comando `db`, numeração final das abas (Arquivos 5, Bancos 6, Suporte 7).
- [ ] **Passo 5: Verificação final**

Executar: `cargo build --offline` (sem avisos) e `cargo test --offline`.
Esperado: verde, em segundos.

- [ ] **Passo 6: Verificação manual** (registrar o resultado no `docs/V3_MUDANCAS.md`)

1. Colocar `psql.exe` (de uma instalação do PostgreSQL) em `tools\db_clients\postgres\16\bin\` e definir `setx PG_PWD "..."` numa instância de teste; `cli_admin_v3 db tools` deve listá-lo.
2. `cli_admin_v3 db test LOCAL_PG`: níveis 1, 2 e 3 verdes, com a versão do PostgreSQL.
3. Errar a senha de propósito: nível 3 em FALHA com `FATAL`, **sem a senha na saída**.
4. Na TUI, aba `6`: `t` não congela; `Enter` abre o `psql` em nova janela; `g` abre uma ferramenta gráfica da lista.
5. Com Oracle (se houver): confirmar que o `sqlplus` aceita a senha pela entrada padrão e se `tnsping` existe no pacote usado (a presença do `tnsping` no Instant Client **não está confirmada**).

---

## Auto-revisão

1. **Cobertura do spec:** configuração e senha só por ambiente (T1, T5); modelo e níveis (T2, T4, T7); descoberta e versão (T3); login sem senha em argumento, tempo limite, truncamento e máscara (T5, T6); abrir console e gráfico (T8); aba, teclas e segundo plano (T9); CLI com códigos de saída (T10); pasta de ferramentas, `.gitignore` e licenças (T11); ferramenta extra / SPDB por configuração (T3, T11). Fora do plano por decisão do spec: nível 2 de Oracle e SQL Server, SQL digitado, drivers nativos.
2. **Placeholders:** `db_ui.rs` está descrito (colunas, símbolos e a função pura `simbolo`) em vez de listado linha a linha, porque segue o padrão já mostrado em `fm_ui.rs`; o passo exige o teste unitário de `simbolo` antes do desenho. Nenhum "TODO".
3. **Consistência de tipos:** `Etapa`, `Estado`, `ResultadoTeste`, `InstanciaResolvida`, `Ferramenta`, `ComandoExterno` e as assinaturas de `testar`, `escolher_cliente`, `montar`, `executar` coincidem entre T2–T10.
4. **Validação do código (2026-10-17):** o código das **Tarefas 2 a 8** (modelo, descoberta, níveis 1 e 2, comando do nível 3, execução com tempo limite, orquestração e abertura do cliente) foi extraído do plano para um projeto descartável e testado: **39 testes passam (31 unitários e 8 de integração), sem avisos**. A validação encontrou e corrigiu **quatro defeitos do plano original**:
   - um `PATH` com entrada vazia na frente (`;C:\...`) faz o `cmd.exe` não achar mais nenhum programa; acontecia sempre que o executável vinha sem pasta;
   - testes que usam `cmd.exe` como cliente falso quebravam com o *AutoRun* do usuário (`doskey`); a correção é `cmd /D`;
   - o teste de tempo limite com `cmd /C ping` deixava o `ping` neto vivo por ~30 s, levando a suíte de ~1 s a ~30 s; a correção é executar `ping.exe` direto;
   - o servidor falso aceitava uma única conexão, mas os níveis 1 e 2 conectam uma vez cada.

   **NÃO foram compiladas:** a Tarefa 1 (configuração, depende do `AppConfig` real), a Tarefa 9 (estado e tela da TUI) e a Tarefa 10 (subcomando `db`). Pontos de atenção: `DbKind as u8` na ordenação (T3), `creation_flags` inerente no `tokio::process::Command` (T6) e via `CommandExt` no `std::process::Command` (T8), e `tokio::runtime::Builder` na thread de fundo (T9).
