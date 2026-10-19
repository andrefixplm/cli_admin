//! Política do servidor MCP. **Funções puras, sem dependência do SDK**, para serem testadas
//! sem rede e sobreviverem a uma troca de SDK. A redação é MELHOR ESFORÇO, não garantia:
//! um segredo em formato inesperado pode passar. Por isso o servidor também limita o que expõe.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Nomes de opção/campo que carregam segredo (comparação exata, sem diferenciar caixa).
const CHAVES: [&str; 12] = [
    "password", "passwd", "pwd", "pass", "secret", "token", "apikey", "api-key", "api_key", "key",
    "auth", "authorization",
];

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Divide por espaços, mas mantém junto o que está entre aspas duplas.
fn dividir(linha: &str) -> Vec<String> {
    let (mut tokens, mut atual, mut aspas) = (Vec::new(), String::new(), false);
    for c in linha.chars() {
        match c {
            '"' => {
                aspas = !aspas;
                atual.push(c);
            }
            c if c.is_whitespace() && !aspas => {
                if !atual.is_empty() {
                    tokens.push(std::mem::take(&mut atual));
                }
            }
            c => atual.push(c),
        }
    }
    if !atual.is_empty() {
        tokens.push(atual);
    }
    tokens
}

fn nome_da_chave(token: &str) -> String {
    token.trim_start_matches(['-', '/', '"', '\'']).trim_end_matches([':', '=', '"']).to_ascii_lowercase()
}

fn redigir_cadeia_conexao(tok: &str) -> String {
    let mut s = tok.to_string();
    for chave in ["password=", "pwd="] {
        let mut ini = 0;
        loop {
            let baixa = s.to_ascii_lowercase();
            let Some(pos) = baixa[ini..].find(chave) else { break };
            let valor_ini = ini + pos + chave.len();
            let valor_fim = s[valor_ini..].find([';', '"']).map(|i| valor_ini + i).unwrap_or(s.len());
            s.replace_range(valor_ini..valor_fim, "***");
            ini = valor_ini + 3;
        }
    }
    s
}

fn redigir_chave_valor(tok: &str) -> String {
    if let Some(i) = tok.find(['=', ':']) {
        if CHAVES.contains(&nome_da_chave(&tok[..i]).as_str()) && i + 1 < tok.len() {
            return format!("{}***", &tok[..=i]);
        }
    }
    tok.to_string()
}

fn redigir_url(tok: &str) -> String {
    let Some(p) = tok.find("://") else { return tok.to_string() };
    let ini = p + 3;
    let fim = tok[ini..].find('/').map(|i| ini + i).unwrap_or(tok.len());
    let Some(arroba) = tok[ini..fim].rfind('@') else { return tok.to_string() };
    let usuario_senha = &tok[ini..ini + arroba];
    let Some(dp) = usuario_senha.find(':') else { return tok.to_string() };
    format!("{}{}:***{}", &tok[..ini], &usuario_senha[..dp], &tok[ini + arroba..])
}

fn parece_guid(t: &str) -> bool {
    let partes: Vec<&str> = t.split('-').collect();
    partes.len() == 5
        && [8, 4, 4, 4, 12].iter().zip(&partes).all(|(n, p)| p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn redigir_token(tok: &str) -> String {
    let mut s = redigir_cadeia_conexao(tok);
    s = redigir_chave_valor(&s);
    s = redigir_url(&s);
    // -p<senha> colado (estilo mysql). Falso positivo possível; se o resto for só dígitos, é porta.
    if s.len() > 2
        && s.starts_with("-p")
        && !s.starts_with("--")
        && !s.contains('=')
        && !s[2..].chars().all(|c| c.is_ascii_digit())
    {
        return "-p***".to_string();
    }
    // Sequência opaca longa (token de API em hexadecimal ou base64). GUID e caminhos são poupados.
    if s.len() >= 32
        && !parece_guid(&s)
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(c))
    {
        return "***".to_string();
    }
    s
}

/// Mascara senhas, tokens e cadeias de conexão numa linha de comando.
pub fn redigir_linha_de_comando(linha: &str) -> String {
    let mut saida: Vec<String> = Vec::new();
    let mut mascarar_proximo = false;
    for tok in dividir(linha) {
        if mascarar_proximo {
            mascarar_proximo = false;
            saida.push("***".into());
            continue;
        }
        let nome = nome_da_chave(&tok);
        let eh_opcao = tok.starts_with(['-', '/']);
        if nome == "bearer" || (eh_opcao && !tok.contains(['=', ':']) && CHAVES.contains(&nome.as_str())) {
            saida.push(tok);
            mascarar_proximo = true;
            continue;
        }
        saida.push(redigir_token(&tok));
    }
    saida.join(" ")
}

fn corresponde_negado(caminho: &Path, padrao: &str) -> bool {
    let p = padrao.to_ascii_lowercase();
    if let Some(ext) = p.strip_prefix("*.") {
        return caminho
            .file_name()
            .is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().ends_with(&format!(".{ext}")));
    }
    caminho.components().any(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase() == p)
}

