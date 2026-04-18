use std::collections::HashMap;

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
            let count = app.up_snapshots().len();
            app.replay.step_forward(count);
            app.sync_pnl();
        }

        KeyCode::Left | KeyCode::Char('h') => {
            app.replay.step_backward();
            app.sync_pnl();
        }

        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.replay.speed_up();
        }

        KeyCode::Char('-') => {
            app.replay.slow_down();
        }

        KeyCode::Char('t') => {
            app.show_all_trades = !app.show_all_trades;
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
    let trades_fut = fetch_polymarket_trades(&db.http, &market.condition_id, user_addresses);

    let (snapshots, mut user_trades, resolution) = tokio::try_join!(
        db::fetch_orderbook(db, &market.condition_id),
        trades_fut,
        db::fetch_resolution(db, &market.condition_id),
    )?;

    // Fetch all market trades separately — non-fatal so older markets without
    // `trades` table data still load correctly.
    let mut all_trades = db::fetch_market_trades(db, &market.condition_id)
        .await
        .unwrap_or_else(|e| {
            eprintln!("fetch_market_trades failed (continuing without): {e}");
            vec![]
        });

    // Phase 1 (read-only): correct user trade timestamps and collect annotation data.
    // hash_to_idx borrows all_trades immutably; drop it before mutating all_trades.
    // Match only by transaction_hash — QuestDB stores the counterparty's outcome
    // (inverted vs Polymarket), so (hash, outcome, side) never matches.
    let annotations: Vec<(usize, bool)> = {
        let hash_to_idx: HashMap<String, usize> = all_trades
            .iter()
            .enumerate()
            .map(|(i, t)| (t.transaction_hash.to_ascii_uppercase(), i))
            .collect();

        let mut pairs = Vec::new();
        for ut in &mut user_trades {
            let key = ut.transaction_hash.to_ascii_uppercase();
            if let Some(&idx) = hash_to_idx.get(&key) {
                ut.timestamp = all_trades[idx].timestamp; // nanosecond precision
                pairs.push((idx, ut.is_taker));
            }
        }
        pairs
    };

    // Phase 2: mark matching entries in all_trades as user trades.
    for (idx, is_taker) in annotations {
        all_trades[idx].is_user = true;
        all_trades[idx].is_taker = Some(is_taker);
    }

    user_trades.sort_by_key(|t| t.timestamp);

    let (up_snapshots, down_snapshots): (Vec<_>, Vec<_>) =
        snapshots.into_iter().partition(|s| s.outcome == "Up");

    Ok(MarketData {
        market: market.clone(),
        up_snapshots,
        down_snapshots,
        all_trades,
        user_trades,
        resolution,
    })
}

async fn fetch_polymarket_trades(
    http: &reqwest::Client,
    condition_id: &str,
    user_addresses: &[String],
) -> anyhow::Result<Vec<crate::model::UserTrade>> {
    let mut all = Vec::new();
    for addr in user_addresses {
        match crate::poly_api::fetch_trades_for_market(http, addr, condition_id).await {
            Ok(mut trades) => all.append(&mut trades),
            Err(e) => {
                eprintln!("polymarket fetch failed for {addr}: {e}");
            }
        }
    }
    all.sort_by_key(|t| t.timestamp);
    Ok(all)
}
