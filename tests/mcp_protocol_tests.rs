#![cfg(feature = "mcp")]

use std::io::{BufRead, BufReader, Write};
use cli_admin::mcp::elevation;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Pasta temporária própria de cada teste, com planos, auditoria e uma raiz permitida.
struct Ambiente {
    base: PathBuf,
    config: PathBuf,
}

impl Ambiente {
    fn novo(nome: &str, allow_apply: bool) -> Self {
        let base = std::env::temp_dir().join(format!("cli_admin_mcpproto_{nome}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("raiz")).unwrap();
        let config = base.join("config.toml");
        let texto = format!(
            "[general]\nlog_file = '{}'\n\n[mcp]\nallow_apply = {allow_apply}\npasta_planos = '{}'\nauditoria = '{}'\nraizes_permitidas = ['{}']\n",
            base.join("app.log").display(),
            base.join("planos").display(),
            base.join("audit.log").display(),
            base.join("raiz").display(),
        );
        std::fs::write(&config, texto).unwrap();
        Self { base, config }
    }

    fn auditoria(&self) -> String {
        std::fs::read_to_string(self.base.join("audit.log")).unwrap_or_default()
    }
}

impl Drop for Ambiente {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Conversa com o `cli_admin_mcp` por stdio e devolve as linhas que ele escreveu no stdout.
fn conversar(config: &Path, linhas: &[String]) -> Vec<String> {
    let mut comando = Command::new(env!("CARGO_BIN_EXE_cli_admin_mcp"));
    comando.arg("--config").arg(config);
    // Se o ambiente de teste estiver elevado, o servidor recusa iniciar (T7). Para testar o
    // protocolo nesse caso, passa-se a opção explícita; a recusa tem teste próprio abaixo.
    if elevation::is_admin() {
        comando.arg("--permitir-elevado");
    }
    let mut filho = comando
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = filho.stdin.take().unwrap();
    for l in linhas {
        writeln!(stdin, "{l}").unwrap();
    }
    drop(stdin); // fecha a entrada: o servidor encerra ao fim dela
    let saida: Vec<String> = BufReader::new(filho.stdout.take().unwrap()).lines().map_while(Result::ok).collect();
    let _ = filho.wait();
    saida
}

/// Revisão 2026-07-28: não há mais handshake `initialize`; cada requisição declara a versão
/// do protocolo e as capacidades do cliente em `_meta`.
fn meta() -> &'static str {
    r#"{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{},"io.modelcontextprotocol/clientInfo":{"name":"teste","version":"0"}}"#
}

fn lista() -> String {
    format!(r#"{{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{{"_meta":{}}}}}"#, meta())
}

fn chamada(id: u32, ferramenta: &str, argumentos: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"{ferramenta}","arguments":{argumentos},"_meta":{}}}}}"#,
        meta()
    )
}

