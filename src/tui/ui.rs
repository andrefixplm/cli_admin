use super::app::{ActiveTab, AddVmField, App, InputMode, ModalState};
use crate::modules::top::SortField;
use crate::modules::vmware::VmStatus;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, Gauge, Paragraph, Row, Table, Tabs, Wrap,
    },
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Layout principal vertical: Header, Conteúdo Central, Rodapé
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Header + System Gauges
            Constraint::Min(10),   // Active View
            Constraint::Length(3), // Footer
        ])
        .split(size);

    draw_header(f, chunks[0], app);

    match app.active_tab {
        ActiveTab::Vmware => draw_vmware_tab(f, chunks[1], app),
        ActiveTab::TopMetrics => draw_top_tab(f, chunks[1], app),
        ActiveTab::ProcessManager => draw_process_tab(f, chunks[1], app),
    }

    draw_footer(f, chunks[2], app);

    // Modais sobrepostos
    match &app.modal {
        ModalState::ConfirmKill { pids, has_critical } => {
            draw_confirm_kill_modal(f, size, pids, *has_critical);
        }
        ModalState::CreateSnapshot { snapshot_name, .. } => {
            draw_snapshot_modal(f, size, snapshot_name);
        }
        ModalState::AddVm { field, alias, path } => {
            draw_add_vm_modal(f, size, *field, alias, path);
        }
        ModalState::Help => {
            draw_help_modal(f, size);
        }
        ModalState::None => {}
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let header_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .split(area);

    // 1. Linha superior: Abas de navegação
    let tab_titles = vec![
        Span::styled(" [1] 🖥️  VMware Workstation ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" [2] 📊 TOP Recursos (RAM/GPU/IO/VRAM) ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" [3] 🎯 Gerenciador de Processos ", Style::default().add_modifier(Modifier::BOLD)),
    ];

    let selected_index = app.active_tab as usize;
    let tabs = Tabs::new(tab_titles)
        .select(selected_index)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(20, 30, 45))
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(tabs, header_chunks[0]);

    // 2. Linha inferior: Barras de recursos do sistema (CPU, RAM, VRAM/GPU)
    let gauge_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25), // CPU
            Constraint::Percentage(35), // RAM
            Constraint::Percentage(40), // GPU & VRAM
        ])
        .split(header_chunks[1]);

    // CPU Gauge
    let cpu_val = (app.overview.cpu_pct as u16).min(100);
    let cpu_color = if cpu_val > 85 {
        Color::Red
    } else if cpu_val > 60 {
        Color::Yellow
    } else {
        Color::Green
    };
    let cpu_gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(cpu_color))
        .percent(cpu_val)
        .label(format!("CPU: {}%", cpu_val));
    f.render_widget(cpu_gauge, gauge_chunks[0]);

    // RAM Gauge
    let ram_pct = (app.overview.ram_usage_pct() as u16).min(100);
    let used_ram_gb = app.overview.used_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_ram_gb = app.overview.total_ram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let ram_gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(Color::Cyan))
        .percent(ram_pct)
        .label(format!("RAM: {:.1}/{:.1} GB ({}%)", used_ram_gb, total_ram_gb, ram_pct));
    f.render_widget(ram_gauge, gauge_chunks[1]);

    // GPU & VRAM Gauge
    let vram_pct = (app.overview.vram_usage_pct() as u16).min(100);
    let used_vram_gb = app.overview.used_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let total_vram_gb = app.overview.total_vram_bytes as f64 / (1024.0 * 1024.0 * 1024.0);

    let gpu_extra = match (app.overview.gpu_temp_c, app.overview.gpu_power_watts) {
        (Some(t), Some(w)) => format!(" | {}°C {:.0}W", t, w),
        (Some(t), None) => format!(" | {}°C", t),
        _ => String::new(),
    };

    let gpu_label = format!("VRAM: {:.1}/{:.1} GB ({}%){}", used_vram_gb, total_vram_gb, vram_pct, gpu_extra);
    let vram_gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(Color::Magenta))
        .percent(vram_pct)
        .label(gpu_label);
    f.render_widget(vram_gauge, gauge_chunks[2]);
}

