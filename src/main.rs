mod app;
mod db;
mod event;
mod fees;
mod model;
mod pnl;
mod poly_api;
mod replay;
mod strategies;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;

const USERS_FILE: &str = "users.txt";

fn load_user_addresses() -> Vec<String> {
    std::fs::read_to_string(USERS_FILE)
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let user_addresses = load_user_addresses();
    let host = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("QUESTDB_HOST").ok())
        .unwrap_or_else(|| "localhost".to_string());
    let db = db::connect(&host).await?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(user_addresses);

    let tick_rate = Duration::from_millis(50);
    let mut last_tick = Instant::now();

    loop {
        // Refresh markets if needed
        if app.needs_refresh {
            app.needs_refresh = false;
            match db::fetch_markets(
                &db,
                app.crypto_filter.as_deref(),
                app.date_from,
                app.date_to,
            )
            .await
            {
                Ok(markets) => {
                    app.markets = markets;
                    if !app.markets.is_empty() {
                        app.market_list_state.select(Some(0));
                    } else {
                        app.market_list_state.select(None);
                    }
                    app.status_message = format!("{} markets found", app.markets.len());
                }
                Err(e) => {
                    app.status_message = format!("DB error: {e}");
                }
            }
        }

        terminal.draw(|f| ui::render(f, &mut app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = crossterm::event::read()? {
                if event::handle_key(&mut app, key, &db).await {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
