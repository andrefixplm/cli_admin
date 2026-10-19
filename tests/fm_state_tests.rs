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
    // "Calculando..." é síncrono; espera-se a resposta de fundo, que traz o erro
    esperar(&mut s, |s| s.mensagem.as_deref().unwrap_or("").contains("dentro"));
    assert!(s.confirmacao.is_none(), "não pode haver plano");
    assert!(s.mensagem.as_deref().unwrap_or("").contains("dentro"));
    let _ = std::fs::remove_dir_all(&raiz);
}
