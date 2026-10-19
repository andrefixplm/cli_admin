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