/// Devolve o caminho **canônico** se estiver dentro de alguma raiz permitida e não casar com a
/// lista de negação. `canonicalize` resolve `..`, links e junções: quem aponta para fora é recusado.
pub fn caminho_permitido(raizes: &[PathBuf], negados: &[String], alvo: &Path) -> Result<PathBuf, String> {
    if raizes.is_empty() {
        return Err("nenhuma raiz permitida: configure [mcp].raizes_permitidas".into());
    }
    let canonico = std::fs::canonicalize(alvo).map_err(|_| "caminho inexistente ou ilegível".to_string())?;
    let dentro = raizes
        .iter()
        .filter_map(|r| std::fs::canonicalize(r).ok())
        .any(|r| canonico.starts_with(&r));
    if !dentro {
        return Err("caminho fora das raízes permitidas".into());
    }
    if let Some(regra) = negados.iter().find(|p| corresponde_negado(&canonico, p)) {
        return Err(format!("caminho bloqueado pela regra '{regra}'"));
    }
    Ok(canonico)
}

pub fn assinatura(ferramenta: &str, args: &str) -> u64 {
    fnv1a(format!("{ferramenta}\0{args}").as_bytes())
}

/// Limites contra abuso e laços de agente. O relógio entra como parâmetro: testável sem esperar.
pub struct Limitador {
    por_minuto: usize,
    max_sessao: u64,
    total: u64,
    janela: VecDeque<u64>,
    recentes: VecDeque<(u64, u64)>,
}

impl Limitador {
    pub fn new(por_minuto: usize, max_sessao: u64) -> Self {
        Self { por_minuto, max_sessao, total: 0, janela: VecDeque::new(), recentes: VecDeque::new() }
    }

    pub fn registrar(&mut self, agora_ms: u64, assinatura: u64) -> Result<(), String> {
        if self.total >= self.max_sessao {
            return Err("limite de chamadas da sessão atingido".into());
        }
        while self.janela.front().is_some_and(|t| agora_ms.saturating_sub(*t) >= 60_000) {
            self.janela.pop_front();
        }
        if self.janela.len() >= self.por_minuto {
            return Err("limite de chamadas por minuto atingido; aguarde".into());
        }
        while self.recentes.front().is_some_and(|(t, _)| agora_ms.saturating_sub(*t) >= 30_000) {
            self.recentes.pop_front();
        }
        if self.recentes.iter().filter(|(_, a)| *a == assinatura).count() >= 5 {
            return Err("chamada idêntica repetida demais; mude a estratégia".into());
        }
        self.janela.push_back(agora_ms);
        self.recentes.push_back((agora_ms, assinatura));
        self.total += 1;
        Ok(())
    }
}

