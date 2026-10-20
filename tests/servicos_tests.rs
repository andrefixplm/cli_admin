//! Aba Serviços da TUI e configuração `[services]`. Dados injetados direto no `App`, sem SCM,
//! então rodam sem privilégio e sem tocar em nenhum serviço real.

use cli_admin::config::{AppConfig, ServicesConfig};
use cli_admin::modules::services::{StartType, SvcDetail};
use cli_admin::modules::support::SvcState;
use cli_admin::tui::app::{ActiveTab, App, ModalState};
use cli_admin::tui::ui;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};
use std::path::PathBuf;

fn tecla(c: KeyCode) -> KeyEvent {
    KeyEvent::new(c, KeyModifiers::NONE)
}

fn svc(nome: &str, exibicao: &str, estado: SvcState) -> SvcDetail {
    SvcDetail {
        name: nome.into(),
        display_name: exibicao.into(),
        state: estado,
        start_type: StartType::Automatico,
        binary_path: r"C:\Windows\System32\svc.exe".into(),
        account: "LocalSystem".into(),
    }
}

async fn app_com_servicos() -> App {
    let mut app = App::new(AppConfig::default(), PathBuf::from("config.toml")).await;
    app.active_tab = ActiveTab::Servicos;
    app.servicos = vec![
        svc("RpcSs", "Chamada de Procedimento Remoto", SvcState::Running),
        svc("Spooler", "Spooler de Impressão", SvcState::Stopped),
    ];
    app
}

fn tela(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(160, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        let linha: String = (0..buffer.area.width).map(|x| buffer[(x, y)].symbol().to_string()).collect();
        out.push_str(linha.trim_end());
        out.push('\n');
    }
    out
}

#[tokio::test]
async fn aba_mostra_servicos_e_marca_criticos() {
    let mut app = app_com_servicos().await;
    let t = tela(&mut app);
    assert!(t.contains("Spooler de Impressão"), "lista deve mostrar os serviços");
    assert!(t.contains("Serviços (2 de 2)"), "contador de serviços");
    // O TestBackend põe um espaço depois do emoji de escudo; confere a marca e o nome na mesma linha.
    let linha_critica = t.lines().find(|l| l.contains("RpcSs")).unwrap_or("");
    assert!(linha_critica.contains("🛡"), "serviço crítico deve ter a marca do escudo");
    let linha_comum = t.lines().find(|l| l.contains("Spooler de Impressão")).unwrap_or("");
    assert!(!linha_comum.contains("🛡"), "serviço comum não recebe a marca");
}

#[tokio::test]
async fn filtro_reduz_a_lista_e_esc_encerra_a_busca() {
    let mut app = app_com_servicos().await;
    app.handle_key(tecla(KeyCode::Char('/'))).await;
    assert!(app.servicos_busca);
    app.handle_key(tecla(KeyCode::Char('s'))).await;
    app.handle_key(tecla(KeyCode::Char('p'))).await;
    assert_eq!(app.servicos_visiveis().len(), 1, "filtro 'sp' deixa só o Spooler");
    // Enquanto digita, as teclas viram texto e não trocam de aba.
    app.handle_key(tecla(KeyCode::Char('7'))).await;
    assert_eq!(app.active_tab, ActiveTab::Servicos);
    app.handle_key(tecla(KeyCode::Esc)).await;
    assert!(!app.servicos_busca);
}

#[tokio::test]
async fn tecla_i_abre_escolha_de_inicio_e_esc_cancela() {
    let mut app = app_com_servicos().await;
    app.handle_key(tecla(KeyCode::Char('i'))).await;
    assert!(matches!(app.modal, ModalState::ServicoInicio { .. }));
    app.handle_key(tecla(KeyCode::Esc)).await;
    assert!(matches!(app.modal, ModalState::None));
}

#[tokio::test]
async fn parar_servico_critico_nao_chega_a_confirmacao() {
    let mut app = app_com_servicos().await;
    // RpcSs é o primeiro item da lista; 't' tenta parar.
    app.handle_key(tecla(KeyCode::Char('t'))).await;
    assert!(
        matches!(app.modal, ModalState::None),
        "crítico é recusado antes de abrir o modal de confirmação"
    );
}

#[tokio::test]
async fn tecla_7_abre_a_aba_e_proximo_tab_passa_por_ela() {
    let mut app = App::new(AppConfig::default(), PathBuf::from("config.toml")).await;
    app.handle_key(tecla(KeyCode::Char('7'))).await;
    assert_eq!(app.active_tab, ActiveTab::Servicos);
    app.handle_key(tecla(KeyCode::Tab)).await;
    assert_eq!(app.active_tab, ActiveTab::Vmware, "Tab volta ao início da ciclagem");
    app.handle_key(tecla(KeyCode::BackTab)).await;
    assert_eq!(app.active_tab, ActiveTab::Servicos);
}

#[test]
fn config_services_tem_padroes_e_aceita_toml_parcial() {
    let padrao = ServicesConfig::default();
    assert!(padrao.protected_names.is_empty());
    assert_eq!(padrao.timeout_padrao_s, 30);

    // Seção ausente e campo ausente não quebram o parse.
    let cfg: AppConfig = toml::from_str("[services]\nprotected_names = [\"MeuAgente\"]\n").unwrap();
    assert_eq!(cfg.services.protected_names, vec!["MeuAgente".to_string()]);
    assert_eq!(cfg.services.timeout_padrao_s, 30);
    let vazio: AppConfig = toml::from_str("").unwrap();
    assert_eq!(vazio.services.timeout_padrao_s, 30);
}
