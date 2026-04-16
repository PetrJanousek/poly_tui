# poly_tui — User Manual

A terminal UI for replaying Polymarket crypto-prediction orderbook data from QuestDB. Pair it with `poly_rustik` (sibling project) which records the live feed.

## Prerequisites

- **QuestDB** running on `localhost` (PG wire `:8812`, REST `:9000`).
- **`users.txt`** in the project root, one Ethereum address per line. These are the wallets whose trades appear in the Replay view (fetched live from Polymarket's data API on market selection — see "Data sources" below).
- `poly_rustik` (or some other recorder) has populated the `resolutions` and `orderbook` QuestDB tables. This TUI is read-only on those tables.

## Run

```bash
cargo run                    # connects to localhost
cargo run -- <host>          # or set QUESTDB_HOST env var
```

## Layout

Two modes. The top status bar and footer always show the relevant keybindings.

### 1. Market Browser (startup screen)

- List of markets from the `resolutions` table, filtered by crypto and date range (default: today).
- Shows each market's question and end time.

### 2. Replay (after pressing `Enter` on a market)

Split into three rows:

1. **Top**: market info panel | cumulative-depth chart | bid/ask ladder
2. **Middle**: user-trades table | PnL panel
3. **Footer (timeline)**: trade markers row + progress bar

## Keyboard shortcuts

### Market Browser

| Key | Action |
|---|---|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Enter` | Open selected market in Replay |
| `1` – `7` | Filter by crypto: `1`=btc, `2`=eth, `3`=xrp, `4`=sol, `5`=hype, `6`=doge, `7`=bnb |
| `a` | Clear crypto filter (show all) |
| `[` | Previous day |
| `]` | Next day |
| `q` / `Esc` | Quit |

### Replay

| Key | Action |
|---|---|
| `Space` | Play / pause |
| `l` / `→` | Step forward one snapshot |
| `h` / `←` | Step backward one snapshot |
| `+` or `=` | Increase playback speed |
| `-` | Decrease playback speed |
| `s` | Toggle outcome shown (Up ↔ Down) |
| `q` / `Esc` | Back to Market Browser |

Playback speeds cycle through: `1x → 2x → 5x → 10x → 30x → 50x → 200x → 2000x`.

## Reading the UI

### Market info panel (top-left, Replay)

- `Market` — question text.
- `Crypto` — underlying (BTC/ETH/…).
- `Outcome` — which side of the book you're viewing (Up/Down). Toggle with `s`.
- `Snapshots` — count of orderbook snapshots for this outcome.
- `Trades` — total user trades fetched for this market across all addresses in `users.txt`.
- `Playing | Paused` and current `Speed`.

### Depth chart (top-middle, Replay)

Cumulative bid/ask size at each level of the current snapshot.

### Orderbook ladder (top-right, Replay)

Top 5 bid and ask levels with price, size, and a colored bar proportional to size.

### Trades table (middle-left, Replay)

User trades *up to* the current replay cursor. As you play or step forward, new trades appear. Stepping backward drops trades (PnL re-computes from scratch).

Columns: `Time | Side | Outcome | Role | Price | Size`. `Role` is `T` (taker) or `M` (maker), derived by diffing the `takerOnly=true` vs `takerOnly=false` `/trades` responses per `(user, market)`.

Row colors: **green** for BUY Up (bullish-Up), **red** for everything else (BUY Down, SELL of either side).

### PnL panel (middle-right, Replay)

- `Up:` inventory and average cost for Up shares.
- `Down:` inventory and average cost for Down shares.
- `Sum:` (only when both sides held) `up_avg + down_avg`. **Green if < 1.0** (a guaranteed-profit arb regardless of resolution), **red if ≥ 1.0**. Gray parenthetical shows the delta from 1.0.
- `Realized:` cash PnL from closed trades.
- `Unrealized:` mark-to-market PnL of current inventory, using the **current orderbook mid** (`up_mid` for Up side, `1 − up_mid` for Down side).
- `Total:` Realized + Unrealized — what you'd pocket if you liquidated everything at mid right now.
- `Resolved:` the winning outcome (only once the market has resolved).
- `Final PnL:` hypothetical payout at settlement: `inventory × (1 if won else 0) − cost_basis + realized` summed across both sides. This is what you actually collect at expiry.

**Why Total ≠ Final.** Total is priced at the *current mid* (e.g. the eventual winner trading at 0.85). Final is priced at the *actual payout* (winner = $1, loser = $0). The gap closes as the market approaches resolution. If Final > Total the market hadn't fully priced in the outcome yet; if Final < Total you were mark-to-market overvaluing a losing side.

### Timeline (footer, Replay)

- **Top row**: `|` markers showing where trades occurred on the current market's time window. **Green** = Up trade, **red** = Down trade, **yellow** = both outcomes share that column. Markers outside the snapshot window are dropped.
- **Bottom row**: progress bar between the first and last snapshot timestamps, with the current cursor time, speed, and `<current>/<total>` snapshot index.

## Data sources

| What you see | Where it comes from |
|---|---|
| Market list | QuestDB `resolutions` (read-only; poly_rustik records this) |
| Orderbook snapshots | QuestDB `orderbook` via REST API (the 2D `bids`/`asks` arrays can't be deserialized by tokio-postgres) |
| Resolution / winning outcome | QuestDB `resolutions` (read-only) |
| User trades | **Polymarket `/trades` API**, live, scoped per `user × condition_id`. In-memory only — *not* persisted to QuestDB. Re-fetched on every market selection. |

The TUI does not write to QuestDB. Recording is poly_rustik's job.

## Troubleshooting

- **"DB error"** on startup — QuestDB isn't reachable. Start it (`localhost:9000`) and restart.
- **No markets listed** — the date filter defaults to today; try `[` to go back days, or check whether `resolutions` has data for the selected crypto.
- **No user trades** — either the addresses in `users.txt` didn't trade this market, the Polymarket API returned an error (printed to stderr on the terminal), or the market is outside the time window the API returns.
- **Sum line missing from PnL** — only appears when you hold both Up and Down inventory.
