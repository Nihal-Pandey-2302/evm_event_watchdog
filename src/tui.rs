use std::sync::Arc;
use std::time::{Duration, Instant};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier},
    widgets::{Block, Borders, Paragraph, BarChart, Table, Row, Cell},
    Terminal,
};
use crate::state::AppState;
use crate::events::Severity;
use eyre::Result;

pub fn run_tui(state: Arc<AppState>) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, state);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    state: Arc<AppState>,
) -> std::io::Result<()> {
    let start_time = Instant::now();
    
    let mut current_filter_index = 0; // 0 = All

    loop {
        terminal.draw(|f| {
            // ... (Layout remains same) ...
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3), // Header
                        Constraint::Length(10), // Report
                        Constraint::Min(10),   // Table
                    ]
                    .as_ref(),
                )
                .split(f.size());

            // --- Chain Filtering Logic ---
            let mut active_chains: Vec<String> = vec!["ALL".to_string()];
            if let Ok(heights) = state.chain_heights.lock() {
                let mut chains: Vec<String> = heights.keys().cloned().collect();
                chains.sort();
                active_chains.extend(chains);
            }
            // Ensure index is valid
            if current_filter_index >= active_chains.len() {
                current_filter_index = 0;
            }
            let selected_filter = &active_chains[current_filter_index];


            // --- Header ---
            let block_info = if let Ok(heights) = state.chain_heights.lock() {
                if heights.is_empty() {
                    "No Chains Active".to_string()
                } else {
                    heights.iter()
                        .map(|(k, v)| format!("{}: #{}", k, v))
                        .collect::<Vec<String>>()
                        .join(" | ")
                }
            } else {
                "State Locked".to_string()
            };

            let uptime = start_time.elapsed().as_secs();
            
            let header_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(25), Constraint::Percentage(25)])
                .split(chunks[0]);

            let block_widget = Paragraph::new(format!("BLOCKS: {}", block_info))
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL));
            
            let uptime_widget = Paragraph::new(format!("UPTIME: {}s", uptime))
                 .style(Style::default().fg(Color::White))
                 .block(Block::default().borders(Borders::ALL));

            // Status now shows Filter
            let filter_text = format!("FILTER: [{}] (Tab)", selected_filter);
            let status_widget = Paragraph::new(filter_text)
                 .style(Style::default().fg(if selected_filter == "ALL" { Color::Green } else { Color::Yellow }).add_modifier(Modifier::BOLD))
                 .block(Block::default().title(" Status ").borders(Borders::ALL));

            f.render_widget(block_widget, header_layout[0]);
            f.render_widget(uptime_widget, header_layout[1]);
            f.render_widget(status_widget, header_layout[2]);

            // --- Middle Section (Bar Chart & Health) ---
             let mid_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
                .split(chunks[1]);

            // Summary Bar Chart
            let counts = state.severity_counts.lock().unwrap();
            let crit = *counts.get(&Severity::Critical).unwrap_or(&0);
            let high = *counts.get(&Severity::High).unwrap_or(&0);
            let med = *counts.get(&Severity::Medium).unwrap_or(&0);
            let low = *counts.get(&Severity::Low).unwrap_or(&0);

            // BarChart requires (label, u64) tuples.
            // Using a simple BarChart from ratatui
            let data = [
                ("Critical", crit),
                ("High", high),
                ("Medium", med),
                ("Low", low),
            ];
            
            let bar_chart = BarChart::default()
                .block(Block::default().title(" Risk Distribution ").borders(Borders::ALL))
                .data(&data)
                .bar_width(10)
                .bar_style(Style::default().fg(Color::Yellow))
                .value_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
            
            f.render_widget(bar_chart, mid_chunks[0]);

            // Health Panel
            let last_block_time = state.last_block_time.lock().unwrap();
            let block_age = last_block_time.elapsed().as_secs();
            let health_style = if block_age < 15 { Style::default().fg(Color::Green) } else { Style::default().fg(Color::Red) };
            
            let health_text = vec![
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw("Last Block: "),
                    ratatui::text::Span::styled(format!("{}s ago", block_age), health_style.add_modifier(Modifier::BOLD))
                ]),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw("Rate Limiting: "),
                    ratatui::text::Span::styled("Active", Style::default().fg(Color::Cyan))
                ]),
                 ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw("Mode: "),
                    ratatui::text::Span::styled("Live Monitoring", Style::default().fg(Color::Magenta))
                ]),
            ];
            
            let health_p = Paragraph::new(health_text)
                .block(Block::default().title(" System Health ").borders(Borders::ALL));
            f.render_widget(health_p, mid_chunks[1]);


            // --- Footer (Recent Alerts Table) ---
            let history = state.alert_history.lock().unwrap();
            // Upgraded headers to include Chain
            let headers = Row::new(vec!["CHAIN", "SEVERITY", "TIME AGO", "MESSAGE"])
                .style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))
                .bottom_margin(1);
            
            let rows: Vec<Row> = history.iter()
                .rev()
                .filter(|(sev, _, _, _, _)| *sev != Severity::Low) // Filter Low severity
                .filter(|(_, chain, _, _, _)| selected_filter == "ALL" || chain == selected_filter) // CHAIN FILTER
                .take(15) // Strict Cap
                .map(|(sev, chain, msg, time, count)| {
                    let age = time.elapsed().as_secs();
                    let color = match sev {
                        Severity::Critical => Color::Red,
                        Severity::High => Color::LightRed,
                        Severity::Medium => Color::Yellow,
                        _ => Color::Blue,
                    };
                    
                    let mut display_msg = msg.clone();
                    if display_msg.len() > 50 {
                        display_msg.truncate(47);
                        display_msg.push_str("...");
                    }
                    
                    if *count > 1 {
                        display_msg = format!("{} (x{})", display_msg, count);
                    }

                    Row::new(vec![
                        Cell::from(chain.clone()).style(Style::default().fg(Color::Cyan)),
                        Cell::from(format!("{:?}", sev)).style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
                        Cell::from(format!("{}s", age)).style(Style::default().fg(Color::DarkGray)),
                        Cell::from(display_msg),
                    ])
                }).collect();
            
            let table = Table::new(rows, [
                Constraint::Length(10), // Chain
                Constraint::Length(12), // Severity
                Constraint::Length(10), // Time
                Constraint::Fill(1),    // Message
            ])
            .header(headers)
            .block(Block::default().title(" Recent Alerts ").borders(Borders::ALL))
            .column_spacing(2);
            
             f.render_widget(table, chunks[2]);

        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Tab => {
                        current_filter_index += 1; // Cycle
                    }
                    _ => {}
                }
            }
        }
    }
}
