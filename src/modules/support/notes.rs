use std::fs;
use std::path::{Path, PathBuf};

/// Metadados de uma nota (o corpo só é lido sob demanda).
#[derive(Debug, Clone)]
pub struct NoteMeta {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub created: String,
    pub path: PathBuf,
}

/// Backend de sincronização web (futuro: Notion/Jira/GLPI...).
/// Hoje sem implementação — as notas são 100% locais.
pub trait NotesBackend {
    fn name(&self) -> &str;
    fn push(&self, note_body: &str) -> Result<String, String>;
}

fn slugify(title: &str) -> String {
    let mut slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "nota".to_string()
    } else {
        slug.chars().take(48).collect()
    }
}

fn timestamp_now() -> (String, String) {
    let ts = chrono::Local::now();
    (
        ts.format("%Y%m%d-%H%M%S").to_string(),
        ts.format("%Y-%m-%d %H:%M").to_string(),
    )
}

/// Cria uma nota Markdown com cabeçalho simples (título, data, tags).
pub fn add_note(
    dir: &Path,
    title: &str,
    tags: &[String],
    body: &str,
) -> Result<NoteMeta, String> {
    fs::create_dir_all(dir).map_err(|e| format!("Erro ao criar diretório de notas: {}", e))?;
    let (stamp, created) = timestamp_now();
    let slug = slugify(title);
    let id = format!("{}-{}", stamp, slug);
    let path = dir.join(format!("{}.md", id));

    let mut content = format!("# {}\n\n> {} | id: {}", title, created, id);
    if !tags.is_empty() {
        content.push_str(&format!("\n> tags: {}", tags.join(", ")));
    }
    content.push_str("\n\n");
    content.push_str(body);
    if !body.ends_with('\n') {
        content.push('\n');
    }

    fs::write(&path, &content).map_err(|e| format!("Erro ao salvar nota: {}", e))?;
    Ok(NoteMeta {
        id,
        title: title.to_string(),
        tags: tags.to_vec(),
        created,
        path,
    })
}

fn parse_header(path: &Path) -> Option<NoteMeta> {
    let content = fs::read_to_string(path).ok()?;
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());
    let title = lines.next()?.strip_prefix("# ")?.to_string();
    let meta_line = lines.next().unwrap_or("");
    let (created, id_from_body) = if let Some(rest) = meta_line.strip_prefix("> ") {
        let mut it = rest.split("| id:");
        let created = it.next()?.trim().to_string();
        let id = it.next().map(|s| s.trim().to_string());
        (created, id)
    } else {
        (String::new(), None)
    };
    let mut tags = Vec::new();
    if let Some(tags_line) = lines.next() {
        if let Some(t) = tags_line.strip_prefix("> tags:") {
            tags = t.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
    }
    let id = id_from_body.or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
    })?;
    Some(NoteMeta { id, title, tags, created, path: path.to_path_buf() })
}

/// Lista notas (mais recentes primeiro), com busca textual opcional
/// em título, tags e corpo.
pub fn list_notes(dir: &Path, query: &str) -> Vec<NoteMeta> {
    let mut notes = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return notes,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(meta) = parse_header(&path) {
            notes.push(meta);
        }
    }
    // Nome do arquivo carrega timestamp → ordem decrescente = recentes primeiro
    notes.sort_by(|a, b| b.id.cmp(&a.id));

    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return notes;
    }
    notes
        .into_iter()
        .filter(|n| {
            n.title.to_lowercase().contains(&q)
                || n.tags.iter().any(|t| t.to_lowercase().contains(&q))
                || fs::read_to_string(&n.path)
                    .map(|b| b.to_lowercase().contains(&q))
                    .unwrap_or(false)
        })
        .collect()
}

/// Lê o corpo de uma nota pelo id (prefixo único basta).
pub fn read_note(dir: &Path, id_prefix: &str) -> Result<String, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Erro ao ler diretório: {}", e))?;
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.starts_with(id_prefix) {
                matches.push(path);
            }
        }
    }
    match matches.len() {
        0 => Err(format!("Nota '{}' não encontrada.", id_prefix)),
        1 => fs::read_to_string(&matches[0]).map_err(|e| format!("Erro ao ler nota: {}", e)),
        _ => Err(format!(
            "Prefixo '{}' é ambíguo ({} notas). Informe mais caracteres.",
            id_prefix,
            matches.len()
        )),
    }
}
