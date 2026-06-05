use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::atomic::Ordering;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Gauge, List, ListItem},
    Terminal,
};

use crate::application::telemetry::Telemetry;
use crate::infrastructure::simulator::{spawn_simulation, SimulationContext};

pub enum AppScreen {
    Setup,
    Simulation,
}

pub struct App {
    pub screen: AppScreen,
    pub input_players: String,
    pub input_threads: String,
    pub active_input: usize,
    pub should_quit: bool,
    pub telemetry: Option<Arc<Telemetry>>,
    pub context: Option<SimulationContext>,
    pub recent_matches: Vec<String>,
    pub start_time: Option<Instant>,
    pub is_finished: bool,
    pub total_elapsed_secs: f64,
    pub last_queued: usize,
    pub last_matches: usize,
    pub current_delta: isize,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: AppScreen::Setup,
            input_players: "10000".to_string(),
            input_threads: "10".to_string(),
            active_input: 0,
            should_quit: false,
            telemetry: None,
            context: None,
            recent_matches: Vec::new(),
            start_time: None,
            is_finished: false,
            total_elapsed_secs: 0.0,
            last_queued: 0,
            last_matches: 0,
            current_delta: 0,
        }
    }
}

pub fn run_tui() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>, app: &mut App) -> io::Result<()> {
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut last_delta_tick = Instant::now();

    loop {
        if app.should_quit {
            return Ok(());
        }

        terminal.draw(|f| ui(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match app.screen {
                    AppScreen::Setup => {
                        match key.code {
                            KeyCode::Char('q') => app.should_quit = true,
                            KeyCode::Tab => app.active_input = (app.active_input + 1) % 2,
                            KeyCode::Enter => {
                                if let (Ok(p), Ok(t)) = (app.input_players.parse::<usize>(), app.input_threads.parse::<usize>()) {
                                    let telemetry = Arc::new(Telemetry::new());
                                    let shutdown_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
                                    let context = spawn_simulation(p, t, telemetry.clone(), shutdown_flag);
                                    
                                    app.telemetry = Some(telemetry);
                                    app.context = Some(context);
                                    app.start_time = Some(Instant::now());
                                    app.screen = AppScreen::Simulation;
                                }
                            }
                            KeyCode::Char(c) => {
                                if c.is_digit(10) {
                                    if app.active_input == 0 {
                                        app.input_players.push(c);
                                    } else {
                                        app.input_threads.push(c);
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                if app.active_input == 0 {
                                    app.input_players.pop();
                                } else {
                                    app.input_threads.pop();
                                }
                            }
                            _ => {}
                        }
                    }
                    AppScreen::Simulation => {
                        if let KeyCode::Char('q') = key.code {
                            app.should_quit = true;
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
            if let AppScreen::Simulation = app.screen {
                if let Some(telemetry) = &app.telemetry {
                    if last_delta_tick.elapsed().as_secs() >= 1 {
                        let q = telemetry.players_queued.load(Ordering::Relaxed);
                        let m = telemetry.matches_formed.load(Ordering::Relaxed);
                        let diff_q = q.saturating_sub(app.last_queued);
                        let diff_m = (m.saturating_sub(app.last_matches)) * 10;
                        app.current_delta = (diff_q as isize) - (diff_m as isize);
                        app.last_queued = q;
                        app.last_matches = m;
                        last_delta_tick = Instant::now();
                    }
                }

                if let Some(ctx) = &app.context {
                    if !app.is_finished && ctx.engine_handle.is_finished() {
                        app.is_finished = true;
                        app.total_elapsed_secs = app.start_time.unwrap_or(Instant::now()).elapsed().as_secs_f64();
                    }

                    while let Ok(formed_match) = ctx.match_receiver.try_recv() {
                        let team_a_avg = formed_match.team_a.iter().map(|p| p.mmr).sum::<f64>() / 5.0;
                        let team_b_avg = formed_match.team_b.iter().map(|p| p.mmr).sum::<f64>() / 5.0;
                        let info = format!("Team A: {:.1} | Team B: {:.1} | Cost: {:.3}", team_a_avg, team_b_avg, formed_match.cost);
                        app.recent_matches.insert(0, info);
                        if app.recent_matches.len() > 50 {
                            app.recent_matches.truncate(50);
                        }
                    }
                }
            }
        }
    }
}

fn ui(f: &mut ratatui::Frame, app: &App) {
    match app.screen {
        AppScreen::Setup => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(5)
                .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)])
                .split(f.area());

            let p_style = if app.active_input == 0 { Style::default().fg(Color::Cyan) } else { Style::default() };
            let t_style = if app.active_input == 1 { Style::default().fg(Color::Cyan) } else { Style::default() };

            let title = Paragraph::new("=== 5v5 Matchmaking Engine Setup ===")
                .style(Style::default().add_modifier(Modifier::BOLD));
            f.render_widget(title, chunks[0]);

            let p_input = Paragraph::new(format!("Total Players: {}", app.input_players))
                .block(Block::default().borders(Borders::ALL).title("Total Players"))
                .style(p_style);
            f.render_widget(p_input, chunks[1]);

            let t_input = Paragraph::new(format!("Ingress Threads: {}", app.input_threads))
                .block(Block::default().borders(Borders::ALL).title("Ingress Threads"))
                .style(t_style);
            f.render_widget(t_input, chunks[2]);
            
            let help = Paragraph::new("Press <Tab> to switch inputs, <Enter> to start, <q> to quit.");
            f.render_widget(help, chunks[3]);
        }
        AppScreen::Simulation => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(10),
                ])
                .split(f.area());

            let header_status = if app.is_finished { "FINISHED" } else { "Running" };
            let mut header = Paragraph::new(format!(
                " 5v5 Matchmaking Simulation {} | Players: {} | Threads: {} ",
                header_status, app.input_players, app.input_threads
            ))
            .block(Block::default().borders(Borders::ALL));
            
            if app.is_finished {
                header = header.style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
            } else {
                header = header.style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
            }
            f.render_widget(header, chunks[0]);

            if let Some(telemetry) = &app.telemetry {
                let queued = telemetry.players_queued.load(Ordering::Relaxed);
                let matches = telemetry.matches_formed.load(Ordering::Relaxed);
                let active = telemetry.active_players.load(Ordering::Relaxed);
                
                let target_players = app.input_players.parse::<usize>().unwrap_or(10000);
                
                let metrics_text = format!(
                    " Total Queued: {}/{} | Active in Queue: {} | Matches Formed: {} ({} players) ",
                    queued, target_players, active, matches, matches * 10
                );
                
                let metrics_p = Paragraph::new(metrics_text)
                    .block(Block::default().borders(Borders::ALL).title("Metrics"))
                    .style(Style::default().fg(Color::Cyan));
                f.render_widget(metrics_p, chunks[1]);
                
                let bottom_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[2]);
                    
                let mut p50 = 0;
                let mut p90 = 0;
                let mut p99 = 0;
                let mut max = 0;
                
                if let Ok(mut lock) = telemetry.wait_times_ms.try_lock() {
                    if !lock.is_empty() {
                        lock.sort_unstable();
                        let n = lock.len() as f64;
                        p50 = lock[(n * 0.50).floor() as usize];
                        p90 = lock[(n * 0.90).floor() as usize];
                        p99 = lock[(n * 0.99).floor() as usize];
                        max = *lock.last().unwrap();
                    }
                }
                
                let left_chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(10), Constraint::Min(8)])
                    .split(bottom_chunks[0]);

                let block = Block::default().title("Wait Time Percentiles (Log Scale)").borders(Borders::ALL);
                let inner_area = block.inner(left_chunks[0]);
                f.render_widget(block, left_chunks[0]);

                let percentiles_chunk = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(2),
                        Constraint::Length(2),
                        Constraint::Length(2),
                        Constraint::Length(2),
                        Constraint::Min(0),
                    ])
                    .split(inner_area);

                let render_gauge = |f: &mut ratatui::Frame, val: u64, max_val: u64, title: &str, chunk, color| {
                    let ratio = if max_val == 0 { 0.0 } else { ((val as f64) + 1.0).log10() / ((max_val as f64) + 1.0).log10() };
                    let gauge = Gauge::default()
                        .block(Block::default().title(title))
                        .gauge_style(Style::default().fg(color))
                        .ratio(ratio.clamp(0.0, 1.0))
                        .label(ratatui::text::Span::styled(format!("{}ms", val), Style::default().fg(Color::Black)));
                    f.render_widget(gauge, chunk);
                };

                render_gauge(f, p50, max, "p50", percentiles_chunk[0], Color::Green);
                render_gauge(f, p90, max, "p90", percentiles_chunk[1], Color::Yellow);
                render_gauge(f, p99, max, "p99", percentiles_chunk[2], Color::LightRed);
                render_gauge(f, max, max, "Max", percentiles_chunk[3], Color::Red);

                let active_solos = telemetry.active_solos.load(Ordering::Relaxed);
                let active_duos = telemetry.active_duos.load(Ordering::Relaxed);
                let active_trios = telemetry.active_trios.load(Ordering::Relaxed);
                
                let cumulative_cost = *telemetry.cumulative_match_cost.lock().unwrap();
                let oldest_age_ms = telemetry.oldest_ticket_age_ms.load(Ordering::Relaxed);
                
                
                let total_wait_ms = telemetry.total_wait_time_ms.load(Ordering::Relaxed);
                let total_wait_hours = (total_wait_ms as f64) / 1000.0 / 3600.0;
                
                let active_buckets = telemetry.active_buckets.load(Ordering::Relaxed);
                let min_mmr = *telemetry.min_active_mmr.lock().unwrap();
                let max_mmr = *telemetry.max_active_mmr.lock().unwrap();
                let min_mmr_str = if min_mmr == f64::MAX { "N/A".to_string() } else { format!("{:.0}", min_mmr) };
                let max_mmr_str = if max_mmr == f64::MIN { "N/A".to_string() } else { format!("{:.0}", max_mmr) };
                
                let tension = telemetry.tension_stats.lock().unwrap();
                let avg_delta = if matches > 0 { tension.sum_delta / (matches as f64) } else { 0.0 };
                let min_delta = if matches > 0 && tension.min_delta != f64::MAX { tension.min_delta } else { 0.0 };
                let max_delta = if matches > 0 && tension.max_delta != f64::MIN { tension.max_delta } else { 0.0 };

                let avg_variance = if matches > 0 { tension.sum_variance / (matches as f64) } else { 0.0 };
                let min_variance = if matches > 0 && tension.min_variance != f64::MAX { tension.min_variance } else { 0.0 };
                let max_variance = if matches > 0 && tension.max_variance != f64::MIN { tension.max_variance } else { 0.0 };

                let avg_radius = if matches > 0 { (tension.sum_radius as f64) / (matches as f64) } else { 0.0 };
                let min_radius = if matches > 0 && tension.min_radius != u64::MAX { tension.min_radius } else { 0 };
                let max_radius = if matches > 0 { tension.max_radius } else { 0 };
                drop(tension);

                let tick_us = telemetry.total_tick_micros.load(Ordering::Relaxed);
                let tick_count = telemetry.tick_count.load(Ordering::Relaxed);
                let avg_tick_us = if tick_count > 0 { tick_us / (tick_count as u64) } else { 0 };

                let avg_cost = if matches > 0 { cumulative_cost / (matches as f64) } else { 0.0 };
                
                let elapsed_secs = if app.is_finished {
                    app.total_elapsed_secs
                } else {
                    app.start_time.unwrap_or(Instant::now()).elapsed().as_secs_f64()
                };
                let mps = if elapsed_secs > 0.0 { (matches as f64) / elapsed_secs } else { 0.0 };

                let delta_str = if app.current_delta >= 0 { format!("+{}", app.current_delta) } else { format!("{}", app.current_delta) };

                let advanced_metrics_text = format!(
                    "\n Matches / Sec: {:.2} | Queue Delta: {}\n CPU Tick Overhead: {} μs\n\n QUEUE HEALTH (Total Wait Debt: {:.1} hrs)\n  Active Buckets: {} (Fragmentation)\n  MMR Bounds: {} -> {}\n\n MATCH TENSION (Cost {:.2})\n  Delta      - Avg: {:.2} | Min: {:.2} | Max: {:.2}\n  Variance   - Avg: {:.2} | Min: {:.2} | Max: {:.2}\n  Radius Ext - Avg: {:.2} | Min: {} | Max: {}\n\n DEMOGRAPHICS (Oldest: {}ms)\n  Solos: {} | Duos: {} | Trios: {}",
                    mps, delta_str, avg_tick_us, total_wait_hours, active_buckets, min_mmr_str, max_mmr_str,
                    avg_cost, 
                    avg_delta, min_delta, max_delta, 
                    avg_variance, min_variance, max_variance, 
                    avg_radius, min_radius, max_radius, 
                    oldest_age_ms, active_solos, active_duos, active_trios
                );

                let advanced_p = Paragraph::new(advanced_metrics_text)
                    .block(Block::default().borders(Borders::ALL).title("Advanced Engine Telemetry"))
                    .style(Style::default().fg(Color::Magenta));
                
                f.render_widget(advanced_p, left_chunks[1]);
                
                let items: Vec<ListItem> = app.recent_matches
                    .iter()
                    .map(|m| ListItem::new(m.as_str()))
                    .collect();
                    
                let list = List::new(items)
                    .block(Block::default().title("Recent Matches").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White));
                f.render_widget(list, bottom_chunks[1]);
            }
        }
    }
}
