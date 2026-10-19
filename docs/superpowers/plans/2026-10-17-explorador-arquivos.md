# Explorador de arquivos na TUI — Plano de implementação

> **Para agentes:** SUB-SKILL OBRIGATÓRIA: usar `superpowers:subagent-driven-development` (recomendado) ou `superpowers:executing-plans` para executar este plano tarefa por tarefa. Os passos usam a sintaxe de caixa de seleção (`- [ ]`).

**Objetivo:** aba **[5] Arquivos** na TUI do `cli_admin_v3`: navegar, copiar arquivos e pastas e criar pastas, sem nunca travar a interface.

**Arquitetura:** lógica pura em `src/modules/fm/` (nomes, listagem, unidades, plano e execução de cópia); estado e teclas em `src/tui/fm.rs`; desenho em `src/tui/fm_ui.rs`; `app.rs` e `ui.rs` recebem só a ligação. Toda E/S de disco roda em threads de fundo, consultadas a cada tick de 150 ms (`App::poll_background`).

**Tecnologias:** Rust (edição 2024), ratatui 0.29, crossterm 0.28, `walkdir`, `chrono`, crate `windows` 0.58 (todos já são dependências). **Nenhuma dependência nova.**

**Spec:** `docs/superpowers/specs/2026-10-17-explorador-arquivos-design.md`

## Restrições globais

- Compilar e testar sempre com `cargo build --offline` e `cargo test --offline` (diretório do projeto: `D:\W\DEV\GENERAL_TOOLS\CLI_ADMIN_V3`).
- Nome do pacote e da biblioteca: `cli_admin` (imports `cli_admin::...`); executável: `cli_admin_v3`.
- Comentários, mensagens e documentação em **pt-BR**, com acentos.
- **Nada de E/S de disco na thread da interface** (regra do projeto; ver `ferramentas_CLI/licoes/06-integracao-windows.md`, L-WIN-11 e L-WIN-12).
- **Cópia nunca sobrescreve.** Conflito vira `nome (2).ext`.
- Testes não podem escrever em `audit/` do projeto: `FmState` recebe o caminho de auditoria (`None` nos testes).
- **Commits só se o usuário pedir.** O repositório `D:\W\DEV` está na branch `codex/nx-cadenas-name-editor` com muitas alterações não relacionadas; se for commitar, use `git add` apenas dos caminhos de `GENERAL_TOOLS/CLI_ADMIN_V3`. Os passos "Commit" abaixo existem para quando isso for autorizado.
- Cada tarefa termina com `cargo build --offline` sem avisos e `cargo test --offline` verde.

## Mapa de arquivos

| Arquivo | Ação | Responsabilidade |
|---|---|---|
| `src/audit.rs` | criar | Trilha de auditoria (5 campos separados por TAB) |
| `src/lib.rs` | modificar | `pub mod audit;` |
| `src/modules/fm/mod.rs` | criar | Reexporta a API |
| `src/modules/fm/names.rs` | criar | `nome_valido`, `nome_livre` |
| `src/modules/fm/listing.rs` | criar | `FmEntry`, `FmSort`, `listar`, `ordenar`, `visiveis` |
| `src/modules/fm/drives.rs` | criar | `unidades()` |
| `src/modules/fm/copy.rs` | criar | `planejar`, `executar`, `Progresso` |
| `src/modules/mod.rs` | modificar | `pub mod fm;` |
| `src/tui/fm.rs` | criar | `FmState`: estado, teclas, threads |
| `src/tui/fm_ui.rs` | criar | `draw_files_tab` e modais |
| `src/tui/mod.rs` | modificar | `pub mod fm; pub mod fm_ui;` |
| `src/tui/app.rs` | modificar | `ActiveTab::Files`, campo `fm`, teclas, `poll_background` |
| `src/tui/ui.rs` | modificar | aba no cabeçalho, rodapé, ajuda; `centered_rect` passa a `pub(super)` |
| `tests/fm_state_tests.rs` | criar | Testes do estado com o `App`/`FmState` reais |
| `tests/fm_ui_tests.rs` | criar | Renderização em terminal virtual |

---

### Tarefa 1: Auditoria compartilhada

**Arquivos:**
- Criar: `src/audit.rs`
- Modificar: `src/lib.rs`

**Interfaces:**
- Produz: `pub const ARQUIVO_PADRAO: &str`; `pub fn formatar(quando, usuario, acao, resultado, msg: &str) -> String`; `pub fn registrar(arquivo: &Path, acao: &str, resultado: &str, msg: &str) -> std::io::Result<()>`.

- [ ] **Passo 1: Escrever o teste que falha** (no fim de `src/audit.rs`, junto da implementação do passo 3; escreva primeiro só o módulo de teste)

```rust
#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn linha_tem_cinco_campos_e_sem_quebras() {
        let l = formatar("2026-10-17 10:00:00", "andre", "fm.copiar", "ok", "a\tb\nc");
        assert_eq!(l.split('\t').count(), 5);
        assert!(!l.contains('\n'));
        assert!(l.ends_with("a b c"));
    }

    #[test]
    fn registrar_acrescenta_linhas_e_cria_pastas() {
        let dir = std::env::temp_dir().join(format!("cli_admin_audit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let arq = dir.join("sub").join("a.log");
        registrar(&arq, "x", "ok", "1").unwrap();
        registrar(&arq, "x", "ok", "2").unwrap();
        assert_eq!(std::fs::read_to_string(&arq).unwrap().lines().count(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar**

Executar: `cargo test --offline --lib audit`
Esperado: erro de compilação (`formatar` e `registrar` não existem).

- [ ] **Passo 3: Implementação mínima** (início de `src/audit.rs`)

```rust
//! Trilha de auditoria: uma linha por evento, 5 campos separados por TAB:
//! quando, usuário, ação, resultado, mensagem. Quebras de linha e tabs viram espaço.
//! Quem chama é responsável por **não** passar segredos na mensagem.

use std::io::Write;
use std::path::Path;

pub const ARQUIVO_PADRAO: &str = "audit/auditoria.log";

fn limpar(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ")
}

pub fn formatar(quando: &str, usuario: &str, acao: &str, resultado: &str, msg: &str) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}",
        limpar(quando),
        limpar(usuario),
        limpar(acao),
        limpar(resultado),
        limpar(msg)
    )
}