fn draw_vmware_tab(f: &mut Frame, area: Rect, app: &App) {
    let header_cells = ["ALIAS", "NOME DA MÁQUINA VIRTUAL", "STATUS", "CAMINHO (.VMX)"]
        .into_iter()
        .map(|h| Span::styled(h, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    let header_row = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .vms
        .iter()
        .enumerate()
        .map(|(idx, vm)| {
            let is_selected = idx == app.selected_vm_index;
            let alias = vm.alias.as_deref().unwrap_or("-");

            let (status_text, status_style) = match vm.status {
                VmStatus::Running => (
                    "● RUNNING",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
                VmStatus::Stopped => (
                    "○ STOPPED",
                    Style::default().fg(Color::DarkGray),
                ),
                VmStatus::Paused => (
                    "⏸ PAUSED",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                VmStatus::Offline => (
                    "✖ OFFLINE",
                    Style::default().fg(Color::Red),
                ),
                VmStatus::Unknown => ("? UNKNOWN", Style::default().fg(Color::Gray)),
            };

            let prefix = if is_selected { "▶ " } else { "  " };

            let cells = vec![
                Span::styled(format!("{}{}", prefix, alias), Style::default().fg(Color::Cyan)),
                Span::raw(&vm.name),
                Span::styled(status_text, status_style),
                Span::styled(vm.path.to_string_lossy().to_string(), Style::default().fg(Color::DarkGray)),
            ];

            let row_style = if is_selected {
                Style::default().bg(Color::Rgb(30, 45, 60))
            } else {
                Style::default()
            };

            Row::new(cells).style(row_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(30),
            Constraint::Length(14),
            Constraint::Min(30),
        ],
    )
    .header(header_row)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Inventário VMware Workstation ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    );

    f.render_widget(table, area);
}

fn draw_top_tab(f: &mut Frame, area: Rect, app: &mut App) {
    let sort_col = app.top_sort;

    let col_label = |name: &str, field: SortField| -> Span<'static> {
        if sort_col == field {
            Span::styled(
                format!("{} ▼", name),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
        } else {
            Span::styled(
                name.to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )
        }
    };

    let header_cells = vec![
        Span::styled("PID", Style::default().fg(Color::DarkGray)),
        Span::styled("PROCESSO", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        col_label("CPU %", SortField::Cpu),
        col_label("RAM (MB)", SortField::Ram),
        col_label("DISCO (KB/s)", SortField::DiskIo),
        col_label("GPU %", SortField::Gpu),
        col_label("VRAM (MB)", SortField::Vram),
    ];
    let header_row = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .top_processes
        .iter()
        .map(|proc| {
            let cpu_style = if proc.cpu_pct > 50.0 {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else if proc.cpu_pct > 15.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };

            let vram_style = if proc.vram_mb() > 500.0 {
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::LightMagenta)
            };

            let cells = vec![
                Span::styled(format!("{}", proc.pid), Style::default().fg(Color::Cyan)),
                Span::raw(&proc.name),
                Span::styled(format!("{:.1}%", proc.cpu_pct), cpu_style),
                Span::styled(format!("{:.1}", proc.memory_mb()), Style::default().fg(Color::LightCyan)),
                Span::styled(format!("{:.1}", proc.total_disk_io_kb_sec()), Style::default().fg(Color::Gray)),
                Span::styled(format!("{:.1}%", proc.gpu_pct), Style::default().fg(Color::LightYellow)),
                Span::styled(format!("{:.1}", proc.vram_mb()), vram_style),
            ];

            Row::new(cells)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(32),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Length(12),
            Constraint::Length(14),
        ],
    )
    .header(header_row)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(
                " TOP Recursos do Sistema (Ordenado por {}) ",
                sort_col.as_str().to_uppercase()
            ))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");

    f.render_stateful_widget(table, area, &mut app.top_table_state);
}

fn draw_process_tab(f: &mut Frame, area: Rect, app: &mut App) {
    // Layout com painel de detalhes opcional
    let has_selection = app.filtered_processes.get(app.selected_proc_index).is_some();
    let chunks = if has_selection {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Search bar
                Constraint::Min(8),     // Process table
                Constraint::Length(8),  // Details panel
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(8)])
            .split(area)
    };

    // 1. Barra de Busca Fuzzy
    let is_editing = app.input_mode == InputMode::SearchInput;
    let search_title = if is_editing {
        " 🔍 Buscar Processo (Digitação Ativa - [ESC] para Concluir) "
    } else {
        " 🔍 Buscar Processo (Pressione [/] para Digitar | [Ctrl+C] para Limpar) "
    };

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(if is_editing {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Gray)
        })
        .title(search_title);

    let display_query = if is_editing {
        format!("{}█", app.search_query)
    } else if app.search_query.is_empty() {
        "<Digite um nome ou PID para filtrar...>".to_string()
    } else {
        app.search_query.clone()
    };

    let query_paragraph = Paragraph::new(display_query)
        .style(if app.search_query.is_empty() && !is_editing {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        })
        .block(search_block);

    f.render_widget(query_paragraph, chunks[0]);

    // 2. Tabela de Processos
    let header_cells = [
        "SEL",
        "STATUS",
        "PID",
        "NOME DO PROCESSO",
        "CPU %",
        "MEMÓRIA (MB)",
    ]
    .into_iter()
    .map(|h| Span::styled(h, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
    let header_row = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .filtered_processes
        .iter()
        .map(|proc| {
            let check = if proc.selected {
                Span::styled(" [✓] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [ ] ", Style::default().fg(Color::DarkGray))
            };

            let (status_text, status_style) = if proc.is_protected {
                (
                    "🛡️ CRÍTICO",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )
            } else {
                ("NORMAL", Style::default().fg(Color::Gray))
            };

            let cells = vec![
                check,
                Span::styled(status_text, status_style),
                Span::styled(format!("{}", proc.pid), Style::default().fg(Color::Cyan)),
                Span::raw(&proc.name),
                Span::styled(format!("{:.1}%", proc.cpu_pct), Style::default().fg(Color::White)),
                Span::styled(format!("{:.1}", proc.memory_mb()), Style::default().fg(Color::LightCyan)),
            ];

            let row_style = Style::default();

            Row::new(cells).style(row_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Min(30),
            Constraint::Length(12),
            Constraint::Length(14),
        ],
    )
    .header(header_row)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(
                " Processos em Execução (Encontrados: {}) ",
                app.filtered_processes.len()
            ))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");

    f.render_stateful_widget(table, chunks[1], &mut app.proc_table_state);

    // 3. Painel de Detalhes do Processo Selecionado
    if has_selection && chunks.len() > 2 {
        if let Some(proc) = app.filtered_processes.get(app.selected_proc_index) {
            let details_text = vec![
                Line::from(vec![
                    Span::styled("Path: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(if proc.exe_path.is_empty() { "<não disponível>" } else { &proc.exe_path }),
                ]),
                Line::from(vec![
                    Span::styled("Cmd: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(if proc.cmd_line.is_empty() { "<não disponível>" } else { &proc.cmd_line }),
                ]),
                Line::from(vec![
                    Span::styled("Usuário: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(if proc.user.is_empty() { "<não disponível>" } else { &proc.user }),
                    Span::raw("  |  "),
                    Span::styled("Uptime: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(proc.formatted_uptime()),
                    Span::raw("  |  "),
                    Span::styled("Parent PID: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(proc.parent_pid.map(|p| p.to_string()).unwrap_or_else(|| "-".to_string())),
                ]),
                Line::from(vec![
                    Span::styled("GPU: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{:.1}%", proc.gpu_pct)),
                    Span::raw("  |  "),
                    Span::styled("VRAM: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{:.1} MB", proc.vram_mb())),
                    Span::raw("  |  "),
                    Span::styled("Disco I/O: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{:.1} KB/s", proc.total_disk_io_kb_sec())),
                ]),
            ];

            let details_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Detalhes do Processo Selecionado ")
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

            // Wrap: a linha de comando costuma ser longa (quebra em até 3 linhas no painel)
            let details = Paragraph::new(details_text)
                .block(details_block)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::White));

            f.render_widget(details, chunks[2]);
        }
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(2)])
        .split(area);

    // Linha de Status / Notificação
    if let Some((msg, _)) = &app.status_message {
        let status_p = Paragraph::new(format!(" ℹ️  {}", msg))
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
        f.render_widget(status_p, chunks[0]);
    } else {
        let default_status = Paragraph::new(" Pronto.")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(default_status, chunks[0]);
    }

    // Linha de Atalhos
    let shortcuts = match app.active_tab {
        ActiveTab::Vmware => {
            "[>] Iniciar | [x] Parar | [p] Pausar | [u] Retomar | [z] Suspender | [n] Snapshot | [a] Cadastrar | [r] Atualizar | [Tab] Alternar Aba | [?] Ajuda | [q] Sair"
        }
        ActiveTab::TopMetrics => {
            "[c] Ord. CPU | [m] RAM | [d] Disco | [g] GPU | [v] VRAM | [Tab] Alternar Aba | [?] Ajuda | [q] Sair"
        }
        ActiveTab::ProcessManager => {
            "[/] Buscar | [Espaço] Selecionar | [k/Enter] Matar | [n] Ord. Nome | [C] CPU | [M] Mem | [V] VRAM | [e] CSV | [E] JSON | [Tab] Aba | [?] Ajuda | [q] Sair"
        }
    };

    let footer_p = Paragraph::new(shortcuts)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(20, 25, 35))
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(footer_p, chunks[1]);
}