fn invisivel(c: char) -> bool {
    matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

/// Texto livre vindo do sistema: sem controles nem caracteres invisíveis de direção, limitado.
pub fn sanitizar_campo(texto: &str, max: usize) -> String {
    let limpo: String = texto
        .chars()
        .filter(|c| !invisivel(*c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    crate::texto::truncar(limpo.trim(), max)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn sem_vazar(entrada: &str, segredo: &str) {
        let r = redigir_linha_de_comando(entrada);
        assert!(!r.contains(segredo), "vazou '{segredo}' em: {r}");
        assert!(r.contains("***"), "deveria ter mascarado: {r}");
    }

    #[test]
    fn mascara_chave_igual_valor() {
        sem_vazar("app.exe --password=hunter2 --user bob", "hunter2");
        sem_vazar("app.exe /pwd:hunter2", "hunter2");
        sem_vazar(r#"app.exe --token="abc def ghi""#, "def");
    }

    #[test]
    fn mascara_o_valor_que_vem_depois_da_flag() {
        sem_vazar("app.exe --password hunter2 --x 1", "hunter2");
        sem_vazar("app.exe -secret hunter2", "hunter2");
        let r = redigir_linha_de_comando("app.exe --api-key K123 --verbose");
        assert!(r.contains("--verbose"), "o resto da linha continua: {r}");
    }

    #[test]
    fn mascara_senha_colada_na_opcao_p() {
        sem_vazar("mysql -phunter2 -u root", "hunter2");
    }

    #[test]
    fn nao_mascara_p_numerico_nem_opcoes_legitimas() {
        for ok in [
            "app.exe -p8080",
            r"C:\Windows\System32\svchost.exe -k netsvcs -p",
            "chrome.exe --type=renderer --lang=pt-BR",
            "node server.js --port=3000 --key-file=chave.pem",
        ] {
            assert_eq!(redigir_linha_de_comando(ok), ok, "não deveria alterar: {ok}");
        }
    }

    #[test]
    fn mascara_bearer_e_credencial_em_url() {
        sem_vazar(r#"curl -H "Authorization: Bearer abc.def.ghi" https://x.com"#, "abc.def.ghi");
        sem_vazar("git clone https://user:tok3n@github.com/x/y.git", "tok3n");
        let r = redigir_linha_de_comando("git clone https://user:tok3n@github.com/x/y.git");
        assert!(r.contains("user:***@github.com"), "{r}");
    }

    #[test]
    fn mascara_cadeia_de_conexao() {
        sem_vazar("app.exe \"Server=a;Database=b;User Id=u;Password=s3cr3t;Encrypt=true\"", "s3cr3t");
        let r = redigir_linha_de_comando("x Server=a;Pwd=abc;Port=1");
        assert!(r.contains("Pwd=***;Port=1"), "{r}");
    }

    #[test]
    fn mascara_sequencia_opaca_longa_mas_poupa_guid_e_caminho() {
        sem_vazar("tool 0123456789abcdef0123456789abcdef0123", "0123456789abcdef");
        let guid = "app.exe 123e4567-e89b-12d3-a456-426614174000";
        assert_eq!(redigir_linha_de_comando(guid), guid, "GUID não é segredo");
        let caminho = r"C:\Program Files\Aplicativo\muito_longo_nome_de_executavel_sem_ponto_nenhum_aqui.exe";
        assert_eq!(redigir_linha_de_comando(caminho), caminho);
    }

    #[test]
    fn linha_vazia_e_espacos() {
        assert_eq!(redigir_linha_de_comando(""), "");
    }

    #[test]
    fn sanitizar_remove_controle_e_trunca() {
        assert_eq!(sanitizar_campo("a\nb\tc\u{7}d", 50), "a b c d");
        assert_eq!(sanitizar_campo(&"x".repeat(1000), 10).chars().count(), 10);
    }

    #[test]
    fn sanitizar_remove_caracteres_invisiveis_de_direcao() {
        // U+202E (inverte a direção do texto) é usado para esconder instruções e falsificar nomes
        let s = sanitizar_campo("arquivo\u{202E}gpj.exe\u{200B}", 50);
        assert!(!s.contains('\u{202E}') && !s.contains('\u{200B}'));
    }

    fn arvore(nome: &str) -> PathBuf {
        let raiz = std::env::temp_dir().join(format!("cli_admin_mcp_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        for rel in [r"dados\ok.txt", r"dados\.ssh\id_rsa", r"dados\cert.pem", r"fora\segredo.txt"] {
            let p = raiz.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"x").unwrap();
        }
        raiz
    }

    fn negados() -> Vec<String> {
        vec![".ssh".into(), "*.pem".into(), ".env".into()]
    }

    #[test]
    fn permite_dentro_da_raiz() {
        let t = arvore("permite");
        let r = caminho_permitido(&[t.join("dados")], &negados(), &t.join("dados").join("ok.txt"));
        assert!(r.is_ok(), "{r:?}");
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn sem_raizes_configuradas_recusa_tudo() {
        let t = arvore("sem_raiz");
        let e = caminho_permitido(&[], &negados(), &t.join("dados").join("ok.txt")).unwrap_err();
        assert!(e.contains("raiz"));
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn recusa_fora_da_raiz_e_por_ponto_ponto() {
        let t = arvore("fora");
        let raiz = t.join("dados");
        assert!(caminho_permitido(&[raiz.clone()], &negados(), &t.join("fora").join("segredo.txt")).is_err());
        let truque = raiz.join("..").join("fora").join("segredo.txt");
        assert!(caminho_permitido(&[raiz], &negados(), &truque).is_err(), "'..' não pode escapar da raiz");
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn recusa_caminhos_da_lista_de_negacao() {
        let t = arvore("negados");
        let raiz = t.join("dados");
        assert!(caminho_permitido(&[raiz.clone()], &negados(), &raiz.join(".ssh").join("id_rsa")).is_err());
        assert!(caminho_permitido(&[raiz.clone()], &negados(), &raiz.join("CERT.PEM")).is_err(), "sem diferenciar caixa");
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn caminho_inexistente_e_recusado_sem_ecoar_o_caminho() {
        let t = arvore("inexistente");
        let e = caminho_permitido(&[t.join("dados")], &negados(), &t.join("dados").join("nao_existe")).unwrap_err();
        assert!(!e.contains("nao_existe"));
        let _ = std::fs::remove_dir_all(&t);
    }

    #[cfg(windows)]
    #[test]
    fn juncao_para_fora_da_raiz_e_recusada() {
        let t = arvore("juncao");
        let raiz = t.join("dados");
        let link = raiz.join("atalho");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"]).arg(&link).arg(t.join("fora"))
            .output().unwrap().status.success();
        assert!(ok, "mklink /J falhou");
        let r = caminho_permitido(&[raiz], &negados(), &link.join("segredo.txt"));
        assert!(r.is_err(), "a junção aponta para fora da raiz: {r:?}");
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn limite_por_minuto_e_liberacao_com_o_tempo() {
        let mut l = Limitador::new(3, 1000);
        for i in 0..3 {
            l.registrar(i * 1000, i).unwrap(); // assinaturas diferentes
        }
        assert!(l.registrar(3000, 99).unwrap_err().contains("minuto"));
        assert!(l.registrar(61_000, 100).is_ok(), "passado um minuto a janela libera");
    }

    #[test]
    fn limite_total_da_sessao() {
        let mut l = Limitador::new(1000, 2);
        l.registrar(0, 1).unwrap();
        l.registrar(1, 2).unwrap();
        assert!(l.registrar(2, 3).unwrap_err().contains("sessão"));
    }

    #[test]
    fn corta_repeticao_identica_em_30_segundos() {
        let mut l = Limitador::new(1000, 1000);
        for i in 0..5 {
            l.registrar(i * 100, 7).unwrap();
        }
        assert!(l.registrar(600, 7).unwrap_err().contains("repetida"));
        assert!(l.registrar(600, 8).is_ok(), "outra chamada passa");
        assert!(l.registrar(31_000, 7).is_ok(), "depois de 30 s a repetição é esquecida");
    }

    #[test]
    fn assinatura_depende_da_ferramenta_e_dos_argumentos() {
        assert_eq!(assinatura("a", "{}"), assinatura("a", "{}"));
        assert_ne!(assinatura("a", "{}"), assinatura("b", "{}"));
        assert_ne!(assinatura("a", "{\"x\":1}"), assinatura("a", "{\"x\":2}"));
    }

    #[test]
    fn fnv1a_e_estavel() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }
}