/// Acrescenta uma linha ao arquivo, criando as pastas que faltarem.
pub fn registrar(arquivo: &Path, acao: &str, resultado: &str, msg: &str) -> std::io::Result<()> {
    if let Some(pai) = arquivo.parent() {
        std::fs::create_dir_all(pai)?;
    }
    let quando = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let usuario = std::env::var("USERNAME").unwrap_or_else(|_| "desconhecido".into());
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(arquivo)?;
    writeln!(f, "{}", formatar(&quando, &usuario, acao, resultado, msg))
}
```

Em `src/lib.rs`, acrescentar `pub mod audit;` (antes de `pub mod config;`).

- [ ] **Passo 4: Rodar e ver passar**

Executar: `cargo test --offline --lib audit`
Esperado: 2 testes passam.

- [ ] **Passo 5: Commit (se autorizado)**

```bash
git add GENERAL_TOOLS/CLI_ADMIN_V3/src/audit.rs GENERAL_TOOLS/CLI_ADMIN_V3/src/lib.rs
git commit -m "feat(cli_admin_v3): trilha de auditoria compartilhada"
```

---

### Tarefa 2: Nomes de arquivo e pasta

**Arquivos:**
- Criar: `src/modules/fm/mod.rs`, `src/modules/fm/names.rs`
- Modificar: `src/modules/mod.rs` (`pub mod fm;`)

**Interfaces:**
- Produz: `pub fn nome_valido(nome: &str) -> Result<(), String>`; `pub fn nome_livre(existe: impl Fn(&str) -> bool, nome: &str) -> String`.

- [ ] **Passo 1: Escrever os testes que falham** (`src/modules/fm/names.rs`)

```rust
#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nomes_validos() {
        assert!(nome_valido("Relatórios 2026").is_ok());
        assert!(nome_valido("a.b.c").is_ok());
        assert!(nome_valido(&"x".repeat(255)).is_ok());
    }

    #[test]
    fn nomes_invalidos() {
        for ruim in ["", "a:b", "a/b", "a\\b", "a|b", "a*b", "a?b", "a\"b", "pasta.", "pasta ", "..", "x\u{7}y"] {
            assert!(nome_valido(ruim).is_err(), "deveria recusar {ruim:?}");
        }
        assert!(nome_valido(&"x".repeat(256)).is_err());
    }

    #[test]
    fn nomes_reservados_do_windows() {
        for r in ["CON", "con", "NUL", "COM1", "lpt9", "con.txt", "Aux.log"] {
            assert!(nome_valido(r).is_err(), "deveria recusar {r:?}");
        }
        assert!(nome_valido("CONTA").is_ok());
        assert!(nome_valido("COM10").is_ok());
    }

    #[test]
    fn nome_livre_sem_conflito_nao_muda() {
        assert_eq!(nome_livre(|_| false, "a.txt"), "a.txt");
    }

    #[test]
    fn nome_livre_numera_antes_da_extensao() {
        let ocupados = ["a.txt", "a (2).txt"];
        assert_eq!(nome_livre(|n| ocupados.contains(&n), "a.txt"), "a (3).txt");
    }

    #[test]
    fn nome_livre_sem_extensao_e_com_ponto_inicial() {
        assert_eq!(nome_livre(|n| n == "pasta", "pasta"), "pasta (2)");
        assert_eq!(nome_livre(|n| n == ".env", ".env"), ".env (2)");
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar**

Executar: `cargo test --offline --lib names`
Esperado: erro de compilação (funções não existem).

- [ ] **Passo 3: Implementação mínima** (início de `src/modules/fm/names.rs`)

```rust
//! Regras de nome do Windows para criar pastas e para nunca sobrescrever na cópia.

const RESERVADOS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Valida o nome de **um** item (sem separadores de caminho).
pub fn nome_valido(nome: &str) -> Result<(), String> {
    if nome.is_empty() {
        return Err("o nome está vazio".into());
    }
    if nome.chars().count() > 255 {
        return Err("nome longo demais (máximo 255 caracteres)".into());
    }
    if nome.ends_with(' ') || nome.ends_with('.') {
        return Err("o nome não pode terminar com espaço ou ponto".into());
    }
    if let Some(c) = nome.chars().find(|c| c.is_control() || "<>:\"/\\|?*".contains(*c)) {
        return Err(format!("caractere inválido: {c:?}"));
    }
    let base = nome.split('.').next().unwrap_or("").to_uppercase();
    if RESERVADOS.contains(&base.as_str()) {
        return Err(format!("nome reservado do Windows: {base}"));
    }
    Ok(())
}

/// Primeiro nome livre: `nome`, depois `nome (2).ext`, `nome (3).ext`...
/// `existe` diz se um nome já está ocupado (o Windows não diferencia maiúsculas).
pub fn nome_livre(existe: impl Fn(&str) -> bool, nome: &str) -> String {
    if !existe(nome) {
        return nome.to_string();
    }
    let (base, ext) = match nome.rfind('.') {
        Some(i) if i > 0 => (&nome[..i], &nome[i..]),
        _ => (nome, ""),
    };
    (2..)
        .map(|n| format!("{base} ({n}){ext}"))
        .find(|candidato| !existe(candidato))
        .expect("sempre existe um número livre")
}
```

`src/modules/fm/mod.rs`:

```rust
pub mod names;

pub use names::{nome_livre, nome_valido};
```

Em `src/modules/mod.rs`, acrescentar `pub mod fm;`.

- [ ] **Passo 4: Rodar e ver passar**

Executar: `cargo test --offline --lib names`
Esperado: 6 testes passam.

- [ ] **Passo 5: Commit (se autorizado)** — `git add` de `src/modules/fm/` e `src/modules/mod.rs`; mensagem `feat(cli_admin_v3): validação e numeração de nomes do explorador`.

---

### Tarefa 3: Listagem, ordenação e filtro

**Arquivos:**
- Criar: `src/modules/fm/listing.rs`
- Modificar: `src/modules/fm/mod.rs`

**Interfaces:**
- Consome: nada.
- Produz: `FmEntry { nome, caminho, eh_pasta, eh_link, bytes, modificado: Option<SystemTime>, oculto }`; `FmSort { Nome, Tamanho, Data }` com `proximo()` e `rotulo()`; `listar(&Path) -> io::Result<Vec<FmEntry>>`; `ordenar(&mut [FmEntry], FmSort)`; `visiveis(&[FmEntry], mostrar_ocultos: bool, filtro: &str) -> Vec<FmEntry>`; `pub(super) fn atributos(&Metadata) -> u32`; `pub const ATTR_REPARSE: u32`.

- [ ] **Passo 1: Escrever os testes que falham** (`src/modules/fm/listing.rs`)

```rust
#[cfg(test)]
mod testes {
    use super::*;

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cli_admin_fm_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("zeta")).unwrap();
        std::fs::write(d.join("b.txt"), vec![0u8; 10]).unwrap();
        std::fs::write(d.join("A.txt"), vec![0u8; 100]).unwrap();
        d
    }

    #[test]
    fn lista_e_marca_pastas_e_tamanhos() {
        let d = pasta_de_teste("lista");
        let mut v = listar(&d).unwrap();
        ordenar(&mut v, FmSort::Nome);
        let nomes: Vec<&str> = v.iter().map(|e| e.nome.as_str()).collect();
        assert_eq!(nomes, ["zeta", "A.txt", "b.txt"], "pastas primeiro, depois por nome sem diferenciar caixa");
        assert!(v[0].eh_pasta && !v[1].eh_pasta);
        assert_eq!(v[1].bytes, 100);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn ordena_por_tamanho_decrescente() {
        let d = pasta_de_teste("tam");
        let mut v = listar(&d).unwrap();
        ordenar(&mut v, FmSort::Tamanho);
        let nomes: Vec<&str> = v.iter().map(|e| e.nome.as_str()).collect();
        assert_eq!(nomes, ["zeta", "A.txt", "b.txt"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn filtro_por_texto_sem_diferenciar_caixa() {
        let d = pasta_de_teste("filtro");
        let v = listar(&d).unwrap();
        let r = visiveis(&v, true, "B.T");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].nome, "b.txt");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn oculto_some_sem_mostrar_ocultos() {
        let d = pasta_de_teste("oculto");
        let alvo = d.join("b.txt");
        let ok = std::process::Command::new("attrib").arg("+h").arg(&alvo).status().unwrap().success();
        assert!(ok, "attrib +h falhou");
        let v = listar(&d).unwrap();
        assert!(visiveis(&v, false, "").iter().all(|e| e.nome != "b.txt"));
        assert!(visiveis(&v, true, "").iter().any(|e| e.nome == "b.txt"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn pasta_inexistente_devolve_erro() {
        assert!(listar(Path::new(r"Z:\nao\existe\mesmo")).is_err());
    }

    #[test]
    fn ciclo_de_ordenacao() {
        assert_eq!(FmSort::Nome.proximo(), FmSort::Tamanho);
        assert_eq!(FmSort::Tamanho.proximo(), FmSort::Data);
        assert_eq!(FmSort::Data.proximo(), FmSort::Nome);
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib listing` → erro de compilação.

- [ ] **Passo 3: Implementação mínima** (início de `src/modules/fm/listing.rs`)

```rust
//! Leitura de uma pasta para o explorador. Itens ilegíveis são pulados, nunca derrubam a lista.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

const ATTR_OCULTO: u32 = 0x2;
const ATTR_SISTEMA: u32 = 0x4;
/// Link simbólico ou junção. Para a **cópia**, nunca é seguido.
pub const ATTR_REPARSE: u32 = 0x400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FmEntry {
    pub nome: String,
    pub caminho: PathBuf,
    /// Pode ser aberta na navegação (inclui link para pasta).
    pub eh_pasta: bool,
    /// Link simbólico ou junção: aparece na lista, mas a cópia o ignora.
    pub eh_link: bool,
    pub bytes: u64,
    pub modificado: Option<SystemTime>,
    pub oculto: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmSort {
    Nome,
    Tamanho,
    Data,
}

impl FmSort {
    pub fn proximo(self) -> Self {
        match self {
            FmSort::Nome => FmSort::Tamanho,
            FmSort::Tamanho => FmSort::Data,
            FmSort::Data => FmSort::Nome,
        }
    }

    pub fn rotulo(self) -> &'static str {
        match self {
            FmSort::Nome => "nome",
            FmSort::Tamanho => "tamanho",
            FmSort::Data => "data",
        }
    }
}

#[cfg(windows)]
pub(super) fn atributos(md: &std::fs::Metadata) -> u32 {
    use std::os::windows::fs::MetadataExt;
    md.file_attributes()
}

#[cfg(not(windows))]
pub(super) fn atributos(_md: &std::fs::Metadata) -> u32 {
    0
}

/// Lê `dir`. Pode demorar em unidade de rede: **chame de uma thread de fundo**.
pub fn listar(dir: &Path) -> std::io::Result<Vec<FmEntry>> {
    let mut saida = Vec::new();
    for ent in std::fs::read_dir(dir)? {
        let Ok(ent) = ent else { continue };
        // No Windows, DirEntry::metadata não segue links
        let Ok(md) = ent.metadata() else { continue };
        let attr = atributos(&md);
        let nome = ent.file_name().to_string_lossy().to_string();
        let caminho = ent.path();
        let eh_link = md.file_type().is_symlink() || attr & ATTR_REPARSE != 0;
        let eh_pasta = if eh_link {
            std::fs::metadata(&caminho).map(|m| m.is_dir()).unwrap_or(false)
        } else {
            md.is_dir()
        };
        let oculto = attr & (ATTR_OCULTO | ATTR_SISTEMA) != 0
            || (cfg!(not(windows)) && nome.starts_with('.'));
        saida.push(FmEntry {
            nome,
            caminho,
            eh_pasta,
            eh_link,
            bytes: if md.is_file() { md.len() } else { 0 },
            modificado: md.modified().ok(),
            oculto,
        });
    }
    Ok(saida)
}

/// Pastas primeiro; depois pela ordem pedida (tamanho e data em ordem decrescente).
pub fn ordenar(v: &mut [FmEntry], ordem: FmSort) {
    v.sort_by(|a, b| {
        let por_nome = || a.nome.to_lowercase().cmp(&b.nome.to_lowercase());
        b.eh_pasta.cmp(&a.eh_pasta).then_with(|| match ordem {
            FmSort::Nome => por_nome(),
            FmSort::Tamanho => b.bytes.cmp(&a.bytes).then_with(por_nome),
            FmSort::Data => b.modificado.cmp(&a.modificado).then_with(por_nome),
        })
    });
}

/// Itens que aparecem na tela: respeita ocultos e o filtro por texto.
pub fn visiveis(todas: &[FmEntry], mostrar_ocultos: bool, filtro: &str) -> Vec<FmEntry> {
    let f = filtro.to_lowercase();
    todas
        .iter()
        .filter(|e| (mostrar_ocultos || !e.oculto) && (f.is_empty() || e.nome.to_lowercase().contains(&f)))
        .cloned()
        .collect()
}
```

`src/modules/fm/mod.rs` passa a ser:

```rust
pub mod listing;
pub mod names;

pub use listing::{FmEntry, FmSort, listar, ordenar, visiveis};
pub use names::{nome_livre, nome_valido};
```

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib listing` → 6 testes passam (5 fora do Windows).

- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): listagem, ordenação e filtro do explorador`.

---

### Tarefa 4: Unidades do Windows

**Arquivos:**
- Criar: `src/modules/fm/drives.rs`
- Modificar: `src/modules/fm/mod.rs`

**Interfaces:**
- Produz: `pub fn unidades() -> Vec<String>` (ex.: `["C:\\", "D:\\"]`).

- [ ] **Passo 1: Teste que falha** (`src/modules/fm/drives.rs`)

```rust
#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn inclui_a_unidade_do_temp() {
        let temp = std::env::temp_dir().to_string_lossy().to_uppercase();
        let raiz: String = temp.chars().take(3).collect();
        assert!(
            unidades().iter().any(|u| u.to_uppercase() == raiz),
            "unidades={:?} temp={temp}",
            unidades()
        );
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib drives` → erro de compilação.

- [ ] **Passo 3: Implementação mínima**

```rust
//! Letras de unidade presentes. Usa `GetLogicalDrives` (uma máscara de bits): **não toca a
//! mídia**, então não trava com unidade de rede lenta nem com leitor vazio.

#[cfg(windows)]
pub fn unidades() -> Vec<String> {
    use windows::Win32::Storage::FileSystem::GetLogicalDrives;
    // SAFETY: a função não recebe ponteiros e só lê o estado do sistema.
    let mascara = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|i| mascara & (1 << i) != 0)
        .map(|i| format!("{}:\\", (b'A' + i as u8) as char))
        .collect()
}

#[cfg(not(windows))]
pub fn unidades() -> Vec<String> {
    vec!["/".to_string()]
}
```

`mod.rs`: acrescentar `pub mod drives;` e `pub use drives::unidades;`.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib drives` → 1 teste passa.

- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): unidades do Windows sem tocar a mídia`.

---

### Tarefa 5: Plano de cópia

**Arquivos:**
- Criar: `src/modules/fm/copy.rs`
- Modificar: `src/modules/fm/mod.rs`

**Interfaces:**
- Consome: `names::nome_livre`; `listing::{atributos, ATTR_REPARSE}` (tornar `atributos` `pub(super)` já está feito na Tarefa 3).
- Produz: `ItemCopia { origem, destino, eh_pasta, bytes }`; `PlanoCopia { itens, total_bytes, renomeados, ignorados }` (`Default`); `pub fn planejar(fontes: &[PathBuf], destino: &Path) -> Result<PlanoCopia, String>`.

- [ ] **Passo 1: Testes que falham** (`src/modules/fm/copy.rs`)

```rust
#[cfg(test)]
mod testes {
    use super::*;

    /// Cria `<temp>/cli_admin_cp_<nome>/{origem,destino}` e devolve (origem, destino).
    fn cenario(nome: &str) -> (PathBuf, PathBuf) {
        let raiz = std::env::temp_dir().join(format!("cli_admin_cp_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        std::fs::create_dir_all(raiz.join("origem")).unwrap();
        std::fs::create_dir_all(raiz.join("destino")).unwrap();
        (raiz.join("origem"), raiz.join("destino"))
    }

    fn limpar(p: &Path) {
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn arquivo_simples() {
        let (o, d) = cenario("simples");
        std::fs::write(o.join("a.txt"), b"hello").unwrap();
        let p = planejar(&[o.join("a.txt")], &d).unwrap();
        assert_eq!(p.itens.len(), 1);
        assert_eq!(p.total_bytes, 5);
        assert_eq!(p.renomeados, 0);
        assert_eq!(p.itens[0].destino, d.join("a.txt"));
        limpar(&o);
    }

    #[test]
    fn conflito_vira_nome_2() {
        let (o, d) = cenario("conflito");
        std::fs::write(o.join("a.txt"), b"novo").unwrap();
        std::fs::write(d.join("a.txt"), b"antigo").unwrap();
        let p = planejar(&[o.join("a.txt")], &d).unwrap();
        assert_eq!(p.itens[0].destino, d.join("a (2).txt"));
        assert_eq!(p.renomeados, 1);
        limpar(&o);
    }

    #[test]
    fn dois_com_o_mesmo_nome_nao_colidem() {
        let (o, d) = cenario("mesmo_nome");
        std::fs::create_dir_all(o.join("x")).unwrap();
        std::fs::write(o.join("a.txt"), b"1").unwrap();
        std::fs::write(o.join("x").join("a.txt"), b"2").unwrap();
        let p = planejar(&[o.join("a.txt"), o.join("x").join("a.txt")], &d).unwrap();
        assert_ne!(p.itens[0].destino, p.itens[1].destino);
        limpar(&o);
    }

    #[test]
    fn pasta_recursiva() {
        let (o, d) = cenario("recursiva");
        std::fs::create_dir_all(o.join("p").join("sub")).unwrap();
        std::fs::write(o.join("p").join("x.txt"), b"xx").unwrap();
        std::fs::write(o.join("p").join("sub").join("y.txt"), b"yyy").unwrap();
        let p = planejar(&[o.join("p")], &d).unwrap();
        assert_eq!(p.itens.len(), 4, "p, x.txt, sub, y.txt");
        assert_eq!(p.total_bytes, 5);
        assert!(p.itens[0].eh_pasta, "a pasta raiz vem antes dos filhos");
        limpar(&o);
    }

    #[test]
    fn destino_dentro_da_origem_e_recusado() {
        let (o, _d) = cenario("dentro");
        std::fs::create_dir_all(o.join("p").join("sub")).unwrap();
        let r = planejar(&[o.join("p")], &o.join("p").join("sub"));
        assert!(r.unwrap_err().contains("dentro"));
        limpar(&o);
    }

    #[test]
    fn sem_fontes_ou_destino_invalido() {
        let (o, d) = cenario("invalidos");
        assert!(planejar(&[], &d).is_err());
        std::fs::write(o.join("f.txt"), b"x").unwrap();
        assert!(planejar(&[o.join("f.txt")], &o.join("f.txt")).is_err(), "destino é arquivo");
        assert!(planejar(&[o.join("nao_existe")], &d).is_err());
        limpar(&o);
    }

    #[cfg(windows)]
    #[test]
    fn juncao_e_ignorada_e_nao_seguida() {
        let (o, d) = cenario("juncao");
        std::fs::create_dir_all(o.join("p").join("real")).unwrap();
        std::fs::write(o.join("p").join("real").join("dado.bin"), b"12345").unwrap();
        let link = o.join("p").join("atalho");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(o.join("p").join("real"))
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "mklink /J falhou; o teste precisa criar a junção");
        let p = planejar(&[o.join("p")], &d).unwrap();
        assert_eq!(p.ignorados.len(), 1, "a junção deve ser listada como ignorada: {:?}", p.ignorados);
        assert!(p.itens.iter().all(|i| !i.origem.starts_with(&link)), "nada dentro da junção entra no plano");
        // dado.bin entra uma única vez (via 'real'), não duas
        assert_eq!(p.itens.iter().filter(|i| i.origem.ends_with("dado.bin")).count(), 1);
        limpar(&o);
    }
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib copy` → erro de compilação.

- [ ] **Passo 3: Implementação mínima** (início de `src/modules/fm/copy.rs`)

```rust
//! Plano e execução de cópia. O plano é calculado antes, mostrado ao usuário e só então executado.
//! Regras: nunca sobrescreve (conflito vira `nome (2)`), não segue links nem junções,
//! recusa copiar uma pasta para dentro dela mesma.

use super::listing::{ATTR_REPARSE, atributos};
use super::names::nome_livre;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct ItemCopia {
    pub origem: PathBuf,
    pub destino: PathBuf,
    pub eh_pasta: bool,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct PlanoCopia {
    pub itens: Vec<ItemCopia>,
    pub total_bytes: u64,
    /// Quantos itens da raiz ganharam sufixo `(2)` por conflito.
    pub renomeados: usize,
    /// Links, junções e itens ilegíveis que ficaram de fora, com o motivo.
    pub ignorados: Vec<String>,
}

fn eh_reparse(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink() || atributos(&m) & ATTR_REPARSE != 0)
        .unwrap_or(false)
}

/// Calcula o que seria copiado para `destino`. Lê a árvore de origem: pode demorar,
/// então **chame de uma thread de fundo**.
pub fn planejar(fontes: &[PathBuf], destino: &Path) -> Result<PlanoCopia, String> {
    if fontes.is_empty() {
        return Err("nada selecionado para copiar".into());
    }
    if !destino.is_dir() {
        return Err(format!("o destino não é uma pasta: {}", destino.display()));
    }
    let destino_canon =
        std::fs::canonicalize(destino).map_err(|e| format!("destino ilegível: {e}"))?;

    let mut plano = PlanoCopia::default();
    let mut usados: HashSet<String> = HashSet::new();

    for fonte in fontes {
        let md = std::fs::symlink_metadata(fonte)
            .map_err(|e| format!("origem ilegível ({}): {e}", fonte.display()))?;
        if eh_reparse(fonte) {
            plano.ignorados.push(format!("{} (link ou junção não é copiado)", fonte.display()));
            continue;
        }
        let nome = fonte
            .file_name()
            .ok_or_else(|| format!("origem sem nome: {}", fonte.display()))?
            .to_string_lossy()
            .to_string();

        if md.is_dir() {
            let fonte_canon = std::fs::canonicalize(fonte)
                .map_err(|e| format!("origem ilegível ({}): {e}", fonte.display()))?;
            if destino_canon.starts_with(&fonte_canon) {
                return Err(format!("o destino está dentro da pasta de origem: {}", fonte.display()));
            }
        }

        let livre = nome_livre(
            |n| destino.join(n).exists() || usados.contains(&n.to_lowercase()),
            &nome,
        );
        if livre != nome {
            plano.renomeados += 1;
        }
        usados.insert(livre.to_lowercase());
        let raiz_destino = destino.join(&livre);

        if !md.is_dir() {
            plano.total_bytes += md.len();
            plano.itens.push(ItemCopia {
                origem: fonte.clone(),
                destino: raiz_destino,
                eh_pasta: false,
                bytes: md.len(),
            });
            continue;
        }

        plano.itens.push(ItemCopia {
            origem: fonte.clone(),
            destino: raiz_destino.clone(),
            eh_pasta: true,
            bytes: 0,
        });
        let mut it = WalkDir::new(fonte).min_depth(1).follow_links(false).into_iter();
        while let Some(ent) = it.next() {
            let ent = match ent {
                Ok(e) => e,
                Err(e) => {
                    plano.ignorados.push(e.to_string());
                    continue;
                }
            };
            if eh_reparse(ent.path()) {
                plano.ignorados.push(format!("{} (link ou junção)", ent.path().display()));
                if ent.file_type().is_dir() {
                    it.skip_current_dir();
                }
                continue;
            }
            let Ok(rel) = ent.path().strip_prefix(fonte) else { continue };
            let alvo = raiz_destino.join(rel);
            if ent.file_type().is_dir() {
                plano.itens.push(ItemCopia {
                    origem: ent.path().to_path_buf(),
                    destino: alvo,
                    eh_pasta: true,
                    bytes: 0,
                });
            } else if ent.file_type().is_file() {
                let bytes = ent.metadata().map(|m| m.len()).unwrap_or(0);
                plano.total_bytes += bytes;
                plano.itens.push(ItemCopia {
                    origem: ent.path().to_path_buf(),
                    destino: alvo,
                    eh_pasta: false,
                    bytes,
                });
            }
        }
    }

    if plano.itens.is_empty() {
        return Err("nada que possa ser copiado (só links ou itens ilegíveis)".into());
    }
    Ok(plano)
}
```

`mod.rs`: acrescentar `pub mod copy;` e `pub use copy::{ItemCopia, PlanoCopia, planejar};`.

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib copy` → 7 testes passam (6 fora do Windows). **Atenção:** se `juncao_e_ignorada_e_nao_seguida` falhar, a detecção de junção não funciona como esperado; **pare e investigue antes de seguir** (é o risco registrado no spec, §10).

- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): plano de cópia sem sobrescrita nem links`.

---

### Tarefa 6: Execução da cópia

**Arquivos:**
- Modificar: `src/modules/fm/copy.rs`, `src/modules/fm/mod.rs`

**Interfaces:**
- Consome: `PlanoCopia`, `ItemCopia`.
- Produz: `Progresso { bytes: AtomicU64, itens: AtomicUsize, cancelar: AtomicBool }` (`Default`); `ResultadoCopia { copiados: usize, bytes: u64, erros: Vec<String>, cancelado: bool }` (`Default`); `pub fn executar(plano: &PlanoCopia, prog: &Progresso) -> ResultadoCopia`.

- [ ] **Passo 1: Testes que falham** (dentro do `mod testes` existente em `copy.rs`)

```rust
    #[test]
    fn executar_copia_o_conteudo_e_conta() {
        let (o, d) = cenario("exec");
        std::fs::create_dir_all(o.join("p").join("sub")).unwrap();
        std::fs::write(o.join("p").join("x.txt"), b"xx").unwrap();
        std::fs::write(o.join("p").join("sub").join("y.txt"), b"yyy").unwrap();
        let plano = planejar(&[o.join("p")], &d).unwrap();
        let prog = Progresso::default();
        let r = executar(&plano, &prog);
        assert!(r.erros.is_empty(), "{:?}", r.erros);
        assert!(!r.cancelado);
        assert_eq!(r.copiados, 4);
        assert_eq!(r.bytes, 5);
        assert_eq!(std::fs::read(d.join("p").join("sub").join("y.txt")).unwrap(), b"yyy");
        assert_eq!(prog.bytes.load(Ordering::Relaxed), 5);
        limpar(&o);
    }

    #[test]
    fn cancelado_antes_de_comecar_nao_copia_nada() {
        let (o, d) = cenario("cancel");
        std::fs::write(o.join("a.txt"), b"hello").unwrap();
        let plano = planejar(&[o.join("a.txt")], &d).unwrap();
        let prog = Progresso::default();
        prog.cancelar.store(true, Ordering::Relaxed);
        let r = executar(&plano, &prog);
        assert!(r.cancelado);
        assert_eq!(r.copiados, 0);
        assert!(!d.join("a.txt").exists());
        limpar(&o);
    }

    #[test]
    fn nunca_sobrescreve_mesmo_se_o_destino_surgir_depois_do_plano() {
        let (o, d) = cenario("corrida");
        std::fs::write(o.join("a.txt"), b"novo").unwrap();
        let plano = planejar(&[o.join("a.txt")], &d).unwrap();
        // o arquivo aparece entre o plano e a execução
        std::fs::write(d.join("a.txt"), b"antigo").unwrap();
        let r = executar(&plano, &Progresso::default());
        assert_eq!(r.copiados, 0);
        assert_eq!(r.erros.len(), 1);
        assert_eq!(std::fs::read(d.join("a.txt")).unwrap(), b"antigo");
        limpar(&o);
    }

    #[test]
    fn falha_em_um_item_nao_aborta_os_outros() {
        let (o, d) = cenario("falha");
        std::fs::write(o.join("a.txt"), b"1").unwrap();
        std::fs::write(o.join("b.txt"), b"2").unwrap();
        let mut plano = planejar(&[o.join("a.txt"), o.join("b.txt")], &d).unwrap();
        std::fs::remove_file(o.join("a.txt")).unwrap(); // some antes de copiar
        let r = executar(&plano, &Progresso::default());
        assert_eq!(r.erros.len(), 1);
        assert_eq!(r.copiados, 1);
        assert!(d.join("b.txt").exists());
        plano.itens.clear();
        limpar(&o);
    }
```

E acrescentar `use std::sync::atomic::Ordering;` no topo do `mod testes`.

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --lib copy` → erro de compilação (`executar`, `Progresso`).

- [ ] **Passo 3: Implementação mínima** (acrescentar a `copy.rs`, antes do `mod testes`)

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// Contadores lidos pela interface enquanto a cópia roda em outra thread.
#[derive(Debug, Default)]
pub struct Progresso {
    pub bytes: AtomicU64,
    pub itens: AtomicUsize,
    pub cancelar: AtomicBool,
}

#[derive(Debug, Default)]
pub struct ResultadoCopia {
    pub copiados: usize,
    pub bytes: u64,
    pub erros: Vec<String>,
    pub cancelado: bool,
}

/// Executa o plano. Confere o cancelamento **entre itens**.
// ponytail: um arquivo muito grande não é interrompido no meio (std::fs::copy).
// Se isso incomodar, trocar por cópia em blocos conferindo `cancelar` a cada bloco.
pub fn executar(plano: &PlanoCopia, prog: &Progresso) -> ResultadoCopia {
    let mut r = ResultadoCopia::default();
    for item in &plano.itens {
        if prog.cancelar.load(Ordering::Relaxed) {
            r.cancelado = true;
            break;
        }
        // Defesa contra corrida: o destino pode ter surgido depois do plano
        if item.destino.exists() {
            r.erros.push(format!("{}: já existe, não foi sobrescrito", item.destino.display()));
            continue;
        }
        let resultado = if item.eh_pasta {
            std::fs::create_dir_all(&item.destino).map(|_| 0u64)
        } else {
            std::fs::copy(&item.origem, &item.destino)
        };
        match resultado {
            Ok(bytes) => {
                r.copiados += 1;
                r.bytes += bytes;
                prog.bytes.fetch_add(bytes, Ordering::Relaxed);
                prog.itens.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => r.erros.push(format!("{}: {e}", item.origem.display())),
        }
    }
    r
}
```

`mod.rs`: `pub use copy::{ItemCopia, PlanoCopia, Progresso, ResultadoCopia, executar, planejar};`

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --lib copy` → 11 testes passam (10 fora do Windows).

- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): execução de cópia cancelável e sem sobrescrita`.

---

### Tarefa 7: Estado da aba (navegação em segundo plano)

**Arquivos:**
- Criar: `src/tui/fm.rs`, `tests/fm_state_tests.rs`
- Modificar: `src/tui/mod.rs` (`pub mod fm;`)

**Interfaces:**
- Consome: `modules::fm::{listar, ordenar, visiveis, unidades, FmEntry, FmSort}`.
- Produz: `FmModo`; `FmState` com campos públicos `dir, em_unidades, visiveis, cursor, tabela, marcados, area, ordem, mostrar_ocultos, filtro, modo, entrada, erro, mensagem`; `FmState::novo(inicio: PathBuf, auditoria: Option<PathBuf>) -> Self`; `abrir(PathBuf)`; `subir()`; `entrar()`; `poll()`; `captura_teclado() -> bool`; `tecla(KeyEvent)`.

- [ ] **Passo 1: Testes que falham** (`tests/fm_state_tests.rs`)

```rust
use cli_admin::tui::fm::{FmModo, FmState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn tecla(s: &mut FmState, c: KeyCode) {
    s.tecla(KeyEvent::new(c, KeyModifiers::NONE));
}

/// Consulta o estado como a TUI faz (a cada 150 ms) até a condição valer, ou falha em 10 s.
fn esperar(s: &mut FmState, mut ok: impl FnMut(&FmState) -> bool) {
    let limite = Instant::now() + Duration::from_secs(10);
    loop {
        s.poll();
        if ok(s) {
            return;
        }
        assert!(Instant::now() < limite, "tempo esgotado esperando o estado");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn cenario(nome: &str) -> PathBuf {
    let raiz = std::env::temp_dir().join(format!("cli_admin_fmstate_{nome}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    std::fs::create_dir_all(raiz.join("pasta_a")).unwrap();
    std::fs::create_dir_all(raiz.join("pasta_b")).unwrap();
    std::fs::write(raiz.join("arq.txt"), b"conteudo").unwrap();
    raiz
}

#[test]
fn abre_a_pasta_inicial_em_segundo_plano() {
    let raiz = cenario("abre");
    let mut s = FmState::novo(raiz.clone(), None);
    // a lista começa vazia e chega pelo poll, não na hora
    esperar(&mut s, |s| s.visiveis.len() == 3);
    assert_eq!(s.dir, raiz);
    assert!(s.visiveis[0].eh_pasta, "pastas primeiro");
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn entra_e_sobe_de_pasta() {
    let raiz = cenario("entra");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    tecla(&mut s, KeyCode::Enter); // cursor em pasta_a
    esperar(&mut s, |s| s.dir == raiz.join("pasta_a"));
    tecla(&mut s, KeyCode::Backspace);
    esperar(&mut s, |s| s.dir == raiz);
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn pasta_inexistente_mantem_a_pasta_atual_e_mostra_o_erro() {
    let raiz = cenario("erro");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    s.abrir(raiz.join("nao_existe"));
    esperar(&mut s, |s| s.erro.is_some());
    assert_eq!(s.dir, raiz, "continua na pasta anterior");
    assert_eq!(s.visiveis.len(), 3, "a lista anterior não some");
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn na_raiz_subir_mostra_as_unidades() {
    let mut s = FmState::novo(PathBuf::from(r"C:\"), None);
    esperar(&mut s, |s| !s.visiveis.is_empty());
    tecla(&mut s, KeyCode::Backspace);
    assert!(s.em_unidades);
    assert!(s.visiveis.iter().any(|e| e.nome.to_uppercase().starts_with("C:")));
}

#[test]
fn filtro_e_ordenacao_e_ocultos() {
    let raiz = cenario("filtro");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    tecla(&mut s, KeyCode::Char('/'));
    assert_eq!(s.modo, FmModo::Filtro);
    for c in "arq".chars() {
        tecla(&mut s, KeyCode::Char(c));
    }
    tecla(&mut s, KeyCode::Enter);
    assert_eq!(s.visiveis.len(), 1);
    assert_eq!(s.visiveis[0].nome, "arq.txt");
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn campo_de_texto_captura_teclas_globais() {
    // Digitar "q5" no nome de uma pasta não pode sair do programa nem trocar de aba
    let raiz = cenario("captura");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    assert!(!s.captura_teclado());
    tecla(&mut s, KeyCode::Char('n'));
    assert!(s.captura_teclado());
    tecla(&mut s, KeyCode::Char('q'));
    tecla(&mut s, KeyCode::Char('5'));
    assert_eq!(s.entrada, "q5");
    let _ = std::fs::remove_dir_all(&raiz);
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test fm_state_tests` → erro de compilação (`cli_admin::tui::fm` não existe).

- [ ] **Passo 3: Implementação mínima** (`src/tui/fm.rs`; a cópia e a criação de pasta entram na Tarefa 8, por isso aqui `tecla` trata navegação, filtro, ordenação, ocultos, marcação e campos de texto)

```rust
//! Estado e teclas da aba Arquivos.
//! **Nenhuma E/S de disco roda na thread da interface**: listar, planejar, copiar e criar pasta
//! rodam em threads de fundo e são consultados por `poll()` a cada tick de 150 ms.

use crate::modules::fm::{self, FmEntry, FmSort};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::widgets::TableState;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FmModo {
    Normal,
    Filtro,
    Caminho,
    NovaPasta,
}

/// Roda `f` em outra thread e devolve o canal do resultado.
pub(crate) fn em_segundo_plano<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> mpsc::Receiver<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx
}

struct Carga {
    alvo: PathBuf,
    rx: mpsc::Receiver<Result<Vec<FmEntry>, String>>,
}

pub struct FmState {
    pub dir: PathBuf,
    pub em_unidades: bool,
    todas: Vec<FmEntry>,
    pub visiveis: Vec<FmEntry>,
    pub cursor: usize,
    pub tabela: TableState,
    pub marcados: HashSet<PathBuf>,
    /// Itens guardados com `y`, à espera do `p`.
    pub area: Vec<PathBuf>,
    pub ordem: FmSort,
    pub mostrar_ocultos: bool,
    pub filtro: String,
    pub modo: FmModo,
    /// Texto do campo ativo (caminho ou nome da nova pasta).
    pub entrada: String,
    pub erro: Option<String>,
    /// Mensagem curta para a barra de status do programa (a TUI a consome).
    pub mensagem: Option<String>,
    pub(crate) auditoria: Option<PathBuf>,
    // Substituir `carga` descarta o receptor anterior: resposta antiga nunca vale.
    carga: Option<Carga>,
}

impl FmState {
    pub fn novo(inicio: PathBuf, auditoria: Option<PathBuf>) -> Self {
        let mut s = FmState {
            dir: inicio.clone(),
            em_unidades: false,
            todas: Vec::new(),
            visiveis: Vec::new(),
            cursor: 0,
            tabela: TableState::default(),
            marcados: HashSet::new(),
            area: Vec::new(),
            ordem: FmSort::Nome,
            mostrar_ocultos: false,
            filtro: String::new(),
            modo: FmModo::Normal,
            entrada: String::new(),
            erro: None,
            mensagem: None,
            auditoria,
            carga: None,
        };
        s.abrir(inicio);
        s
    }

    /// Pede a listagem de `alvo` em segundo plano. Só troca de pasta se a leitura der certo.
    pub fn abrir(&mut self, alvo: PathBuf) {
        let caminho = alvo.clone();
        let rx = em_segundo_plano(move || fm::listar(&caminho).map_err(|e| e.to_string()));
        self.carga = Some(Carga { alvo, rx });
    }

    pub fn carregando(&self) -> bool {
        self.carga.is_some()
    }

    fn reaplicar(&mut self) {
        fm::ordenar(&mut self.todas, self.ordem);
        self.visiveis = fm::visiveis(&self.todas, self.mostrar_ocultos, &self.filtro);
        self.cursor = self.cursor.min(self.visiveis.len().saturating_sub(1));
        self.tabela.select(if self.visiveis.is_empty() { None } else { Some(self.cursor) });
    }

    fn mostrar_unidades(&mut self) {
        self.em_unidades = true;
        self.todas = fm::unidades()
            .into_iter()
            .map(|u| FmEntry {
                nome: u.clone(),
                caminho: PathBuf::from(&u),
                eh_pasta: true,
                eh_link: false,
                bytes: 0,
                modificado: None,
                oculto: false,
            })
            .collect();
        self.marcados.clear();
        self.filtro.clear();
        self.cursor = 0;
        self.reaplicar();
    }

    /// Consulta o que está em segundo plano. Chamado a cada tick da interface.
    pub fn poll(&mut self) {
        let resposta = match &self.carga {
            Some(c) => Some(c.rx.try_recv()),
            None => None,
        };
        match resposta {
            Some(Ok(Ok(entradas))) => {
                if let Some(c) = self.carga.take() {
                    self.dir = c.alvo;
                }
                self.em_unidades = false;
                self.todas = entradas;
                self.erro = None;
                self.marcados.clear();
                self.cursor = 0;
                self.reaplicar();
            }
            Some(Ok(Err(e))) => {
                self.carga = None;
                self.erro = Some(e);
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.carga = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }
    }

    fn sob_cursor(&self) -> Option<&FmEntry> {
        self.visiveis.get(self.cursor)
    }

    pub fn entrar(&mut self) {
        let alvo = match self.sob_cursor() {
            Some(e) if e.eh_pasta => e.caminho.clone(),
            _ => return,
        };
        self.abrir(alvo);
    }

    pub fn subir(&mut self) {
        if self.em_unidades {
            return;
        }
        match self.dir.parent() {
            Some(p) if !p.as_os_str().is_empty() => self.abrir(p.to_path_buf()),
            _ => self.mostrar_unidades(),
        }
    }

    fn mover(&mut self, delta: isize) {
        if self.visiveis.is_empty() {
            return;
        }
        let max = self.visiveis.len() - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, max as isize) as usize;
        self.tabela.select(Some(self.cursor));
    }

    fn alternar_marca(&mut self) {
        if let Some(e) = self.sob_cursor() {
            let c = e.caminho.clone();
            if !self.marcados.remove(&c) {
                self.marcados.insert(c);
            }
            self.mover(1);
        }
    }

    /// `true` quando a aba precisa receber **todas** as teclas (campo de texto ou modal aberto),
    /// para que digitar `q` ou `5` não saia do programa nem troque de aba.
    pub fn captura_teclado(&self) -> bool {
        self.modo != FmModo::Normal
    }

    pub fn tecla(&mut self, key: KeyEvent) {
        match self.modo {
            FmModo::Normal => self.tecla_normal(key),
            _ => self.tecla_campo(key),
        }
    }

    fn tecla_campo(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                if self.modo == FmModo::Filtro {
                    self.filtro.clear();
                    self.reaplicar();
                }
                self.modo = FmModo::Normal;
                self.entrada.clear();
            }
            KeyCode::Enter => match self.modo {
                FmModo::Filtro => self.modo = FmModo::Normal,
                FmModo::Caminho => {
                    let alvo = PathBuf::from(self.entrada.trim());
                    self.modo = FmModo::Normal;
                    self.entrada.clear();
                    self.abrir(alvo);
                }
                _ => {}
            },
            KeyCode::Backspace => {
                if self.modo == FmModo::Filtro {
                    self.filtro.pop();
                    self.reaplicar();
                } else {
                    self.entrada.pop();
                }
            }
            KeyCode::Char(c) => {
                if self.modo == FmModo::Filtro {
                    self.filtro.push(c);
                    self.reaplicar();
                } else {
                    self.entrada.push(c);
                }
            }
            _ => {}
        }
    }

    fn tecla_normal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.mover(-1),
            KeyCode::Down | KeyCode::Char('j') => self.mover(1),
            KeyCode::PageUp => self.mover(-10),
            KeyCode::PageDown => self.mover(10),
            KeyCode::Home | KeyCode::Char('g') => self.mover(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.mover(isize::MAX / 2),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.entrar(),
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Backspace => self.subir(),
            KeyCode::Char(' ') => self.alternar_marca(),
            KeyCode::Char('.') => {
                self.mostrar_ocultos = !self.mostrar_ocultos;
                self.reaplicar();
            }
            KeyCode::Char('s') => {
                self.ordem = self.ordem.proximo();
                self.reaplicar();
                self.mensagem = Some(format!("Ordenado por {}.", self.ordem.rotulo()));
            }
            KeyCode::Char('/') => {
                self.modo = FmModo::Filtro;
            }
            KeyCode::Char(':') => {
                self.modo = FmModo::Caminho;
                self.entrada = self.dir.display().to_string();
            }
            KeyCode::Char('r') => {
                if !self.em_unidades {
                    let d = self.dir.clone();
                    self.abrir(d);
                }
            }
            KeyCode::Esc => {
                self.marcados.clear();
                self.filtro.clear();
                self.reaplicar();
            }
            _ => {}
        }
    }
}
```

Em `src/tui/mod.rs`, acrescentar `pub mod fm;` (junto dos outros `pub mod`).

- [ ] **Passo 4: Rodar e ver passar**

Executar: `cargo test --offline --test fm_state_tests`
Esperado: 6 testes passam.

- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): estado do explorador com navegação em segundo plano`.

---

### Tarefa 8: Guardar, colar e criar pasta

**Arquivos:**
- Modificar: `src/tui/fm.rs`, `tests/fm_state_tests.rs`

**Interfaces:**
- Consome: `modules::fm::{planejar, executar, nome_valido, Progresso, PlanoCopia, ResultadoCopia}`, `support::volume_info`, `audit::registrar`.
- Produz em `FmState`: `confirmacao: Option<Confirmacao>` (pública, com `plano` e `destino`), `progresso() -> Option<ProgressoVista>`, `relatorio: Vec<String>`; teclas `y`, `p`, `n`, `Enter`/`Esc` do modal; `captura_teclado()` passa a incluir `confirmacao.is_some()`.

- [ ] **Passo 1: Testes que falham** (acrescentar a `tests/fm_state_tests.rs`)

```rust
#[test]
fn copiar_um_arquivo_de_ponta_a_ponta() {
    let raiz = cenario("copia");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    // posiciona o cursor em arq.txt (depois das duas pastas) e guarda
    tecla(&mut s, KeyCode::Down);
    tecla(&mut s, KeyCode::Down);
    tecla(&mut s, KeyCode::Char('y'));
    assert_eq!(s.area.len(), 1);
    // entra em pasta_a e cola
    tecla(&mut s, KeyCode::Char('g'));
    tecla(&mut s, KeyCode::Enter);
    esperar(&mut s, |s| s.dir == raiz.join("pasta_a"));
    tecla(&mut s, KeyCode::Char('p'));
    esperar(&mut s, |s| s.confirmacao.is_some());
    assert!(s.captura_teclado(), "com o modal aberto a aba captura as teclas");
    tecla(&mut s, KeyCode::Enter); // confirma
    esperar(&mut s, |s| s.progresso().is_none() && s.visiveis.iter().any(|e| e.nome == "arq.txt"));
    assert_eq!(std::fs::read(raiz.join("pasta_a").join("arq.txt")).unwrap(), b"conteudo");
    assert!(raiz.join("arq.txt").exists(), "a origem continua onde estava");
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn colar_sem_nada_guardado_avisa() {
    let raiz = cenario("vazio");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    tecla(&mut s, KeyCode::Char('p'));
    assert!(s.confirmacao.is_none());
    assert!(s.mensagem.as_deref().unwrap_or("").contains("guard"));
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn esc_no_modal_nao_copia() {
    let raiz = cenario("esc");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    tecla(&mut s, KeyCode::Down);
    tecla(&mut s, KeyCode::Down);
    tecla(&mut s, KeyCode::Char('y'));
    tecla(&mut s, KeyCode::Char('p')); // cola na própria pasta: vira "arq (2).txt"
    esperar(&mut s, |s| s.confirmacao.is_some());
    tecla(&mut s, KeyCode::Esc);
    assert!(s.confirmacao.is_none());
    assert!(!raiz.join("arq (2).txt").exists());
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn criar_pasta_valida_o_nome() {
    let raiz = cenario("mkdir");
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    tecla(&mut s, KeyCode::Char('n'));
    for c in "CON".chars() {
        tecla(&mut s, KeyCode::Char(c));
    }
    tecla(&mut s, KeyCode::Enter);
    assert!(s.erro.is_some(), "nome reservado deve ser recusado");
    assert_eq!(s.modo, FmModo::NovaPasta, "o campo continua aberto para corrigir");
    // corrige e cria
    for _ in 0..3 {
        tecla(&mut s, KeyCode::Backspace);
    }
    for c in "Nova".chars() {
        tecla(&mut s, KeyCode::Char(c));
    }
    tecla(&mut s, KeyCode::Enter);
    esperar(&mut s, |s| s.visiveis.iter().any(|e| e.nome == "Nova"));
    assert!(raiz.join("Nova").is_dir());
    assert_eq!(s.modo, FmModo::Normal);
    let _ = std::fs::remove_dir_all(&raiz);
}

#[test]
fn recusa_copiar_pasta_para_dentro_de_si_mesma() {
    let raiz = cenario("dentro");
    std::fs::create_dir_all(raiz.join("pasta_a").join("fundo")).unwrap();
    let mut s = FmState::novo(raiz.clone(), None);
    esperar(&mut s, |s| s.visiveis.len() == 3);
    tecla(&mut s, KeyCode::Char('y')); // pasta_a
    tecla(&mut s, KeyCode::Enter);
    esperar(&mut s, |s| s.dir == raiz.join("pasta_a"));
    tecla(&mut s, KeyCode::Char('p'));
    esperar(&mut s, |s| s.mensagem.is_some() || s.confirmacao.is_some());
    assert!(s.confirmacao.is_none(), "não pode haver plano");
    assert!(s.mensagem.as_deref().unwrap_or("").contains("dentro"));
    let _ = std::fs::remove_dir_all(&raiz);
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test fm_state_tests` → erro de compilação (`confirmacao`, `progresso`).

- [ ] **Passo 3: Implementação**

Em `src/tui/fm.rs`, acrescentar as importações e tipos:

```rust
use crate::audit;
use crate::modules::fm::{
    PlanoCopia, Progresso, ResultadoCopia, executar, nome_valido, planejar,
};
use crate::modules::support::{bytes_to_human, volume_info};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

/// Folga exigida além dos bytes do plano ao conferir o espaço livre do destino.
const MARGEM_ESPACO: u64 = 64 * 1024 * 1024;

pub struct Confirmacao {
    pub plano: PlanoCopia,
    pub destino: PathBuf,
}

struct Planejando {
    rx: mpsc::Receiver<(Result<PlanoCopia, String>, Option<u64>)>,
    destino: PathBuf,
}

struct CopiaAtiva {
    prog: Arc<Progresso>,
    rx: mpsc::Receiver<ResultadoCopia>,
    total_itens: usize,
    total_bytes: u64,
    destino: PathBuf,
    inicio: Instant,
}

struct CriandoPasta {
    rx: mpsc::Receiver<Result<PathBuf, String>>,
}

/// Fotografia do progresso para a tela.
pub struct ProgressoVista {
    pub itens: usize,
    pub total_itens: usize,
    pub bytes: u64,
    pub total_bytes: u64,
    pub segundos: u64,
}
```

Campos novos em `FmState` (e inicialização em `novo`): `pub confirmacao: Option<Confirmacao>`, `pub relatorio: Vec<String>`, `planejando: Option<Planejando>`, `copia: Option<CopiaAtiva>`, `criando: Option<CriandoPasta>`, todos começando em `None`/vazio.

Métodos (dentro de `impl FmState`):

```rust
    pub fn progresso(&self) -> Option<ProgressoVista> {
        self.copia.as_ref().map(|c| ProgressoVista {
            itens: c.prog.itens.load(Ordering::Relaxed),
            total_itens: c.total_itens,
            bytes: c.prog.bytes.load(Ordering::Relaxed),
            total_bytes: c.total_bytes,
            segundos: c.inicio.elapsed().as_secs(),
        })
    }

    fn auditar(&self, acao: &str, resultado: &str, msg: &str) {
        if let Some(arq) = &self.auditoria {
            let _ = audit::registrar(arq, acao, resultado, msg);
        }
    }

    fn guardar(&mut self) {
        let itens: Vec<PathBuf> = if self.marcados.is_empty() {
            self.sob_cursor().map(|e| vec![e.caminho.clone()]).unwrap_or_default()
        } else {
            self.marcados.iter().cloned().collect()
        };
        if itens.is_empty() || self.em_unidades {
            self.mensagem = Some("Nada para guardar aqui.".into());
            return;
        }
        self.mensagem = Some(format!(
            "{} item(ns) guardado(s). Vá à pasta de destino e aperte [p].",
            itens.len()
        ));
        self.area = itens;
    }

    fn colar(&mut self) {
        if self.area.is_empty() {
            self.mensagem = Some("Nada guardado: marque e aperte [y] antes.".into());
            return;
        }
        if self.em_unidades || self.planejando.is_some() || self.copia.is_some() {
            self.mensagem = Some("Escolha uma pasta de destino (ou espere a operação atual).".into());
            return;
        }
        let (fontes, destino) = (self.area.clone(), self.dir.clone());
        let d2 = destino.clone();
        let rx = em_segundo_plano(move || {
            let livre = volume_info(&d2).map(|v| v.free);
            (planejar(&fontes, &d2), livre)
        });
        self.planejando = Some(Planejando { rx, destino });
        self.mensagem = Some("Calculando o plano de cópia...".into());
    }

    fn confirmar_copia(&mut self) {
        let Some(c) = self.confirmacao.take() else { return };
        let prog = Arc::new(Progresso::default());
        let (total_itens, total_bytes) = (c.plano.itens.len(), c.plano.total_bytes);
        let (p2, plano) = (Arc::clone(&prog), c.plano);
        let rx = em_segundo_plano(move || executar(&plano, &p2));
        self.auditar(
            "fm.copiar",
            "iniciado",
            &format!("{} item(ns), {} para {}", total_itens, bytes_to_human(total_bytes), c.destino.display()),
        );
        self.copia = Some(CopiaAtiva { prog, rx, total_itens, total_bytes, destino: c.destino, inicio: Instant::now() });
    }

    fn tecla_confirmacao(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => self.confirmar_copia(),
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.confirmacao = None;
                self.mensagem = Some("Cópia cancelada.".into());
            }
            _ => {}
        }
    }

    fn criar_pasta(&mut self) {
        let nome = self.entrada.trim().to_string();
        if let Err(e) = nome_valido(&nome) {
            self.erro = Some(e); // o campo continua aberto para corrigir
            return;
        }
        self.erro = None;
        let alvo = self.dir.join(&nome);
        let rx = em_segundo_plano(move || {
            std::fs::create_dir(&alvo).map(|_| alvo.clone()).map_err(|e| e.to_string())
        });
        self.criando = Some(CriandoPasta { rx });
        self.modo = FmModo::Normal;
        self.entrada.clear();
    }
```

Substituir `captura_teclado` e `tecla`:

```rust
    pub fn captura_teclado(&self) -> bool {
        self.modo != FmModo::Normal || self.confirmacao.is_some()
    }

    pub fn tecla(&mut self, key: KeyEvent) {
        if self.confirmacao.is_some() {
            return self.tecla_confirmacao(key);
        }
        match self.modo {
            FmModo::Normal => self.tecla_normal(key),
            _ => self.tecla_campo(key),
        }
    }
```

Em `tecla_campo`, no braço `KeyCode::Enter`, tratar `FmModo::NovaPasta => self.criar_pasta(),` (substituindo o `_ => {}` desse `match`).

Em `tecla_normal`, acrescentar:

```rust
            KeyCode::Char('y') => self.guardar(),
            KeyCode::Char('p') => self.colar(),
            KeyCode::Char('n') => {
                if self.em_unidades {
                    self.mensagem = Some("Entre numa unidade antes de criar pasta.".into());
                } else {
                    self.modo = FmModo::NovaPasta;
                    self.entrada.clear();
                    self.erro = None;
                }
            }
```

E mudar o braço `KeyCode::Esc` de `tecla_normal` para cancelar a cópia em andamento antes de limpar marcas:

```rust
            KeyCode::Esc => {
                if let Some(c) = &self.copia {
                    c.prog.cancelar.store(true, Ordering::Relaxed);
                    self.mensagem = Some("Cancelando a cópia (termina o item atual)...".into());
                } else {
                    self.marcados.clear();
                    self.filtro.clear();
                    self.reaplicar();
                }
            }
```

Em `poll()`, acrescentar ao final (cada bloco usa o padrão "ler o resultado, soltar o empréstimo, então agir"):

```rust
        // plano de cópia
        let plano = self.planejando.as_ref().map(|p| p.rx.try_recv());
        match plano {
            Some(Ok((Ok(plano), livre))) => {
                let destino = self.planejando.take().map(|p| p.destino).unwrap_or_default();
                match livre {
                    Some(l) if plano.total_bytes + MARGEM_ESPACO > l => {
                        self.mensagem = Some(format!(
                            "Sem espaço: precisa de {} e há {} livres.",
                            bytes_to_human(plano.total_bytes + MARGEM_ESPACO),
                            bytes_to_human(l)
                        ));
                    }
                    _ => self.confirmacao = Some(Confirmacao { plano, destino }),
                }
            }
            Some(Ok((Err(e), _))) => {
                self.planejando = None;
                self.mensagem = Some(format!("Não foi possível copiar: {e}"));
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.planejando = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        // cópia em andamento
        let fim = self.copia.as_ref().map(|c| c.rx.try_recv());
        match fim {
            Some(Ok(r)) => {
                let destino = self.copia.take().map(|c| c.destino).unwrap_or_default();
                let resumo = format!(
                    "{} item(ns), {}, {} erro(s){}",
                    r.copiados,
                    bytes_to_human(r.bytes),
                    r.erros.len(),
                    if r.cancelado { ", CANCELADA" } else { "" }
                );
                self.auditar("fm.copiar", if r.erros.is_empty() { "ok" } else { "com erros" }, &resumo);
                self.mensagem = Some(format!("Cópia concluída: {resumo}"));
                self.relatorio = r.erros;
                self.abrir(destino); // recarrega para mostrar o que chegou
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.copia = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }

        // criação de pasta
        let nova = self.criando.as_ref().map(|c| c.rx.try_recv());
        match nova {
            Some(Ok(Ok(caminho))) => {
                self.criando = None;
                self.auditar("fm.criar_pasta", "ok", &caminho.display().to_string());
                self.mensagem = Some(format!("Pasta criada: {}", caminho.display()));
                let dir = self.dir.clone();
                self.abrir(dir);
            }
            Some(Ok(Err(e))) => {
                self.criando = None;
                self.erro = Some(format!("Não foi possível criar a pasta: {e}"));
            }
            Some(Err(mpsc::TryRecvError::Disconnected)) => self.criando = None,
            Some(Err(mpsc::TryRecvError::Empty)) | None => {}
        }
```

- [ ] **Passo 4: Rodar e ver passar** — `cargo test --offline --test fm_state_tests` → 11 testes passam. Se `recusa_copiar_pasta_para_dentro_de_si_mesma` falhar por tempo, conferir que `planejar` devolve a mensagem com "dentro" (Tarefa 5).

- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): copiar e criar pasta no explorador, com plano e auditoria`.

---

### Tarefa 9: Tela da aba e ligação com o programa

**Arquivos:**
- Criar: `src/tui/fm_ui.rs`, `tests/fm_ui_tests.rs`
- Modificar: `src/tui/mod.rs`, `src/tui/app.rs`, `src/tui/ui.rs`

**Interfaces:**
- Consome: `FmState`, `bytes_to_human`, `ui::centered_rect` (passa a `pub(super)`).
- Produz: `pub fn draw_files_tab(f: &mut Frame, area: Rect, fm: &mut FmState)`; `ActiveTab::Files = 4` e `ActiveTab::Support = 5`; tecla global `5` abre Arquivos e `6` abre Suporte.

- [ ] **Passo 1: Teste que falha** (`tests/fm_ui_tests.rs`)

```rust
use cli_admin::config::AppConfig;
use cli_admin::tui::app::{ActiveTab, App};
use cli_admin::tui::fm::FmState;
use cli_admin::tui::ui;
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn tela(terminal: &Terminal<TestBackend>) -> String {
    let b = terminal.backend().buffer().clone();
    let mut s = String::new();
    for y in 0..b.area.height {
        let l: String = (0..b.area.width).map(|x| b[(x, y)].symbol().to_string()).collect();
        s.push_str(l.trim_end());
        s.push('\n');
    }
    s
}

fn esperar_lista(fm: &mut FmState) {
    let limite = Instant::now() + Duration::from_secs(10);
    while fm.visiveis.is_empty() {
        fm.poll();
        assert!(Instant::now() < limite);
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[tokio::test]
async fn desenha_a_aba_arquivos_e_renumera_as_abas() {
    let raiz = std::env::temp_dir().join(format!("cli_admin_fmui_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    std::fs::create_dir_all(raiz.join("Documentos")).unwrap();
    std::fs::write(raiz.join("relatorio.txt"), vec![0u8; 2048]).unwrap();

    let mut app = App::new(AppConfig::default(), PathBuf::from("config.toml")).await;
    app.fm = FmState::novo(raiz.clone(), None);
    esperar_lista(&mut app.fm);
    app.active_tab = ActiveTab::Files;

    let mut terminal = Terminal::new(TestBackend::new(140, 30)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let t = tela(&terminal);
    println!("{t}");

    assert!(t.contains("[5]") && t.contains("Arquivos"), "aba no cabeçalho");
    assert!(t.contains("[6]") && t.contains("Suporte"), "Suporte virou [6]");
    assert!(t.contains("Documentos"), "pasta listada");
    assert!(t.contains("relatorio.txt") && t.contains("2.0 KB"), "arquivo com tamanho");
    assert!(t.contains(&raiz.display().to_string()), "caminho atual no topo");
    let _ = std::fs::remove_dir_all(&raiz);
}
```

- [ ] **Passo 2: Rodar e ver falhar** — `cargo test --offline --test fm_ui_tests` → erro de compilação (`app.fm`, `ActiveTab::Files`).

- [ ] **Passo 3: Implementação**

1. `src/tui/mod.rs`: acrescentar `pub mod fm_ui;`.
2. `src/tui/ui.rs`: trocar `fn centered_rect` por `pub(super) fn centered_rect`; no cabeçalho, trocar a linha `" [5] 🧰 Suporte "` por duas: `" [5] 📁 Arquivos "` e `" [6] 🧰 Suporte "`; no `match app.active_tab` de `draw`, acrescentar `ActiveTab::Files => super::fm_ui::draw_files_tab(f, chunks[1], &mut app.fm),`; no rodapé, acrescentar o braço de `ActiveTab::Files`:

```rust
        ActiveTab::Files => {
            "[↑↓] Mover | [Enter] Abrir | [Bksp] Subir | [Espaço] Marcar | [y] Copiar | [p] Colar | [n] Nova pasta | [/] Filtro | [:] Ir para | [s] Ordem | [.] Ocultos | [Esc] Cancelar | [q] Sair"
        }
```

e na ajuda, antes de "Aba 5: Suporte Técnico:" (renomear essa para "Aba 6") inserir o bloco "Aba 5: Arquivos" com uma linha por tecla da tabela do spec §6.

3. `src/tui/app.rs`:
   - `ActiveTab`: `Files = 4`, `Support = 5`; em `next`/`prev`: `Network → Files → Support → Vmware` e o inverso.
   - Campo `pub fm: super::fm::FmState` e, em `App::new`, `fm: super::fm::FmState::novo(dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")), Some(PathBuf::from(crate::audit::ARQUIVO_PADRAO)))`.
   - Em `handle_key`, **antes** do bloco `// Atalhos globais`:

```rust
        // A aba Arquivos, com campo de texto ou modal aberto, recebe todas as teclas
        if self.active_tab == ActiveTab::Files && self.fm.captura_teclado() {
            self.fm.tecla(key);
            self.drenar_mensagem_fm();
            return false;
        }
```

   - Teclas globais: `'5'` → `ActiveTab::Files`, `'6'` → `ActiveTab::Support`.
   - Despacho da aba: `ActiveTab::Files => { self.fm.tecla(key); self.drenar_mensagem_fm(); }`.
   - `select_next`/`select_prev` precisam de um braço `ActiveTab::Files => {}` (a aba trata as próprias setas).
   - Métodos novos:

```rust
    /// Leva a mensagem curta do explorador para a barra de status do programa.
    fn drenar_mensagem_fm(&mut self) {
        if let Some(m) = self.fm.mensagem.take() {
            self.set_status(m);
        }
    }
```

   - `poll_background` passa a ser:

```rust
    pub fn poll_background(&mut self) {
        self.poll_disk_scan();
        self.fm.poll();
        self.drenar_mensagem_fm();
    }
```

4. `src/tui/fm_ui.rs`:

```rust
//! Desenho da aba Arquivos.

use super::fm::{FmModo, FmState};
use super::ui::centered_rect;
use crate::modules::support::bytes_to_human;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Row, Table},
};

fn bloco(titulo: String, cor: Color) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(cor))
        .title(titulo)
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
}

fn data(t: Option<std::time::SystemTime>) -> String {
    t.map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "—".into())
}

pub fn draw_files_tab(f: &mut Frame, area: Rect, fm: &mut FmState) {
    let partes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4), Constraint::Length(3)])
        .split(area);

    // Topo: caminho atual (ou campo de texto ativo)
    let topo = match fm.modo {
        FmModo::Caminho => format!("Ir para: {}█", fm.entrada),
        FmModo::Filtro => format!("{}   Filtro: {}█", fm.dir.display(), fm.filtro),
        _ => {
            let onde = if fm.em_unidades { "Unidades".to_string() } else { fm.dir.display().to_string() };
            let extra = if fm.carregando() { "   ⏳ carregando..." } else { "" };
            let filtro = if fm.filtro.is_empty() { String::new() } else { format!("   filtro: {}", fm.filtro) };
            format!("{onde}{extra}{filtro}")
        }
    };
    f.render_widget(Paragraph::new(topo).block(bloco(" Arquivos ".into(), Color::Gray)), partes[0]);

    // Lista
    let marcados = fm.marcados.clone();
    let linhas: Vec<Row> = fm
        .visiveis
        .iter()
        .map(|e| {
            let marca = if marcados.contains(&e.caminho) { "✓" } else { " " };
            let mut nome = e.nome.clone();
            if e.eh_pasta {
                nome.push('/');
            }
            if e.eh_link {
                nome.push_str(" →");
            }
            let estilo = if e.eh_pasta {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else if e.oculto {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let tamanho = if e.eh_pasta { "<pasta>".to_string() } else { bytes_to_human(e.bytes) };
            Row::new(vec![
                Span::styled(marca, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(crate::texto::truncar(&nome, 80), estilo),
                Span::raw(tamanho),
                Span::raw(data(e.modificado)),
            ])
        })
        .collect();
    let titulo_lista = format!(
        " {} itens · ordem: {} · marcados: {}{} ",
        fm.visiveis.len(),
        fm.ordem.rotulo(),
        fm.marcados.len(),
        if fm.mostrar_ocultos { " · ocultos visíveis" } else { "" }
    );
    let tabela = Table::new(
        linhas,
        [Constraint::Length(2), Constraint::Min(30), Constraint::Length(12), Constraint::Length(17)],
    )
    .header(Row::new(vec!["", "NOME", "TAMANHO", "MODIFICADO"]).style(
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ))
    .block(bloco(titulo_lista, Color::Cyan))
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");
    f.render_stateful_widget(tabela, partes[1], &mut fm.tabela);

    // Rodapé: progresso da cópia, erro ou área de transferência
    if let Some(p) = fm.progresso() {
        let pct = if p.total_bytes > 0 {
            ((p.bytes as f64 / p.total_bytes as f64) * 100.0).round() as u16
        } else {
            0
        };
        let g = Gauge::default()
            .block(bloco(" Copiando ([Esc] cancela) ".into(), Color::Yellow))
            .gauge_style(Style::default().fg(Color::Green))
            .percent(pct.min(100))
            .label(format!(
                "{}/{} itens · {} de {} · {}s",
                p.itens, p.total_itens, bytes_to_human(p.bytes), bytes_to_human(p.total_bytes), p.segundos
            ));
        f.render_widget(g, partes[2]);
    } else {
        let texto = if let Some(e) = &fm.erro {
            format!("⚠ {e}")
        } else if !fm.area.is_empty() {
            format!("Guardado para copiar: {} item(ns). Vá ao destino e aperte [p].", fm.area.len())
        } else {
            "[y] guarda para copiar · [n] nova pasta · [:] ir para um caminho".to_string()
        };
        let cor = if fm.erro.is_some() { Color::Red } else { Color::DarkGray };
        f.render_widget(Paragraph::new(texto).style(Style::default().fg(cor)).block(bloco(String::new(), Color::DarkGray)), partes[2]);
    }

    // Modais
    if let Some(c) = &fm.confirmacao {
        let m = centered_rect(70, 50, f.area());
        f.render_widget(Clear, m);
        let mut linhas = vec![
            Line::from(Span::styled(
                format!("Copiar {} item(ns) · {}", c.plano.itens.len(), bytes_to_human(c.plano.total_bytes)),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(format!("Destino: {}", c.destino.display())),
        ];
        if c.plano.renomeados > 0 {
            linhas.push(Line::from(format!(
                "{} item(ns) já existem e ganharão o sufixo (2): nada é sobrescrito.",
                c.plano.renomeados
            )));
        }
        for i in c.plano.ignorados.iter().take(3) {
            linhas.push(Line::from(Span::styled(format!("ignorado: {i}"), Style::default().fg(Color::Yellow))));
        }
        let dir_txt = c.destino.display().to_string().to_lowercase();
        if dir_txt.starts_with(r"c:\windows") || dir_txt.starts_with(r"c:\program files") {
            linhas.push(Line::from(Span::styled(
                "⚠ Destino é uma pasta do sistema.",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
        }
        linhas.push(Line::from(""));
        linhas.push(Line::from("[Enter / y] COPIAR      [Esc / n] CANCELAR"));
        f.render_widget(
            Paragraph::new(linhas).alignment(Alignment::Center).block(bloco(" Confirmar cópia ".into(), Color::Yellow)),
            m,
        );
    }
    if fm.modo == FmModo::NovaPasta {
        let m = centered_rect(50, 25, f.area());
        f.render_widget(Clear, m);
        let mut linhas = vec![Line::from(format!("Nome: {}█", fm.entrada))];
        if let Some(e) = &fm.erro {
            linhas.push(Line::from(Span::styled(format!("⚠ {e}"), Style::default().fg(Color::Red))));
        }
        linhas.push(Line::from("[Enter] criar      [Esc] cancelar"));
        f.render_widget(
            Paragraph::new(linhas).alignment(Alignment::Center).block(bloco(" Nova pasta ".into(), Color::Green)),
            m,
        );
    }
}
```

- [ ] **Passo 4: Rodar e ver passar**

Executar: `cargo build --offline` (sem avisos) e `cargo test --offline`
Esperado: todos os testes passam, inclusive `desenha_a_aba_arquivos_e_renumera_as_abas`; os testes antigos `v3_render_grafico_de_disco_e_sem_aba_licencas` (que espera `[5]` e não `[6]`) precisa ser atualizado para as novas posições: a aba Suporte agora é `[6]`. Ajustar a asserção para `t.contains("[6] ") && !t.contains("[7] ")`.

- [ ] **Passo 5: Commit (se autorizado)** — `feat(cli_admin_v3): aba Arquivos com navegação, cópia e nova pasta`.

---

### Tarefa 10: Documentação e verificação final

**Arquivos:**
- Modificar: `docs/V3_MUDANCAS.md`, `AGENTS.md`, `README.md`

- [ ] **Passo 1:** Em `docs/V3_MUDANCAS.md`, acrescentar a seção "Explorador de arquivos" com: teclas, regras (nunca sobrescreve, não segue links, destino dentro da origem recusado, espaço conferido), limite conhecido (arquivo grande não é interrompido no meio) e o que ficou de fora (mover, renomear, apagar, preview, painel duplo).
- [ ] **Passo 2:** `README.md` e `AGENTS.md`: acrescentar a aba [5] e a renumeração (Suporte = [6]).
- [ ] **Passo 3: Verificação completa**

Executar: `cargo build --offline` e `cargo test --offline`, anotando o tempo total.
Esperado: sem avisos; todos os testes verdes; suíte em poucos segundos (um teste que passe a demorar é sinal a investigar, como no `volume_info`).

- [ ] **Passo 4: Verificação manual em terminal real** (não automatizável; registrar o resultado no `docs/V3_MUDANCAS.md`)

1. `cargo run --offline` → tecla `5`.
2. Entrar numa pasta com muitos arquivos: a tela continua respondendo durante o carregamento.
3. Marcar 2 arquivos com `Espaço`, `y`, entrar em outra pasta, `p`, conferir o modal, `Enter`; conferir a cópia e a linha em `audit/auditoria.log`.
4. Repetir `p` na mesma pasta: o modal mostra "ganharão o sufixo (2)".
5. Copiar uma pasta grande e apertar `Esc`: a cópia para ao fim do item atual.
6. `n` → digitar `CON` → mensagem de nome reservado; corrigir e criar.
7. Subir até `C:\`, depois `Backspace` mostra as unidades; entrar numa unidade de rede lenta (se houver) e confirmar que a interface não congela.

---

## Auto-revisão

1. **Cobertura do spec:** navegação (T7), ocultos/ordem/filtro (T3, T7), unidades (T4), seleção múltipla (T7), copiar (T5, T6, T8), criar pasta (T2, T8), conferência de espaço e auditoria (T8), regras de segurança §7 (T5, T6, T8), tela e modais (T9), documentação (T10). Nenhum requisito sem tarefa.
2. **Placeholders:** nenhum "TODO" ou "implementar depois"; as duas mudanças descritas em prosa na Tarefa 9 (cabeçalho, rodapé e ajuda de `ui.rs`; ligação em `app.rs`) listam o texto exato a inserir.
3. **Consistência de tipos:** `FmState::novo(PathBuf, Option<PathBuf>)`, `poll()`, `tecla(KeyEvent)`, `captura_teclado()`, `progresso() -> Option<ProgressoVista>`, `confirmacao: Option<Confirmacao>` usados igualmente nas Tarefas 7, 8 e 9. `planejar`/`executar`/`Progresso` definidos na T5/T6 e consumidos na T8.
4. **Validação do código (2026-10-17):** o código das **Tarefas 1 a 6** (auditoria, nomes, listagem, unidades, plano e execução de cópia) foi extraído do próprio plano para um projeto descartável, fora do V3, e testado: **26 testes passam, sem avisos**, inclusive a detecção real de junção (`mklink /J`), que era o risco registrado no spec. As **Tarefas 7 a 9 (estado e tela da TUI) NÃO foram compiladas**: há risco de erro de empréstimo ou de assinatura; corrija na própria tarefa, sem seguir adiante com teste vermelho.
