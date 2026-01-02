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
    
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints(
                    [
                        Constraint::Length(3), // Header
                        Constraint::Length(10), // Summary & Health
                        Constraint::Min(10),   // Recent Alerts Table
                    ]
                    .as_ref(),
                )
                .split(f.size());

            // --- Header ---
            let block_num = state.last_block.load(std::sync::atomic::Ordering::Relaxed);
            let uptime = start_time.elapsed().as_secs();
            
            let header_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(33)])
                .split(chunks[0]);

            let block_widget = Paragraph::new(format!("BLOCK: #{}", block_num))
                .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
                .block(Block::default().borders(Borders::ALL));
            
            let uptime_widget = Paragraph::new(format!("UPTIME: {}s", uptime))
                 .style(Style::default().fg(Color::White))
                 .block(Block::default().borders(Borders::ALL));

            let status_widget = Paragraph::new("STATUS: CONNECTED")
                 .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
                 .block(Block::default().borders(Borders::ALL));

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
            let headers = Row::new(vec!["SEVERITY", "TIME AGO", "MESSAGE"])
                .style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))
                .bottom_margin(1);
            
            let rows: Vec<Row> = history.iter()
                .rev()
                .filter(|(sev, _, _, _)| *sev != Severity::Low) // Filter Low severity
                .take(15) // Strict Cap
                .map(|(sev, msg, time, count)| {
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
                        Cell::from(format!("{:?}", sev)).style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
                        Cell::from(format!("{}s", age)).style(Style::default().fg(Color::DarkGray)),
                        Cell::from(display_msg),
                    ])
                }).collect();
            
            let table = Table::new(rows, [
                Constraint::Length(12), // Severity
                Constraint::Length(10), // Time
                Constraint::Fill(1),    // Message (Fills remaining space, but truncated input prevents overflow)
            ])
            .header(headers)
            .block(Block::default().title(" Recent Alerts ").borders(Borders::ALL))
            .column_spacing(2);
            
             f.render_widget(table, chunks[2]);

        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    return Ok(());
                }
            }
        }
    }
}