fn draw_confirm_kill_modal(f: &mut Frame, area: Rect, pids: &[u32], has_critical: bool) {
    let modal_area = centered_rect(60, 40, area);
    f.render_widget(Clear, modal_area);

    let border_color = if has_critical { Color::Red } else { Color::Yellow };

    let mut text = vec![
        Line::from(vec![
            Span::styled(
                format!("Confirma a finalização forçada de {} processo(s)?", pids.len()),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        Line::from(format!("PIDs afetados: {:?}", pids)),
        Line::from(""),
    ];

    if has_critical {
        text.push(Line::from(vec![
            Span::styled(
                "🚨 ALERTA CRÍTICO: Foram selecionados processos protegidos do Windows!",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));
        text.push(Line::from(
            "Finalizar processos vitais pode provocar tela azul ou reinicialização imediata.",
        ));
        text.push(Line::from(""));
    }

    text.push(Line::from(vec![
        Span::styled(" [Y / Enter] SIM, FINALIZAR ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        Span::raw("    "),
        Span::styled(" [N / ESC] CANCELAR ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(border_color))
        .title(" CONFIRMAÇÃO DE FINALIZAÇÃO (TASKKILL) ")
        .title_alignment(Alignment::Center);

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, modal_area);
}

fn draw_snapshot_modal(f: &mut Frame, area: Rect, snapshot_name: &str) {
    let modal_area = centered_rect(50, 25, area);
    f.render_widget(Clear, modal_area);

    let text = vec![
        Line::from("Digite o nome do novo Snapshot para a Máquina Virtual:"),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("Nome: {}█", snapshot_name), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Enter] Criar Snapshot ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            Span::styled(" [Esc] Cancelar ", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" CRIAR SNAPSHOT VMWARE ")
        .title_alignment(Alignment::Center);

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, modal_area);
}

fn draw_add_vm_modal(f: &mut Frame, area: Rect, field: AddVmField, alias: &str, path: &str) {
    let modal_area = centered_rect(60, 30, area);
    f.render_widget(Clear, modal_area);

    let active_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);

    let alias_style = if field == AddVmField::Alias { active_style } else { inactive_style };
    let path_style = if field == AddVmField::Path { active_style } else { inactive_style };

    let alias_cursor = if field == AddVmField::Alias { "█" } else { "" };
    let path_cursor = if field == AddVmField::Path { "█" } else { "" };

    let text = vec![
        Line::from("Cadastrar nova Máquina Virtual (aparece na lista mesmo offline):"),
        Line::from(""),
        Line::from(vec![
            Span::styled(format!("Alias: {}{}", alias, alias_cursor), alias_style),
        ]),
        Line::from(vec![
            Span::styled(format!("Caminho (.vmx): {}{}", path, path_cursor), path_style),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" [Tab] Alternar Campo ", Style::default().fg(Color::Cyan)),
            Span::raw("   "),
            Span::styled(" [Enter] Cadastrar ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("   "),
            Span::styled(" [Esc] Cancelar ", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" CADASTRAR VM ")
        .title_alignment(Alignment::Center);

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, modal_area);
}

fn draw_help_modal(f: &mut Frame, area: Rect) {
    let modal_area = centered_rect(70, 70, area);
    f.render_widget(Clear, modal_area);

    let text = vec![
        Line::from(Span::styled("GUIA RÁPIDO DE TECLAS E ATALHOS", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(Span::styled("Navegação Global:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  1, 2, 3       - Alternar diretamente entre abas"),
        Line::from("  Tab / Shift+Tab - Percorrer abas (Próxima / Anterior)"),
        Line::from("  Setas / j, k  - Navegar verticalmente nas listas de itens"),
        Line::from("  PgUp / PgDown - Rolar listas rapidamente"),
        Line::from("  ?             - Abrir esta tela de ajuda"),
        Line::from("  q / Ctrl+C    - Sair da aplicação"),
        Line::from(""),
        Line::from(Span::styled("Aba 1: VMware Workstation:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  > ou s        - Iniciar VM selecionada (nogui)"),
        Line::from("  x             - Parar VM selecionada (soft shutdown)"),
        Line::from("  p             - Pausar VM"),
        Line::from("  u             - Retomar VM pausada"),
        Line::from("  z             - Suspender VM"),
        Line::from("  n             - Criar novo snapshot"),
        Line::from("  a             - Cadastrar nova VM (alias + caminho .vmx)"),
        Line::from("  r             - Atualizar inventário e status"),
        Line::from(""),
        Line::from(Span::styled("Aba 2: TOP Recursos do Sistema:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  c, m, d, g, v - Ordenar por CPU%, RAM, Disco I/O, GPU% ou VRAM"),
        Line::from(""),
        Line::from(Span::styled("Aba 3: Gerenciador de Processos:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  /             - Entrar no modo de busca e digitação"),
        Line::from("  Espaço        - Marcar / Desmarcar múltiplos processos"),
        Line::from("  k / Delete    - Abrir confirmação para finalizar processo(s)"),
        Line::from("  Ctrl+C        - Limpar texto da busca"),
        Line::from("  n             - Ordenar por nome (alfabético)"),
        Line::from("  C             - Ordenar por CPU% (maior primeiro)"),
        Line::from("  M             - Ordenar por Memória RAM (maior primeiro)"),
        Line::from("  V             - Ordenar por VRAM (maior primeiro)"),
        Line::from("  e             - Exportar lista filtrada para CSV"),
        Line::from("  E             - Exportar lista filtrada para JSON"),
        Line::from("                  (relatórios incluem a linha de comando de cada processo)"),
        Line::from(""),
        Line::from(Span::styled("Pressione [ESC] ou [Enter] para fechar", Style::default().fg(Color::Green))),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" AJUDA & ATALHOS ")
        .title_alignment(Alignment::Center);

    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Left);

    f.render_widget(paragraph, modal_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