/// Texto devolvido por uma chamada de ferramenta (campo `content[0].text`).
fn texto_da_resposta(linhas: &[String], id: u32) -> String {
    let linha = linhas
        .iter()
        .find(|l| serde_json::from_str::<serde_json::Value>(l).is_ok_and(|v| v["id"] == id))
        .unwrap_or_else(|| panic!("sem resposta para o id {id}: {linhas:?}"));
    let v: serde_json::Value = serde_json::from_str(linha).unwrap();
    v["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string()
}

fn so_json_rpc(linhas: &[String]) {
    for l in linhas {
        let v: serde_json::Value = serde_json::from_str(l).unwrap_or_else(|_| panic!("stdout poluído: {l:?}"));
        assert_eq!(v["jsonrpc"], "2.0", "{l}");
    }
}

#[test]
fn stdout_so_tem_json_rpc_e_lista_so_as_ferramentas_seguras() {
    let amb = Ambiente::novo("lista", false);
    let linhas = conversar(&amb.config, &[lista()]);
    assert!(!linhas.is_empty(), "o servidor não respondeu");
    so_json_rpc(&linhas);
    let texto = linhas.join("\n");
    for esperada in ["processos_listar", "portas_listar", "fs_listar", "processo_planejar_encerramento"] {
        assert!(texto.contains(esperada), "faltou {esperada}: {texto}");
    }
    assert!(!texto.contains("plano_aplicar"), "escrita desligada por padrão não deve nem aparecer");
    for proibida in ["executar_comando", "shell", "exec"] {
        assert!(!texto.contains(&format!("\"{proibida}\"")), "ferramenta proibida: {proibida}");
    }
}

#[test]
fn com_allow_apply_a_ferramenta_de_aplicar_aparece() {
    let amb = Ambiente::novo("apply", true);
    let linhas = conversar(&amb.config, &[lista()]);
    assert!(linhas.join("\n").contains("plano_aplicar"));
}

#[test]
fn fs_listar_fora_da_raiz_e_recusado_sem_ecoar_o_caminho() {
    let amb = Ambiente::novo("fora", false);
    let caminho = if cfg!(windows) { r"C:\\Windows" } else { "/etc" };
    let linhas = conversar(&amb.config, &[chamada(3, "fs_listar", &format!(r#"{{"caminho":"{caminho}"}}"#))]);
    so_json_rpc(&linhas);
    let texto = texto_da_resposta(&linhas, 3);
    assert!(texto.contains("fora das raízes permitidas"), "{texto}");
}

#[test]
fn planejar_encerramento_de_processo_protegido_nao_gera_plano() {
    let amb = Ambiente::novo("protegido", false);
    // PID 4 é o processo System no Windows (sempre protegido)
    let linhas = conversar(&amb.config, &[chamada(4, "processo_planejar_encerramento", r#"{"pids":[4]}"#)]);
    so_json_rpc(&linhas);
    let texto = texto_da_resposta(&linhas, 4);
    assert!(texto.contains("nenhum alvo elegível"), "{texto}");
    assert!(!amb.base.join("planos").exists() || std::fs::read_dir(amb.base.join("planos")).unwrap().count() == 0);
}

#[test]
fn auditoria_grava_o_hash_e_nunca_os_argumentos() {
    let amb = Ambiente::novo("audit", false);
    let segredo = "segredo-do-caminho-xyz";
    let linhas = conversar(
        &amb.config,
        &[chamada(5, "fs_listar", &format!(r#"{{"caminho":"C:\\{segredo}"}}"#))],
    );
    so_json_rpc(&linhas);
    let log = amb.auditoria();
    assert!(log.contains("mcp.fs_listar"), "{log}");
    assert!(log.contains("args="), "deve registrar o hash dos argumentos: {log}");
    assert!(!log.contains(segredo), "argumentos não podem ir para a auditoria: {log}");
}

/// Sessão viva com o servidor: cada chamada é escrita e a resposta lida antes da próxima.
/// Necessária para o fluxo com aprovação humana no meio.
struct Sessao {
    filho: std::process::Child,
    entrada: std::process::ChildStdin,
    saida: BufReader<std::process::ChildStdout>,
}

impl Sessao {
    fn abrir(config: &Path) -> Self {
        let mut comando = Command::new(env!("CARGO_BIN_EXE_cli_admin_mcp"));
        comando.arg("--config").arg(config);
        if elevation::is_admin() {
            comando.arg("--permitir-elevado");
        }
        let mut filho = comando.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let entrada = filho.stdin.take().unwrap();
        let saida = BufReader::new(filho.stdout.take().unwrap());
        Self { filho, entrada, saida }
    }

    /// Envia uma requisição e devolve o texto da resposta de ferramenta com esse id.
    fn chamar(&mut self, id: u32, ferramenta: &str, argumentos: &str) -> String {
        writeln!(self.entrada, "{}", chamada(id, ferramenta, argumentos)).unwrap();
        loop {
            let mut linha = String::new();
            let lidos = self.saida.read_line(&mut linha).unwrap();
            assert!(lidos > 0, "o servidor encerrou sem responder ao id {id}");
            let v: serde_json::Value = serde_json::from_str(linha.trim()).unwrap_or_else(|_| panic!("stdout poluído: {linha:?}"));
            if v["id"] == id {
                return v["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string();
            }
        }
    }
}

impl Drop for Sessao {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

/// Fluxo completo com aprovação humana: planejar, tentar aplicar sem aprovação (recusa), o humano
/// aprova pelo executável `cli_admin_v3`, o agente aplica (encerra), e reusar o plano é recusado.
#[cfg(windows)]
#[test]
fn plano_com_aprovacao_humana_encerra_um_processo_de_teste() {
    let amb = Ambiente::novo("e2e", true);
    // processo descartável só para este teste
    let mut alvo = Command::new("ping")
        .args(["-n", "120", "127.0.0.1"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = alvo.id();
    let mut srv = Sessao::abrir(&amb.config);

    let plano = srv.chamar(1, "processo_planejar_encerramento", &format!(r#"{{"pids":[{pid}]}}"#));
    let plano_id = serde_json::from_str::<serde_json::Value>(&plano).unwrap()["plano_id"]
        .as_str()
        .unwrap_or_else(|| panic!("sem plano_id: {plano}"))
        .to_string();

    let antes = srv.chamar(2, "plano_aplicar", &format!(r#"{{"plano_id":"{plano_id}","pids":[{pid}]}}"#));
    assert!(antes.contains("aguardando aprovação"), "sem aprovação não pode aplicar: {antes}");

    let aprovacao = Command::new(env!("CARGO_BIN_EXE_cli_admin_v3"))
        .arg("--config")
        .arg(&amb.config)
        .args(["mcp", "aprovar", &plano_id])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(aprovacao.success(), "o humano deveria conseguir aprovar");

    let aplicado = srv.chamar(3, "plano_aplicar", &format!(r#"{{"plano_id":"{plano_id}","pids":[{pid}]}}"#));
    assert!(aplicado.contains(&pid.to_string()) && aplicado.contains("encerrados"), "{aplicado}");

    let mut terminou = false;
    for _ in 0..50 {
        if alvo.try_wait().unwrap().is_some() {
            terminou = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(terminou, "o processo de teste deveria ter sido encerrado");

    let de_novo = srv.chamar(4, "plano_aplicar", &format!(r#"{{"plano_id":"{plano_id}","pids":[{pid}]}}"#));
    assert!(de_novo.contains("já foi usado"), "uso único: {de_novo}");
    let _ = alvo.kill();
}

#[test]
fn recusa_iniciar_elevado_sem_a_opcao_explicita() {
    if !elevation::is_admin() {
        // ambiente não elevado: a recusa não se aplica; o caso fica coberto pelos testes de elevation
        return;
    }
    let amb = Ambiente::novo("elevado", false);
    let saida = Command::new(env!("CARGO_BIN_EXE_cli_admin_mcp"))
        .arg("--config")
        .arg(&amb.config)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(saida.status.code(), Some(2), "deveria recusar com código 2");
    assert!(saida.stdout.is_empty(), "nada pode ir ao stdout quando recusa");
}
