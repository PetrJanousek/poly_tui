# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build          # build
cargo run            # run (requires real terminal + QuestDB running)
```

No tests yet. No linter configured beyond `cargo check`.

## What This Is

A ratatui TUI for replaying Polymarket crypto prediction market orderbook data from QuestDB. Companion to `poly_rustik` (sibling directory) which records the live data. This tool reads historical snapshots and lets you scrub through them like a DVR.

Data is from 5-minute crypto up/down prediction markets (btc, eth, xrp, sol, hype, doge, bnb). Each market has a `condition_id` and two outcomes: "Up" and "Down".

## Prerequisites

- **QuestDB** running on localhost (PG wire on port 8812, REST API on port 9000)
- **`users.txt`** in project root with Ethereum addresses to track (one per line)

## Architecture

Two modes controlled by `AppMode` in `app.rs`:

1. **MarketBrowser** — lists markets from the `resolutions` QuestDB table, filtered by crypto and date. There is no `markets` table; `resolutions` is the source of truth for market metadata (question, slug, end_date, condition_id).

2. **Replay** — loads all orderbook snapshots + user trades + resolution for a selected `condition_id` into memory, then steps through them chronologically. Snapshots are filtered by outcome ("Up"/"Down", toggled with `s`).

### Database layer (`db.rs`)

Uses **two connections** wrapped in `Db` struct:
- `pg` (tokio-postgres on port 8812) — for markets, user_trades, resolutions. Standard row queries.
- `http` (reqwest to port 9000) — **only for orderbook queries**. QuestDB's 2D array columns (`bids`, `asks`) cannot be deserialized by tokio-postgres (type `Float8Array` fails). The REST API returns them as proper JSON arrays.

Orderbook arrays are `[[prices], [sizes]]` — index 0 is prices, index 1 is sizes, up to 5 levels each. Bids sorted descending, asks sorted ascending.

### Replay engine (`replay.rs`)

Time-scaled cursor: compares real timestamp gaps between consecutive snapshots and divides by speed multiplier (1x/2x/5x/10x). Trades are revealed when their timestamp <= current snapshot timestamp.

### PnL tracking (`pnl.rs`)

Tracks per-outcome (Up/Down) inventory, cost basis, and realized PnL. Processes trades incrementally as they become visible during replay.

### UI (`ui/`)

ratatui with crossterm backend at 20 FPS. Layout splits into:
- **Top row**: market info | depth chart (cumulative bars) | orderbook (bid/ask levels with colored bars)
- **Bottom row**: trades table | PnL panel
- **Footer**: timeline progress bar with speed indicator

Each panel is a separate module receiving the relevant slice of app state.

### Event handling (`event.rs`)

Keyboard dispatch is async because `Enter` in MarketBrowser triggers DB queries via `load_market_data()` which uses `tokio::try_join!` to fetch orderbook, trades, and resolution in parallel.

## QuestDB Tables

| Table | Used For | Query Method |
|-------|----------|-------------|
| `resolutions` | Market list (condition_id, crypto, question, end_date) | PG wire |
| `orderbook` | Orderbook snapshots (bids/asks as 2D arrays) | REST API |
| `user_trades` | User trades (side, outcome, price, size) | PG wire |

Note: `crypto_prices`, `binance_prices` tables exist but are not yet used by this TUI.
