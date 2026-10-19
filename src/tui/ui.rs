use super::app::{ActiveTab, AddVmField, App, InputMode, ModalState, ProcSort, SupportSection};
use crate::modules::net::NetSort;
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
        ActiveTab::Network => draw_network_tab(f, chunks[1], app),
        ActiveTab::Files => super::fm_ui::draw_files_tab(f, chunks[1], &mut app.fm),
        ActiveTab::Support => draw_support_tab(f, chunks[1], app),
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
        Span::styled(" [1] 🖥️  VMware ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" [2] 📊 TOP ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" [3] 🎯 Processos ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" [4] 🌐 Rede ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" [5] 📁 Arquivos ", Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(" [6] 🧰 Suporte ", Style::default().add_modifier(Modifier::BOLD)),
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

    // 2. Tabela de Processos (marca ▼ na coluna da ordenação ativa)
    let proc_sort = app.proc_sort;

    let proc_col_label = |name: &str, field: ProcSort| -> Span<'static> {
        if proc_sort == field {
            Span::styled(
                format!("{} ▼", name),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
        } else {
            Span::styled(
                name.to_string(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )
        }
    };

    let header_cells = vec![
        Span::styled("SEL", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("STATUS", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("PID", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        proc_col_label("NOME DO PROCESSO", ProcSort::Name),
        proc_col_label("CPU %", ProcSort::Cpu),
        proc_col_label("MEMÓRIA (MB)", ProcSort::Ram),
        proc_col_label("VRAM (MB)", ProcSort::Vram),
        Span::styled("LINHA DE COMANDO", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ];
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

            let vram_style = if proc.vram_bytes > 500 * 1024 * 1024 {
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::LightMagenta)
            };

            let cells = vec![
                check,
                Span::styled(status_text, status_style),
                Span::styled(format!("{}", proc.pid), Style::default().fg(Color::Cyan)),
                Span::raw(&proc.name),
                Span::styled(format!("{:.1}%", proc.cpu_pct), Style::default().fg(Color::White)),
                Span::styled(format!("{:.1}", proc.memory_mb()), Style::default().fg(Color::LightCyan)),
                Span::styled(format!("{:.1}", proc.vram_mb()), vram_style),
                // Linha completa do processo. Vazia = sem permissão para ler (outro usuário/sistema)
                if proc.cmd_line.is_empty() {
                    Span::styled("— (sem permissão; use Administrador)", Style::default().fg(Color::DarkGray))
                } else {
                    Span::styled(proc.cmd_line.as_str(), Style::default().fg(Color::Gray))
                },
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
            Constraint::Length(10),
            Constraint::Min(28),
            Constraint::Length(10),
            Constraint::Length(14),
            Constraint::Length(14),
            Constraint::Min(30),
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
                    Span::styled("Cmd: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::raw(if proc.cmd_line.is_empty() { "<não disponível (execute como Administrador)>" } else { &proc.cmd_line }),
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

            let details = Paragraph::new(details_text)
                .block(details_block)
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(Color::White));

            f.render_widget(details, chunks[2]);
        }
    }
}

fn draw_network_tab(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Search bar
            Constraint::Length(3), // Filtros ativos
            Constraint::Min(8),    // Tabela de sockets
        ])
        .split(area);

    // 1. Barra de busca (porta, processo, endereço, estado)
    let is_editing = app.input_mode == InputMode::SearchInput && app.active_tab == ActiveTab::Network;
    let search_title = if is_editing {
        " 🔍 Buscar Porta/Processo (Digitação Ativa - [ESC] para Concluir) "
    } else {
        " 🔍 Buscar Porta/Processo (Pressione [/] para Digitar | [Ctrl+C] para Limpar) "
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
        format!("{}█", app.net_query)
    } else if app.net_query.is_empty() {
        "<Digite porta, PID, nome do processo, endereço ou estado...>".to_string()
    } else {
        app.net_query.clone()
    };

    let query_paragraph = Paragraph::new(display_query)
        .style(if app.net_query.is_empty() && !is_editing {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        })
        .block(search_block);

    f.render_widget(query_paragraph, chunks[0]);

    // 2. Linha de filtros ativos
    let filter_line = vec![Line::from(vec![
        Span::styled("Protocolo: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("[{}] (tecla p alterna)", app.net_proto_filter.label()),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::styled("Escuta: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(
            if app.only_listening { "SOMENTE LISTENING (l)" } else { "TODAS (l)" },
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  |  "),
        Span::styled("Sockets: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(format!("{}", app.filtered_net.len())),
    ])];

    let filter_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Gray))
        .title(" Filtros ");

    f.render_widget(Paragraph::new(filter_line).block(filter_block), chunks[1]);

    // 3. Tabela de sockets (marca ▼ na coluna da ordenação ativa)
    let net_sort = app.net_sort;

    let net_col_label = |name: &str, field: NetSort| -> Span<'static> {
        if net_sort == field {
            Span::styled(
                format!("{} ▼", name),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )
        } else {
            Span::styled(
                name.to_string(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )
        }
    };

    let header_cells = vec![
        Span::styled("PROTO", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        net_col_label("PORTA LOCAL", NetSort::Port),
        Span::styled("ENDEREÇO LOCAL", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("REMOTO", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        net_col_label("ESTADO", NetSort::State),
        net_col_label("PID", NetSort::Pid),
        net_col_label("PROCESSO", NetSort::Name),
        Span::styled("STATUS", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ];
    let header_row = Row::new(header_cells).height(1).bottom_margin(1);

    let rows: Vec<Row> = app
        .filtered_net
        .iter()
        .map(|conn| {
            let (status_text, status_style) = if conn.is_protected {
                (
                    "🛡️ CRÍTICO",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )
            } else {
                ("NORMAL", Style::default().fg(Color::Gray))
            };

            let state_style = match conn.state_label() {
                "LISTENING" => Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                "ESTABLISHED" => Style::default().fg(Color::Cyan),
                "—" => Style::default().fg(Color::DarkGray),
                _ => Style::default().fg(Color::Yellow),
            };

            let cells = vec![
                Span::styled(conn.proto_label(), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}", conn.local_port), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(conn.local_ip.clone()),
                Span::styled(conn.remote_endpoint(), Style::default().fg(Color::DarkGray)),
                Span::styled(conn.state_label(), state_style),
                Span::styled(format!("{}", conn.pid), Style::default().fg(Color::Cyan)),
                Span::raw(conn.process_name.clone()),
                Span::styled(status_text, status_style),
            ];

            Row::new(cells).style(Style::default())
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(14),
            Constraint::Min(18),
            Constraint::Min(18),
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Min(22),
            Constraint::Length(14),
        ],
    )
    .header(header_row)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" Portas Ativas e Processos (Sockets: {}) ", app.filtered_net.len()))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");

    f.render_stateful_widget(table, chunks[2], &mut app.net_table_state);
}

fn support_section_bar(app: &App) -> Vec<Span<'static>> {
    let sections = [
        SupportSection::Ping,
        SupportSection::Lan,
        SupportSection::Disk,
        SupportSection::Boot,
        SupportSection::Notes,
    ];
    let mut spans = vec![Span::styled(
        " Seção: ",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )];
    for s in sections {
        let active = s == app.support_section;
        spans.push(Span::styled(
            format!(" [{}]{} ", if active { "●" } else { "○" }, s.label()),
            if active {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ));
    }
    spans.push(Span::styled(
        "  ([ ] alterna)",
        Style::default().fg(Color::DarkGray),
    ));
    spans
}

fn sparkline(values: &[u32]) -> String {
    const BARS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];
    if values.is_empty() {
        return "<sem medições — pressione [r]>".to_string();
    }
    let max = (*values.iter().max().unwrap()).max(1);
    values
        .iter()
        .map(|v| BARS[((*v as usize * 7) / max as usize).min(7)])
        .collect()
}

fn draw_support_tab(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Seletor de seção
            Constraint::Min(8),    // Conteúdo
            Constraint::Length(5), // Detalhe / ajuda contextual
        ])
        .split(area);

    // 1. Seletor de seção
    let bar_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Gray))
        .title(" Suporte Técnico ");
    f.render_widget(
        Paragraph::new(Line::from(support_section_bar(app))).block(bar_block),
        chunks[0],
    );

    // 2. Conteúdo da seção
    match app.support_section {
        SupportSection::Ping => draw_support_ping(f, chunks[1], app),
        SupportSection::Lan => draw_support_lan(f, chunks[1], app),
        SupportSection::Disk => draw_support_disk(f, chunks[1], app),
        SupportSection::Boot => draw_support_boot(f, chunks[1], app),
        SupportSection::Notes => draw_support_notes(f, chunks[1], app),
    }

    // 3. Ajuda contextual
    let help = match app.support_section {
        SupportSection::Ping => "[/] edita host | [r/Enter] roda série | [e] exporta CSV de evidência",
        SupportSection::Lan => "[r] revarre tabela ARP da LAN",
        SupportSection::Disk => "[/] edita caminho | [r/Enter] calcula em segundo plano | [Esc] cancela",
        SupportSection::Boot => "[r] recarrega Run keys + tarefas agendadas",
        SupportSection::Notes => "[/] busca | [v/Enter] lê nota | criação e edição pelo CLI `notes`",
    };
    let help_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));
    f.render_widget(Paragraph::new(help).style(Style::default().fg(Color::Gray)).block(help_block), chunks[2]);
}

