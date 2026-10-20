//! Aprovação de planos do MCP pela TUI (tecla F2). A aprovação continua sendo humana: a tela só
//! chama a mesma função que `cli_admin_v3 mcp aprovar`, e deixa registro na auditoria.

use cli_admin::config::AppConfig;
use cli_admin::mcp::approval;
use cli_admin::tui::app::{App, ModalState};
use cli_admin::tui::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use serde_json::json;
use std::path::PathBuf;

fn tecla(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

/// App com a pasta de planos e a auditoria apontando para um diretório temporário,
/// e um plano pendente já criado (como o servidor MCP faria).
async fn app_com_plano(nome: &str) -> (App, PathBuf, PathBuf, String) {
    let base = std::env::temp_dir().join(format!("cli_admin_planos_tui_{nome}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let pasta = base.join("planos");
    let auditoria = base.join("audit.log");

    let mut cfg = AppConfig::default();
    cfg.mcp.pasta_planos = pasta.to_string_lossy().to_string();
    cfg.mcp.auditoria = auditoria.to_string_lossy().to_string();

    let plano = approval::criar(
        &pasta,
        "servico_acao",
        &json!({ "nome": "Spooler", "acao": "stop" }),
        json!({ "passos": ["parar Spooler"] }),
        "stop 'Spooler': parar Spooler",
        approval::agora_ms(),
        300,
    )
    .unwrap();
    let app = App::new(cfg, base.join("config.toml")).await;
    (app, pasta, auditoria, plano.id)
}

#[tokio::test]
async fn f2_lista_o_plano_e_y_aprova_com_registro_na_auditoria() {
    let (mut app, pasta, auditoria, id) = app_com_plano("aprova").await;

    app.handle_key(tecla(KeyCode::F(2))).await;
    match &app.modal {
        ModalState::PlanosPendentes { planos, .. } => {
            assert_eq!(planos.len(), 1);
            assert_eq!(planos[0].id, id);
        }
        outro => panic!("F2 deveria abrir os planos pendentes, abriu {outro:?}"),
    }

    app.handle_key(tecla(KeyCode::Char('y'))).await;
    assert!(
        approval::listar_pendentes(&pasta, approval::agora_ms()).is_empty(),
        "plano aprovado sai da lista de pendentes"
    );
    let registro = std::fs::read_to_string(&auditoria).unwrap();
    assert!(registro.contains("mcp.aprovar"), "aprovação deve ficar na auditoria: {registro}");
    assert!(registro.contains("aprovado_por=tui"), "{registro}");
}

#[tokio::test]
async fn esc_fecha_sem_aprovar() {
    let (mut app, pasta, _auditoria, _id) = app_com_plano("cancela").await;

    app.handle_key(tecla(KeyCode::F(2))).await;
    app.handle_key(tecla(KeyCode::Esc)).await;
    assert!(matches!(app.modal, ModalState::None));
    assert_eq!(
        approval::listar_pendentes(&pasta, approval::agora_ms()).len(),
        1,
        "fechar a tela não pode aprovar nada"
    );
}

#[tokio::test]
async fn modal_mostra_id_e_acoes() {
    let (mut app, _pasta, _auditoria, id) = app_com_plano("tela").await;
    app.handle_key(tecla(KeyCode::F(2))).await;

    let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, &mut app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut tela = String::new();
    for y in 0..buffer.area.height {
        let linha: String = (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect();
        tela.push_str(linha.trim_end());
        tela.push('\n');
    }
    assert!(tela.contains(&id), "modal deve mostrar o id do plano");
    assert!(tela.contains("Aprovar selecionado"), "modal deve mostrar as ações");
}
