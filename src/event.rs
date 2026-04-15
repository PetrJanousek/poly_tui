use crossterm::event::{KeyCode, KeyEvent};

use crate::app::{App, AppMode};
use crate::db::{self, Db};
use crate::model::MarketData;

/// Returns true if the app should quit.
pub async fn handle_key(app: &mut App, key: KeyEvent, db: &Db) -> bool {
    match app.mode {
        AppMode::MarketBrowser => handle_browser_key(app, key, db).await,
        AppMode::Replay => handle_replay_key(app, key),
    }
}

async fn handle_browser_key(app: &mut App, key: KeyEvent, db: &Db) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return true,

        KeyCode::Down | KeyCode::Char('j') => {
            let len = app.markets.len();
            if len > 0 {
                let i = app.market_list_state.selected().unwrap_or(0);
                app.market_list_state.select(Some((i + 1).min(len - 1)));
            }
        }

        KeyCode::Up | KeyCode::Char('k') => {
            let i = app.market_list_state.selected().unwrap_or(0);
            app.market_list_state.select(Some(i.saturating_sub(1)));
        }

        KeyCode::Enter => {
            if let Some(market) = app.selected_market().cloned() {
                app.status_message = format!("Loading {}...", market.question);

                match load_market_data(db, &market, &app.user_addresses).await {
                    Ok(data) => {
                        app.status_message.clear();
                        app.enter_replay(data);
                    }
                    Err(e) => {
                        app.status_message = format!("Error: {e}");
                    }
                }
            }
        }

        // Crypto filters: 1=btc, 2=eth, 3=xrp, 4=sol, 5=hype, 6=doge, 7=bnb
        KeyCode::Char(c @ '1'..='7') => {
            let idx = (c as u8 - b'1') as usize;
            app.set_crypto_filter(idx);
        }

        KeyCode::Char('a') => {
            app.clear_filter();
        }

        // Date navigation
        KeyCode::Char('[') => {
            app.move_date_back();
        }
        KeyCode::Char(']') => {
            app.move_date_forward();
        }

        _ => {}
    }
    false
}

fn handle_replay_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.exit_replay();
        }

        KeyCode::Char(' ') => {
            app.replay.toggle_pause();
        }

        KeyCode::Right | KeyCode::Char('l') => {
            let count = app.current_snapshots().len();
            app.replay.step_forward(count);
        }

        KeyCode::Left | KeyCode::Char('h') => {
            app.replay.step_backward();
        }

        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.replay.speed_up();
        }

        KeyCode::Char('-') => {
            app.replay.slow_down();
        }

        KeyCode::Char('s') => {
            app.toggle_outcome();
        }

        _ => {}
    }
    false
}

async fn load_market_data(
    db: &Db,
    market: &crate::model::Market,
    user_addresses: &[String],
) -> anyhow::Result<MarketData> {
    let (snapshots, user_trades, resolution) = tokio::try_join!(
        db::fetch_orderbook(db, &market.condition_id),
        db::fetch_user_trades(db, &market.condition_id, user_addresses),
        db::fetch_resolution(db, &market.condition_id),
    )?;

    Ok(MarketData {
        market: market.clone(),
        snapshots,
        user_trades,
        resolution,
    })
}
