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