fn draw_support_ping(f: &mut Frame, area: Rect, app: &mut App) {
    let is_editing = app.input_mode == InputMode::SearchInput;
    let host_line = Line::from(vec![
        Span::styled("Host: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(
            if is_editing { format!("{}█", app.ping_host) } else { app.ping_host.clone() },
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ]);
    let stats_line = match &app.ping_stats {
        Some(s) => Line::from(vec![
            Span::styled("Última série: ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(s.summary()),
        ]),
        None => Line::from(Span::styled(
            "Nenhuma medição ainda.",
            Style::default().fg(Color::DarkGray),
        )),
    };
    let spark_line = Line::from(vec![
        Span::styled("Histórico (avg ms): ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(sparkline(&app.ping_history), Style::default().fg(Color::Cyan)),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Latência / Ping ")
        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    f.render_widget(Paragraph::new(vec![host_line, Line::from(""), stats_line, spark_line]).block(block), area);
}

fn draw_support_lan(f: &mut Frame, area: Rect, app: &mut App) {
    let header = Row::new(vec![
        Span::styled("IP", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("MAC", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("TIPO", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ])
    .height(1)
    .bottom_margin(1);
    let rows: Vec<Row> = app
        .arp_entries
        .iter()
        .map(|e| {
            Row::new(vec![
                Span::styled(e.ip.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(e.mac.clone()),
                Span::raw(crate::modules::support::arp_type_label(e.entry_type).to_string()),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Length(20), Constraint::Length(22), Constraint::Min(10)],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" Mapa da LAN — ARP ({} hosts) ", app.arp_entries.len()))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");
    f.render_stateful_widget(table, area, &mut app.support_table_state);
}

fn draw_support_disk(f: &mut Frame, area: Rect, app: &mut App) {
    use crate::modules::support::bytes_to_human;

    // Largura fixa da barra proporcional (em caracteres)
    const LARGURA_BARRA: usize = 24;

    let is_editing = app.input_mode == InputMode::SearchInput;
    let estado = match (&app.disk_scan, app.disk_entries.is_empty()) {
        (Some(scan), _) => format!("⏳ Calculando... {}s ([Esc] cancela)", scan.started.elapsed().as_secs()),
        (None, true) => "Pressione [r] para calcular.".to_string(),
        (None, false) => "Pronto. [r] recalcula.".to_string(),
    };
    let path_line = format!(
        "Caminho: {}   ([/] edita | [r] calcula) — {}",
        if is_editing { format!("{}█", app.disk_path) } else { app.disk_path.clone() },
        estado
    );

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(4)])
        .split(area);

    let path_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Gray));
    f.render_widget(
        Paragraph::new(path_line).style(Style::default().fg(Color::White)).block(path_block),
        inner[0],
    );

    // Ocupação da unidade que contém o caminho (gráfico de barra horizontal)
    match &app.disk_volume {
        Some(v) if v.total > 0 => {
            let usado = v.total.saturating_sub(v.free);
            let pct = (((usado as f64 / v.total as f64) * 100.0).round() as u16).min(100);
            let cor = if pct >= 90 {
                Color::Red
            } else if pct >= 75 {
                Color::Yellow
            } else {
                Color::Green
            };
            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(format!(" Unidade {} ", v.mount))
                        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                )
                .gauge_style(Style::default().fg(cor).bg(Color::Rgb(35, 40, 50)))
                .percent(pct)
                .label(format!(
                    "{} usados de {} ({}%) — {} livres",
                    bytes_to_human(usado),
                    bytes_to_human(v.total),
                    pct,
                    bytes_to_human(v.free)
                ));
            f.render_widget(gauge, inner[1]);
        }
        _ => {
            let aviso = Paragraph::new("Ocupação da unidade: aparece depois de calcular ([r]).")
                .style(Style::default().fg(Color::DarkGray))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .title(" Unidade ")
                        .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                );
            f.render_widget(aviso, inner[1]);
        }
    }

    // Barras por pasta, proporcionais à maior. Cor pela fatia na soma listada.
    let maior = app.disk_entries.iter().map(|e| e.bytes).max().unwrap_or(0).max(1);
    let soma: u64 = app.disk_entries.iter().map(|e| e.bytes).sum();
    let total_volume = app.disk_volume.as_ref().map(|v| v.total).unwrap_or(0);

    let header = Row::new(vec![
        Span::styled("TAMANHO", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("PROPORÇÃO", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("% LISTA", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("% UNIDADE", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("PASTA", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ])
    .height(1)
    .bottom_margin(1);

    let rows: Vec<Row> = app
        .disk_entries
        .iter()
        .map(|e| {
            let barra = crate::texto::barra(e.bytes, maior, LARGURA_BARRA);
            let fatia = e.bytes as f64 / soma.max(1) as f64;
            let cor = if fatia >= 0.40 {
                Color::Red
            } else if fatia >= 0.15 {
                Color::Yellow
            } else {
                Color::Green
            };
            let pct_unidade = if total_volume > 0 {
                format!("{:.1}%", e.bytes as f64 / total_volume as f64 * 100.0)
            } else {
                "—".to_string()
            };
            Row::new(vec![
                Span::styled(
                    bytes_to_human(e.bytes),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ),
                Span::styled(barra, Style::default().fg(cor)),
                Span::raw(format!("{:.1}%", fatia * 100.0)),
                Span::raw(pct_unidade),
                Span::raw(e.path.display().to_string()),
            ])
        })
        .collect();

    let titulo = if app.disk_scan.is_some() {
        " Disco — calculando em segundo plano... ".to_string()
    } else {
        format!(" Disco — Top {} pastas (soma listada: {}) ", app.disk_entries.len(), bytes_to_human(soma))
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length((LARGURA_BARRA + 2) as u16),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Min(20),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(titulo)
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");
    f.render_stateful_widget(table, inner[2], &mut app.support_table_state);
}

fn draw_support_boot(f: &mut Frame, area: Rect, app: &mut App) {
    let header = Row::new(vec![
        Span::styled("ORIGEM", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("NOME", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("COMANDO / STATUS", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ])
    .height(1)
    .bottom_margin(1);
    let rows: Vec<Row> = app
        .boot_items
        .iter()
        .map(|b| {
            Row::new(vec![
                Span::styled(b.source.clone(), Style::default().fg(Color::Magenta)),
                Span::styled(b.name.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(b.command.clone()),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Length(16), Constraint::Length(34), Constraint::Min(30)],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" Boot / Login ({} itens) ", app.boot_items.len()))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");
    f.render_stateful_widget(table, area, &mut app.support_table_state);
}

fn draw_support_notes(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(7)])
        .split(area);

    let is_editing = app.input_mode == InputMode::SearchInput;
    let header = Row::new(vec![
        Span::styled(
            format!(
                "ID {}",
                if is_editing {
                    format!("(busca: {}█)", app.note_query)
                } else if app.note_query.is_empty() {
                    "([/] busca)".to_string()
                } else {
                    format!("(filtro: {})", app.note_query)
                }
            ),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled("TÍTULO", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("TAGS", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
    ])
    .height(1)
    .bottom_margin(1);
    let rows: Vec<Row> = app
        .support_notes
        .iter()
        .map(|n| {
            Row::new(vec![
                Span::styled(n.id.clone(), Style::default().fg(Color::Cyan)),
                Span::raw(n.title.clone()),
                Span::styled(n.tags.join(","), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Length(30), Constraint::Min(24), Constraint::Length(20)],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" Notas de Campo ({} — [v] lê) ", app.support_notes.len()))
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    )
    .row_highlight_style(Style::default().bg(Color::Rgb(30, 45, 60)))
    .highlight_symbol("▶ ");
    f.render_stateful_widget(table, chunks[0], &mut app.support_table_state);

    let body = app.note_body.as_deref().unwrap_or("Selecione uma nota e pressione [v] para ler. Criação pelo CLI: `notes add \"título\" --body ...`.");
    let detail = Paragraph::new(body)
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Leitura "))
        .style(Style::default().fg(Color::White));
    f.render_widget(detail, chunks[1]);
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
            "[c] Ord. CPU | [m] RAM | [d] Disco | [g] GPU | [v] VRAM | [Enter] Gerenciar processo | [Tab] Alternar Aba | [?] Ajuda | [q] Sair"
        }
        ActiveTab::ProcessManager => {
            "[/] Buscar | [Espaço] Selecionar | [k/Enter] Matar | [n] Ord. Nome | [C] CPU | [M] Mem | [V] VRAM | [e] CSV | [E] JSON | [Tab] Aba | [?] Ajuda | [q] Sair"
        }
        ActiveTab::Network => {
            "[/] Buscar | [p] Proto | [l] Listening | [o/n/d/s] Ordenar | [r] Atualizar | [k/Enter] Matar | [e] CSV | [E] JSON | [Tab] Aba | [?] Ajuda | [q] Sair"
        }
        ActiveTab::Files => {
            "[↑↓] Mover | [Enter] Abrir | [Bksp] Subir | [Espaço] Marcar | [y] Copiar | [p] Colar | [n] Nova pasta | [/] Filtro | [:] Ir para | [s] Ordem | [.] Ocultos | [Esc] Cancelar | [q] Sair"
        }
        ActiveTab::Support => {
            "[[ ]] Trocar seção | [r/Enter] Executar | [Esc] Cancelar cálculo | [/] Editar campo | [v] Ler nota | [e] Exportar ping | [Tab] Aba | [?] Ajuda | [q] Sair"
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
                "🛡️ Processos protegidos do Windows foram REMOVIDOS desta lista",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));
        text.push(Line::from(
            "Eles não serão encerrados: finalizar processos vitais pode provocar tela azul.",
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
        Line::from("  1..6         - Alternar diretamente entre abas"),
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
        Line::from("  Enter         - Levar processo selecionado p/ Gerenciador (aba 3) já filtrado"),
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
        Line::from(""),
        Line::from(Span::styled("Aba 4: Rede/Portas:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  /             - Entrar no modo de busca (porta, PID, processo, estado)"),
        Line::from("  p             - Alternar filtro de protocolo (TCP+UDP / TCP / UDP)"),
        Line::from("  l             - Alternar somente sockets em escuta"),
        Line::from("  o, n, d, s   - Ordenar por porta, processo, PID ou estado"),
        Line::from("  r             - Revarrer portas agora"),
        Line::from("  k / Delete    - Abrir confirmação para finalizar o processo dono da porta"),
        Line::from("  Ctrl+C        - Limpar texto da busca"),
        Line::from("  e             - Exportar lista filtrada para CSV"),
        Line::from("  E             - Exportar lista filtrada para JSON"),
        Line::from(""),
        Line::from(Span::styled("Aba 5: Arquivos:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  ↑↓ / j k      - Mover o cursor (PgUp, PgDn, g, G também)"),
        Line::from("  Enter / →     - Entrar na pasta      Backspace / ← - Subir (na raiz, mostra unidades)"),
        Line::from("  :             - Digitar um caminho para ir até ele"),
        Line::from("  /             - Filtrar por texto      . - Mostrar/ocultar itens ocultos"),
        Line::from("  s             - Trocar ordenação (nome, tamanho, data)"),
        Line::from("  Espaço        - Marcar/desmarcar item e descer"),
        Line::from("  y             - Guardar marcados (ou o item do cursor) para copiar"),
        Line::from("  p             - Colar na pasta atual (mostra o plano antes de copiar)"),
        Line::from("  n             - Criar nova pasta na pasta atual"),
        Line::from("  Esc           - Cancelar a cópia em andamento, ou limpar marcas e filtro"),
        Line::from(""),
        Line::from(Span::styled("Aba 6: Suporte Técnico:", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  [ / ]         - Alternar seção (Ping, LAN, Disco, Boot, Notas)"),
        Line::from("  r / Enter     - Executar ação da seção (Disco: calcula em segundo plano)"),
        Line::from("  Esc           - Cancelar o cálculo de disco em andamento"),
        Line::from("  /             - Editar host do ping, caminho do disco ou busca de notas"),
        Line::from("  v             - Ler nota selecionada"),
        Line::from("  e             - Exportar evidência de ping (CSV, seção Latência)"),
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

pub(super) fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
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
