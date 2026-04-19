# /// script
# requires-python = ">=3.11"
# dependencies = [
#     "httpx>=0.27",
# ]
# ///
"""
Reverse-engineer a Polymarket user's strategy on 5-minute BTC Up/Down markets.

Run:
    uv run analysis/reverse_engineer.py

QuestDB load: ~7-10 queries total (1 resolutions + N orderbook batches + 2 price queries).
"""

from __future__ import annotations

import bisect
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any

import httpx

# ---- config ----------------------------------------------------------------

USER_ADDR_DEFAULT = "0xb27bc932bf8110d8f78e55da7d5f0497a18b5b82"
QUESTDB_HOST = "100.77.79.50"
QUESTDB_PORT = 9000
POLYMARKET_DATA = "https://data-api.polymarket.com"
CRYPTO = "btc"
MAX_MARKETS = 100
ORDERBOOK_SAMPLE_MS = 100              # 100ms book sampling
ORDERBOOK_BATCH_SIZE = 25              # markets per bulk query
USER_TRADES_FETCH = 500
USER_TRADES_MAX_PAGES = 6              # up to 3000 trades if needed
WINDOW_S = 300                         # 5-minute markets
PRETRADE_LOOKBACK_MS = 200             # snap lookup: find snapshot at most this far before trade
BTC_BINANCE_SYMBOL = "btcusdt"
BTC_CHAINLINK_SYMBOL = "btc/usd"

# ---------------------------------------------------------------------------


def questdb(sql: str, timeout: float = 120.0) -> list[list[Any]]:
    url = f"http://{QUESTDB_HOST}:{QUESTDB_PORT}/exec"
    r = httpx.get(url, params={"query": sql, "fmt": "json"}, timeout=timeout)
    r.raise_for_status()
    data = r.json()
    if "error" in data:
        raise RuntimeError(f"QuestDB error: {data['error']}\nSQL: {sql[:500]}...")
    return data.get("dataset", [])


def parse_ts(s: str) -> datetime:
    s = s.rstrip("Z")
    if "." in s:
        head, frac = s.split(".")
        frac = (frac + "000000")[:6]
        s = f"{head}.{frac}"
        fmt = "%Y-%m-%dT%H:%M:%S.%f"
    else:
        fmt = "%Y-%m-%dT%H:%M:%S"
    return datetime.strptime(s, fmt).replace(tzinfo=timezone.utc)


# ---- data model ------------------------------------------------------------

@dataclass
class Snap:
    ts: datetime
    outcome: str
    bids: list[tuple[float, float]]
    asks: list[tuple[float, float]]

    @property
    def best_bid(self) -> tuple[float, float] | None:
        return self.bids[0] if self.bids else None

    @property
    def best_ask(self) -> tuple[float, float] | None:
        return self.asks[0] if self.asks else None

    @property
    def mid(self) -> float | None:
        if self.best_bid and self.best_ask:
            return (self.best_bid[0] + self.best_ask[0]) / 2
        return None

    @property
    def spread(self) -> float | None:
        if self.best_bid and self.best_ask:
            return self.best_ask[0] - self.best_bid[0]
        return None


@dataclass
class UserTrade:
    ts: datetime
    side: str
    outcome: str
    price: float
    size: float
    tx: str
    condition_id: str
    slug: str


@dataclass
class MarketMeta:
    condition_id: str
    question: str
    slug: str
    end_date: datetime | None
    winning_outcome: str | None = None


@dataclass
class MarketData:
    meta: MarketMeta
    start_ts: datetime
    snaps_by_outcome: dict[str, list[Snap]] = field(default_factory=dict)


@dataclass
class PriceSeries:
    """Time-sorted price ticks with bisect lookups."""
    tss: list[datetime] = field(default_factory=list)
    pxs: list[float] = field(default_factory=list)

    def at(self, ts: datetime) -> float | None:
        """Most recent price at-or-before ts."""
        if not self.tss:
            return None
        i = bisect.bisect_right(self.tss, ts)
        if i == 0:
            return None
        return self.pxs[i - 1]

    def delta(self, t1: datetime, t2: datetime) -> float | None:
        p1, p2 = self.at(t1), self.at(t2)
        if p1 is None or p2 is None:
            return None
        return p2 - p1


# ---- fetch -----------------------------------------------------------------

def fetch_user_trades(addr: str) -> list[UserTrade]:
    trades: list[UserTrade] = []
    seen: set[str] = set()
    for page in range(USER_TRADES_MAX_PAGES):
        offset = page * USER_TRADES_FETCH
        r = httpx.get(
            f"{POLYMARKET_DATA}/trades",
            params={"user": addr, "limit": USER_TRADES_FETCH, "offset": offset},
            timeout=30.0,
        )
        r.raise_for_status()
        batch = r.json()
        if not batch:
            break
        new = 0
        for t in batch:
            key = f"{t.get('transactionHash','')}:{t.get('asset','')}:{t.get('timestamp')}"
            if key in seen:
                continue
            seen.add(key)
            trades.append(UserTrade(
                ts=datetime.fromtimestamp(t["timestamp"], tz=timezone.utc),
                side=t["side"],
                outcome=t["outcome"],
                price=float(t["price"]),
                size=float(t["size"]),
                tx=t.get("transactionHash", ""),
                condition_id=t["conditionId"],
                slug=t.get("slug", ""),
            ))
            new += 1
        print(f"  page {page}: +{new} trades (total {len(trades)})")
        if len(batch) < USER_TRADES_FETCH:
            break
    trades.sort(key=lambda x: x.ts)
    return trades


def window_start_from_slug(slug: str) -> datetime | None:
    try:
        return datetime.fromtimestamp(int(slug.rsplit("-", 1)[-1]), tz=timezone.utc)
    except Exception:
        return None


def fetch_resolutions(condition_ids: list[str]) -> dict[str, MarketMeta]:
    if not condition_ids:
        return {}
    in_list = ",".join(f"'{c}'" for c in condition_ids)
    rows = questdb(
        "SELECT condition_id, question, slug, end_date, winning_outcome "
        "FROM resolutions "
        f"WHERE condition_id IN ({in_list}) "
        "LATEST ON timestamp PARTITION BY condition_id"
    )
    out = {}
    for r in rows:
        out[r[0]] = MarketMeta(
            condition_id=r[0],
            question=r[1] or "",
            slug=r[2] or "",
            end_date=parse_ts(r[3]) if r[3] else None,
            winning_outcome=(r[4] or None) if r[4] else None,
        )
    return out


def fetch_bulk_orderbook(condition_ids: list[str]) -> dict[str, dict[str, list[Snap]]]:
    result: dict[str, dict[str, list[Snap]]] = {}
    for i in range(0, len(condition_ids), ORDERBOOK_BATCH_SIZE):
        batch = condition_ids[i:i + ORDERBOOK_BATCH_SIZE]
        print(f"  orderbook batch {i // ORDERBOOK_BATCH_SIZE + 1} "
              f"({len(batch)} markets)...", flush=True)
        in_list = ",".join(f"'{c}'" for c in batch)
        rows = questdb(
            "SELECT last(timestamp) as ts, condition_id, outcome, "
            "       last(bids) as bids, last(asks) as asks "
            "FROM orderbook "
            f"WHERE condition_id IN ({in_list}) "
            f"SAMPLE BY {ORDERBOOK_SAMPLE_MS}ms ALIGN TO CALENDAR "
            "ORDER BY ts ASC",
            timeout=240.0,
        )
        for r in rows:
            ts = parse_ts(r[0])
            cond = r[1]
            outcome = r[2]
            bids_raw = r[3] or [[], []]
            asks_raw = r[4] or [[], []]
            bids = list(zip(bids_raw[0] or [], bids_raw[1] or [])) if len(bids_raw) >= 2 else []
            asks = list(zip(asks_raw[0] or [], asks_raw[1] or [])) if len(asks_raw) >= 2 else []
            result.setdefault(cond, {}).setdefault(outcome, []).append(
                Snap(ts=ts, outcome=outcome, bids=bids, asks=asks)
            )
        print(f"    -> {len(rows)} snapshots", flush=True)
    return result


def fetch_public_trades(condition_ids: list[str]) -> dict[str, list[dict]]:
    """Return condition_id -> list of public CLOB trades {ts,outcome,side,price,size}."""
    out: dict[str, list[dict]] = defaultdict(list)
    if not condition_ids:
        return out
    for i in range(0, len(condition_ids), ORDERBOOK_BATCH_SIZE):
        batch = condition_ids[i:i + ORDERBOOK_BATCH_SIZE]
        in_list = ",".join(f"'{c}'" for c in batch)
        rows = questdb(
            "SELECT timestamp, condition_id, outcome, side, price, size "
            "FROM trades "
            f"WHERE condition_id IN ({in_list}) "
            "ORDER BY timestamp ASC",
            timeout=120.0,
        )
        for r in rows:
            out[r[1]].append({
                "ts": parse_ts(r[0]),
                "outcome": r[2],
                "side": r[3],
                "price": float(r[4]),
                "size": float(r[5]),
            })
    return dict(out)


def match_user_to_public(
    user_trades: list[UserTrade],
    public_by_cond: dict[str, list[dict]],
) -> dict[str, tuple[datetime, float]]:
    """For each user trade (by tx hash), find the closest public trade with
    matching condition+outcome+price+size. Returns {tx: (match_ts, diff_seconds)}."""
    out: dict[str, tuple[datetime, float]] = {}
    for u in user_trades:
        cands = public_by_cond.get(u.condition_id, [])
        best = None  # (abs_diff_s, match_ts)
        for p in cands:
            if p["outcome"] != u.outcome:
                continue
            if abs(p["price"] - u.price) > 1e-5:
                continue
            if abs(p["size"] - u.size) > 1e-3:
                continue
            diff = (p["ts"] - u.ts).total_seconds()
            abs_diff = abs(diff)
            if best is None or abs_diff < best[0]:
                best = (abs_diff, p["ts"], diff)
        if best:
            out[u.tx] = (best[1], best[2])
    return out


def fetch_price_series(symbol: str, t_from: datetime, t_to: datetime) -> PriceSeries:
    table = "binance_prices" if "usdt" in symbol else "crypto_prices"
    sql = (
        "SELECT timestamp, price "
        f"FROM {table} "
        f"WHERE symbol = '{symbol}' "
        f"  AND timestamp >= cast({int(t_from.timestamp() * 1_000_000)} as timestamp) "
        f"  AND timestamp <= cast({int(t_to.timestamp() * 1_000_000)} as timestamp) "
        "ORDER BY timestamp ASC"
    )
    rows = questdb(sql, timeout=60.0)
    series = PriceSeries()
    for r in rows:
        series.tss.append(parse_ts(r[0]))
        series.pxs.append(float(r[1]))
    return series


# ---- helpers ---------------------------------------------------------------

def snap_pretrade(snaps: list[Snap], ts: datetime) -> Snap | None:
    """Snapshot strictly BEFORE ts (using PRETRADE_LOOKBACK_MS cap for sanity).
    This avoids the post-trade-state measurement artifact from same-second sampling."""
    cutoff = ts - timedelta(milliseconds=1)  # strictly before
    min_ts = ts - timedelta(seconds=5)       # don't reach too far back
    out = None
    for s in snaps:
        if s.ts < min_ts:
            continue
        if s.ts <= cutoff:
            out = s
        else:
            break
    return out


def snap_after(snaps: list[Snap], ts: datetime, min_s: float) -> Snap | None:
    for s in snaps:
        if s.ts > ts and (s.ts - ts).total_seconds() >= min_s:
            return s
    return None


def classify_aggression(trade: UserTrade, pre: Snap | None) -> str:
    if not pre or not pre.best_bid or not pre.best_ask:
        return "one-sided"
    bb, ba = pre.best_bid[0], pre.best_ask[0]
    if trade.side == "BUY":
        if trade.price >= ba - 1e-6:
            return "taker"
        if trade.price <= bb + 1e-6:
            return "below-bid"      # resting limit filled on a sweep
        return "inside"
    else:
        if trade.price <= bb + 1e-6:
            return "taker"
        if trade.price >= ba - 1e-6:
            return "above-ask"
        return "inside"


def window_bucket(off_s: int) -> str:
    if off_s < 0:
        return "pre-open"
    if off_s < 30:
        return "0-30s   (open)"
    if off_s < 120:
        return "30-120s (mid-early)"
    if off_s < 240:
        return "120-240s(mid-late)"
    if off_s < 270:
        return "240-270s(pre-final)"
    if off_s <= 300:
        return "270-300s(last 30s)"
    return "300+s   (post-close)"


def trade_pnl(trade: UserTrade, winning_outcome: str | None) -> float | None:
    if winning_outcome is None:
        return None
    win = (trade.outcome == winning_outcome)
    if trade.side == "BUY":
        return trade.size * (1.0 - trade.price) if win else -trade.size * trade.price
    else:
        return trade.size * (trade.price - 1.0) if win else trade.size * trade.price


# ---- analysis --------------------------------------------------------------

def simulate_naive_strategy(
    picks: list[str],
    metas: dict[str, MarketMeta],
    ob_by_cond: dict[str, dict[str, list[Snap]]],
    mid_offset: float = 0.10,
    per_side_usd: float = 50.0,
    mid_min: float = 0.20,
    mid_max: float = 0.80,
) -> dict:
    """Simulate: at window open, place limit bid at (opening_mid - mid_offset) on BOTH
    Up and Down with fixed $per_side_usd notional. A limit fills if any best_ask during
    the window dips to limit_px or below. PnL = shares * (settle - limit_px)."""
    results = []
    for cond in picks:
        meta = metas.get(cond)
        if not meta or not meta.winning_outcome:
            continue
        ws = window_start_from_slug(meta.slug)
        if ws is None:
            continue
        for outcome in ("Up", "Down"):
            snaps = ob_by_cond.get(cond, {}).get(outcome, [])
            if not snaps:
                continue
            first = [s for s in snaps if (s.ts - ws).total_seconds() < 10]
            mids = [s.mid for s in first if s.mid is not None]
            if not mids:
                continue
            opening_mid = sum(mids) / len(mids)
            if not (mid_min < opening_mid < mid_max):
                continue  # skip certain-outcome markets
            limit_px = round(opening_mid - mid_offset, 3)
            if limit_px <= 0.02 or limit_px >= 0.98:
                continue
            # Only consider snaps after we placed (t+2s) until window end
            place_ts = ws + timedelta(seconds=2)
            end_ts = ws + timedelta(seconds=WINDOW_S)
            relevant = [s for s in snaps if place_ts <= s.ts <= end_ts]
            min_ask = min((s.best_ask[0] for s in relevant if s.best_ask), default=None)
            if min_ask is None or min_ask > limit_px:
                filled = False
                pnl = 0.0
                shares = 0.0
            else:
                filled = True
                shares = per_side_usd / limit_px
                settle = 1.0 if outcome == meta.winning_outcome else 0.0
                pnl = shares * (settle - limit_px)
            results.append({
                "cond": cond, "outcome": outcome,
                "opening_mid": opening_mid, "limit_px": limit_px,
                "filled": filled, "shares": shares, "pnl": pnl,
                "won": (outcome == meta.winning_outcome),
            })
    return {
        "mid_offset": mid_offset,
        "per_side_usd": per_side_usd,
        "attempts": len(results),
        "fills": sum(1 for r in results if r["filled"]),
        "wins": sum(1 for r in results if r["filled"] and r["pnl"] > 0),
        "losses": sum(1 for r in results if r["filled"] and r["pnl"] < 0),
        "pnl": sum(r["pnl"] for r in results),
        "notional": sum(r["shares"] * r["limit_px"] for r in results if r["filled"]),
        "rows": results,
    }


def bag_holder_analysis(
    by_cond: dict[str, list[UserTrade]],
    metas: dict[str, MarketMeta],
    binance: PriceSeries,
    picks: list[str],
) -> None:
    """For markets where user made 5+ trades, compare winners vs losers.
    Print summary of what differentiates 'bag-holding' losses from winning multi-trade markets."""
    multi_markets = []
    for cond in picks:
        trades = sorted(by_cond.get(cond, []), key=lambda t: t.ts)
        meta = metas.get(cond)
        if not meta or not meta.winning_outcome or len(trades) < 5:
            continue
        total_pnl = sum(trade_pnl(t, meta.winning_outcome) or 0 for t in trades)
        ws = window_start_from_slug(meta.slug)
        btc_move = None
        if ws:
            btc_move = binance.delta(ws, ws + timedelta(seconds=WINDOW_S))
        # How much of user's volume was on the losing side?
        losing_vol = sum(t.size * t.price for t in trades if t.outcome != meta.winning_outcome)
        winning_vol = sum(t.size * t.price for t in trades if t.outcome == meta.winning_outcome)
        total_vol = losing_vol + winning_vol
        # Did he switch sides mid-market?
        sides = [t.outcome for t in trades]
        switches = sum(1 for i in range(1, len(sides)) if sides[i] != sides[i - 1])
        # Did he double down on a losing side after it got worse?
        doubled_down = 0
        for i in range(1, len(trades)):
            prev, cur = trades[i - 1], trades[i]
            if cur.outcome != meta.winning_outcome and cur.outcome == prev.outcome:
                if cur.price < prev.price:  # bought cheaper = averaging down
                    doubled_down += 1
        multi_markets.append({
            "cond": cond, "meta": meta, "trades": trades, "pnl": total_pnl,
            "btc_move": btc_move, "losing_vol_pct": losing_vol / total_vol * 100 if total_vol else 0,
            "switches": switches, "doubled_down": doubled_down,
            "n": len(trades), "first_price": trades[0].price,
            "last_price": trades[-1].price,
        })
    if not multi_markets:
        print("  (no multi-trade markets to analyze)")
        return
    winners = [m for m in multi_markets if m["pnl"] > 0]
    losers = [m for m in multi_markets if m["pnl"] < 0]
    print(f"Multi-trade markets (≥5 trades): {len(multi_markets)}  "
          f"winners={len(winners)}  losers={len(losers)}")
    print()
    print(f"  {'avg':<20} {'winners':>10} {'losers':>10}")
    def avg(xs, key):
        vals = [m[key] for m in xs if m[key] is not None]
        return sum(vals) / len(vals) if vals else 0
    for key, label in [
        ("n", "#trades"),
        ("losing_vol_pct", "% vol on loser"),
        ("switches", "side switches"),
        ("doubled_down", "double-downs"),
        ("btc_move", "BTC move ($)"),
        ("pnl", "PnL"),
    ]:
        w = avg(winners, key); l = avg(losers, key)
        print(f"  {label:<20} {w:>10.2f} {l:>10.2f}")

    print("\n  Top 5 bag-holder losses (detail):")
    losers.sort(key=lambda m: m["pnl"])
    for m in losers[:5]:
        meta = m["meta"]
        end_str = meta.end_date.strftime("%m-%d %H:%M") if meta.end_date else "?"
        print(f"    {end_str}  winner={meta.winning_outcome}  "
              f"n={m['n']}  loser-vol%={m['losing_vol_pct']:.0f}  "
              f"switches={m['switches']}  doubled={m['doubled_down']}  "
              f"btcΔ=${m['btc_move']:+.1f}  PnL={m['pnl']:+.2f}" if m['btc_move'] else
              f"    {end_str}  winner={meta.winning_outcome}  "
              f"n={m['n']}  loser-vol%={m['losing_vol_pct']:.0f}  "
              f"PnL={m['pnl']:+.2f}")
        # Trade-by-trade
        for t in m["trades"]:
            is_loser_side = (t.outcome != meta.winning_outcome)
            tag = "LOSE" if is_loser_side else " win"
            off = int((t.ts - window_start_from_slug(meta.slug)).total_seconds()) if window_start_from_slug(meta.slug) else 0
            print(f"      t+{off:>3}s  {tag}  {t.side} {t.outcome} @ {t.price:.3f} × {t.size:>6.1f}")


def analyze_market(
    trades: list[UserTrade],
    md: MarketData,
    binance: PriceSeries,
    chainlink: PriceSeries,
    print_detail: bool,
    match_ts_by_tx: dict[str, tuple[datetime, float]] | None = None,
) -> dict:
    meta = md.meta
    resolved = meta.winning_outcome
    win_start = md.start_ts
    win_end = win_start + timedelta(seconds=WINDOW_S)
    btc_open = binance.at(win_start)
    btc_close = binance.at(win_end)
    cl_open = chainlink.at(win_start)
    cl_close = chainlink.at(win_end)
    btc_move = (btc_close - btc_open) if (btc_open and btc_close) else None

    if print_detail:
        end_str = meta.end_date.strftime("%m-%d %H:%M") if meta.end_date else "?"
        title = meta.question or meta.slug or meta.condition_id[:12]
        open_str = f"${btc_open:,.2f}" if btc_open else "?"
        close_str = f"${btc_close:,.2f}" if btc_close else "?"
        move_str = f"{btc_move:+.2f}" if btc_move is not None else "?"
        cl_open_str = f"${cl_open:,.2f}" if cl_open else "?"
        cl_close_str = f"${cl_close:,.2f}" if cl_close else "?"
        print("-" * 130)
        print(f"{title}")
        print(f"  end={end_str}   winner={resolved or '?'}   user trades: {len(trades)}")
        print(f"  BTC binance:   open={open_str}  close={close_str}  Δ={move_str}")
        print(f"  BTC chainlink: open={cl_open_str}  close={cl_close_str}")
        hdr = (
            f"    {'t':>5} {'side':>4} {'out':>4} "
            f"{'px':>6} {'size':>7} "
            f"{'bid':>5} {'ask':>5} {'spd':>5} "
            f"{'agg':>9} "
            f"{'btcSpot':>9} {'btcΔ-5s':>8} {'btcΔ-30s':>9} "
            f"{'btcToClose':>11} {'PnL':>8}"
        )
        print(hdr)

    stats = {
        "trades": 0, "buys": 0, "sells": 0, "up": 0, "down": 0,
        "taker": 0, "below_bid": 0, "inside": 0, "one_sided": 0, "above_ask": 0,
        "buckets": defaultdict(int),
        "bucket_vol": defaultdict(float),
        "bucket_pnl": defaultdict(float),
        "notional": 0.0, "pnl": 0.0, "pnl_known": 0, "won_trades": 0,
        # direction-alignment: did BTC already move toward his side in the 30s before trade?
        "momentum_aligned": 0, "momentum_against": 0, "momentum_flat": 0,
        # did BTC end up on his side? (post-trade → close)
        "btc_to_close_for": 0, "btc_to_close_against": 0, "btc_to_close_flat": 0,
        # price strata: cheap (<0.4), mid (0.4-0.6), strong (0.6-0.85), near-certain (>0.85)
        "price_strata": defaultdict(lambda: {"n": 0, "pnl": 0.0, "won": 0}),
        # aggression × outcome-winning
        "agg_pnl": defaultdict(lambda: {"n": 0, "pnl": 0.0, "won": 0}),
    }

    for t in trades:
        snaps = md.snaps_by_outcome.get(t.outcome, [])
        # Use matched public-trade timestamp when available (true match time)
        ref_ts = t.ts
        if match_ts_by_tx and t.tx in match_ts_by_tx:
            ref_ts = match_ts_by_tx[t.tx][0]
        pre = snap_pretrade(snaps, ref_ts)
        agg = classify_aggression(t, pre)
        pnl = trade_pnl(t, resolved)
        off_s = int((ref_ts - win_start).total_seconds())
        bucket = window_bucket(off_s)

        # Directional alignment vs BTC spot (using matched ref_ts)
        btc_at = binance.at(ref_ts)
        btc_m5 = binance.delta(ref_ts - timedelta(seconds=5), ref_ts)
        btc_m30 = binance.delta(ref_ts - timedelta(seconds=30), ref_ts)
        btc_to_close_val = (btc_close - btc_at) if (btc_at and btc_close) else None

        favor_sign = +1 if t.outcome == "Up" else -1   # he's long Up → want BTC up, etc.
        # Use 30s momentum as "did BTC move his way BEFORE trade?"
        if btc_m30 is not None:
            signed = btc_m30 * favor_sign
            if abs(btc_m30) < 1e-6:
                stats["momentum_flat"] += 1
            elif signed > 0:
                stats["momentum_aligned"] += 1
            else:
                stats["momentum_against"] += 1
        if btc_to_close_val is not None:
            signed = btc_to_close_val * favor_sign
            if abs(btc_to_close_val) < 1e-6:
                stats["btc_to_close_flat"] += 1
            elif signed > 0:
                stats["btc_to_close_for"] += 1
            else:
                stats["btc_to_close_against"] += 1

        # Price stratum
        if t.price < 0.4: stratum = "cheap (<0.4)"
        elif t.price < 0.6: stratum = "mid   (0.4-0.6)"
        elif t.price < 0.85: stratum = "strong(0.6-0.85)"
        else: stratum = "near-1(>0.85)"
        rec = stats["price_strata"][stratum]
        rec["n"] += 1
        if pnl is not None:
            rec["pnl"] += pnl
            if pnl > 0:
                rec["won"] += 1
        rec2 = stats["agg_pnl"][agg]
        rec2["n"] += 1
        if pnl is not None:
            rec2["pnl"] += pnl
            if pnl > 0: rec2["won"] += 1

        stats["trades"] += 1
        if t.side == "BUY": stats["buys"] += 1
        else: stats["sells"] += 1
        if t.outcome == "Up": stats["up"] += 1
        else: stats["down"] += 1
        key_map = {
            "taker": "taker", "below-bid": "below_bid", "inside": "inside",
            "one-sided": "one_sided", "above-ask": "above_ask",
        }
        stats[key_map.get(agg, "inside")] += 1
        stats["buckets"][bucket] += 1
        stats["bucket_vol"][bucket] += t.size * t.price
        stats["notional"] += t.size * t.price
        if pnl is not None:
            stats["pnl"] += pnl
            stats["pnl_known"] += 1
            stats["bucket_pnl"][bucket] += pnl
            if pnl > 0:
                stats["won_trades"] += 1

        if print_detail:
            bid_px = f"{pre.best_bid[0]:.3f}" if pre and pre.best_bid else "  -  "
            ask_px = f"{pre.best_ask[0]:.3f}" if pre and pre.best_ask else "  -  "
            spd = f"{pre.spread:.3f}" if pre and pre.spread is not None else "  -  "
            btc_str = f"{btc_at:,.0f}" if btc_at else "    -"
            m5_str = f"{btc_m5:+.1f}" if btc_m5 is not None else "   -"
            m30_str = f"{btc_m30:+.1f}" if btc_m30 is not None else "    -"
            tc_str = f"{btc_to_close_val:+.1f}" if btc_to_close_val is not None else "     -"
            pnl_str = f"{pnl:+.2f}" if pnl is not None else "    ?"
            print(
                f"    {off_s:>4}s {t.side:>4} {t.outcome:>4} "
                f"{t.price:>6.3f} {t.size:>7.2f} "
                f"{bid_px:>5} {ask_px:>5} {spd:>5} "
                f"{agg:>9} "
                f"{btc_str:>9} {m5_str:>8} {m30_str:>9} "
                f"{tc_str:>11} {pnl_str:>8}"
            )

    return stats


def aggregate_summary(all_stats: list[dict], meta_by_cond: dict[str, MarketMeta]) -> None:
    print()
    print("=" * 130)
    print("AGGREGATE ACROSS ALL MARKETS")
    print("=" * 130)
    if not all_stats:
        return
    tot = lambda k: sum(s[k] for s in all_stats)
    n = tot("trades")
    buys = tot("buys"); sells = tot("sells")
    up = tot("up"); down = tot("down")
    taker = tot("taker"); below = tot("below_bid"); inside = tot("inside")
    one = tot("one_sided"); above = tot("above_ask")
    notional = tot("notional")
    pnl = sum(s["pnl"] for s in all_stats)
    pnl_known = sum(s["pnl_known"] for s in all_stats)
    won = sum(s["won_trades"] for s in all_stats)

    print(f"Markets analyzed:      {len(all_stats)}")
    print(f"Total trades:          {n}  (BUY={buys}  SELL={sells})")
    print(f"Outcome mix:           Up={up} ({up/max(1,n)*100:.0f}%)   "
          f"Down={down} ({down/max(1,n)*100:.0f}%)")
    print(f"Aggression:            taker={taker}  below-bid={below}  "
          f"inside={inside}  above-ask={above}  one-sided-book={one}")
    if n:
        print(f"  → taker rate: {taker/n*100:.0f}%   below-bid rate: {below/n*100:.0f}%   "
              f"inside rate: {inside/n*100:.0f}%")
    print(f"Total USD notional:    ${notional:,.2f}")
    if pnl_known:
        print(f"Resolved trades:       {pnl_known}/{n}")
        print(f"Hit rate (trade-PnL>0):{won}/{pnl_known} = {won/pnl_known*100:.0f}%")
        print(f"Realized PnL:          ${pnl:+,.2f}   ({pnl/notional*100:+.1f}% of notional)")

    print("\nTiming × volume × PnL:")
    print(f"  {'bucket':<22} {'trades':>7} {'notional':>12} {'PnL':>10} {'PnL/notl':>9}")
    for b in [
        "pre-open", "0-30s   (open)", "30-120s (mid-early)", "120-240s(mid-late)",
        "240-270s(pre-final)", "270-300s(last 30s)", "300+s   (post-close)",
    ]:
        c = sum(s["buckets"].get(b, 0) for s in all_stats)
        v = sum(s["bucket_vol"].get(b, 0.0) for s in all_stats)
        p = sum(s["bucket_pnl"].get(b, 0.0) for s in all_stats)
        if c == 0:
            continue
        ratio = p / v * 100 if v else 0
        print(f"  {b:<22} {c:>7} ${v:>10,.2f} {p:>+9.2f} {ratio:>+7.1f}%")

    print("\nPrice stratum (where he buys):")
    print(f"  {'stratum':<18} {'trades':>7} {'hit%':>5} {'PnL':>10}")
    stratum_names = ["cheap (<0.4)", "mid   (0.4-0.6)", "strong(0.6-0.85)", "near-1(>0.85)"]
    for st in stratum_names:
        nn = 0; ww = 0; pp = 0.0
        for s in all_stats:
            r = s["price_strata"].get(st)
            if r:
                nn += r["n"]; ww += r["won"]; pp += r["pnl"]
        if nn == 0:
            continue
        print(f"  {st:<18} {nn:>7} {ww/nn*100:>4.0f}% {pp:>+10.2f}")

    print("\nAggression × PnL:")
    print(f"  {'type':<12} {'trades':>7} {'hit%':>5} {'PnL':>10} {'avg PnL/tr':>11}")
    for a in ("taker", "below-bid", "inside", "above-ask", "one-sided"):
        nn = 0; ww = 0; pp = 0.0
        for s in all_stats:
            r = s["agg_pnl"].get(a)
            if r:
                nn += r["n"]; ww += r["won"]; pp += r["pnl"]
        if nn == 0:
            continue
        print(f"  {a:<12} {nn:>7} {ww/nn*100:>4.0f}% {pp:>+10.2f} {pp/nn:>+11.2f}")

    # BTC alignment
    mf = tot("momentum_aligned"); ma = tot("momentum_against"); mz = tot("momentum_flat")
    cf = tot("btc_to_close_for"); ca = tot("btc_to_close_against"); cz = tot("btc_to_close_flat")
    print("\nBTC-direction alignment:")
    if mf + ma + mz:
        print(f"  Last 30s BTC momentum aligned with his side: "
              f"{mf}  against: {ma}  flat: {mz}   "
              f"→ aligned rate {mf/(mf+ma+mz)*100:.0f}%  "
              f"(purely trend-following would be ~high; picks dips would be ~low)")
    if cf + ca + cz:
        print(f"  BTC direction from trade → window close: "
              f"for him: {cf}  against: {ca}  flat: {cz}   "
              f"→ {cf/(cf+ca+cz)*100:.0f}% of trades end up with BTC moving his way")

    # Outcome × aggression heatmap (which combos make money)
    print("\nPer-market PnL tail (top 10 wins, top 10 losses):")
    market_pnls = []
    for s in all_stats:
        cond = s.get("_cond")
        meta = meta_by_cond.get(cond)
        if not meta or not s["pnl_known"]:
            continue
        market_pnls.append((s["pnl"], meta, s))
    market_pnls.sort(key=lambda x: x[0], reverse=True)
    print("  Top wins:")
    for pnl_v, meta, s in market_pnls[:10]:
        end_str = meta.end_date.strftime("%m-%d %H:%M") if meta.end_date else "?"
        print(f"    {end_str}  winner={meta.winning_outcome:<4}  "
              f"trades={s['trades']:>2}  notl=${s['notional']:>8,.2f}  "
              f"PnL={pnl_v:>+9.2f}")
    print("  Top losses:")
    for pnl_v, meta, s in market_pnls[-10:]:
        end_str = meta.end_date.strftime("%m-%d %H:%M") if meta.end_date else "?"
        print(f"    {end_str}  winner={meta.winning_outcome:<4}  "
              f"trades={s['trades']:>2}  notl=${s['notional']:>8,.2f}  "
              f"PnL={pnl_v:>+9.2f}")


# ---- main ------------------------------------------------------------------

def main() -> int:
    addr = sys.argv[1] if len(sys.argv) > 1 else USER_ADDR_DEFAULT
    addr = addr.lower()
    print(f"User:  {addr}")
    print(f"Fetching trades from Polymarket (up to {USER_TRADES_MAX_PAGES * USER_TRADES_FETCH})...")
    user_trades = fetch_user_trades(addr)
    print(f"  got {len(user_trades)} trades")
    if not user_trades:
        return 1

    by_cond: dict[str, list[UserTrade]] = defaultdict(list)
    for t in user_trades:
        by_cond[t.condition_id].append(t)
    btc_conds = [c for c, ts in by_cond.items()
                 if ts and ts[0].slug.startswith(f"{CRYPTO}-updown-")]
    btc_conds.sort(key=lambda c: max(t.ts for t in by_cond[c]), reverse=True)
    picks = btc_conds[:MAX_MARKETS]
    print(f"BTC markets he traded: {len(btc_conds)}   picking {len(picks)} most recent\n")

    # Time range to bound price queries
    all_slugs = [by_cond[c][0].slug for c in picks if by_cond[c]]
    windows = [window_start_from_slug(s) for s in all_slugs]
    windows = [w for w in windows if w]
    t_from = min(windows) - timedelta(minutes=1)
    t_to = max(windows) + timedelta(minutes=6)

    print(f"Time range: {t_from}  →  {t_to}")

    print("Fetching resolutions (1 query)...")
    metas = fetch_resolutions(picks)
    print(f"  resolved {sum(1 for m in metas.values() if m.winning_outcome)} / {len(metas)} markets")

    print(f"Fetching orderbook at {ORDERBOOK_SAMPLE_MS}ms "
          f"(batches of {ORDERBOOK_BATCH_SIZE})...")
    ob_by_cond = fetch_bulk_orderbook(picks)
    total_snaps = sum(len(v) for c in ob_by_cond.values() for v in c.values())
    print(f"  total {total_snaps:,} snapshots across {len(ob_by_cond)} markets")

    print(f"Fetching Binance {BTC_BINANCE_SYMBOL} (1 query)...")
    binance = fetch_price_series(BTC_BINANCE_SYMBOL, t_from, t_to)
    print(f"  {len(binance.tss):,} ticks")

    print(f"Fetching Chainlink {BTC_CHAINLINK_SYMBOL} (1 query)...")
    chainlink = fetch_price_series(BTC_CHAINLINK_SYMBOL, t_from, t_to)
    print(f"  {len(chainlink.tss):,} ticks")

    print("Fetching public trades (for timestamp cross-check)...")
    public_by_cond = fetch_public_trades(picks)
    pubtot = sum(len(v) for v in public_by_cond.values())
    print(f"  {pubtot:,} public trades across {len(public_by_cond)} markets")

    print("Matching user trades to public trades...")
    users_in_range = [t for t in user_trades if t.condition_id in set(picks)]
    match_ts_by_tx = match_user_to_public(users_in_range, public_by_cond)
    matched_n = len(match_ts_by_tx)
    print(f"  matched {matched_n}/{len(users_in_range)} user trades")
    if match_ts_by_tx:
        diffs = [d for _, d in match_ts_by_tx.values()]
        diffs_sorted = sorted(diffs)
        med = diffs_sorted[len(diffs_sorted) // 2]
        mn = min(diffs); mx = max(diffs)
        abs_med = sorted(abs(d) for d in diffs)[len(diffs) // 2]
        print(f"  timestamp diff (public - user):  min={mn:+.1f}s  median={med:+.1f}s  "
              f"max={mx:+.1f}s   median(|diff|)={abs_med:.1f}s")
    print()

    all_stats = []
    # Detail print for the most recent 10 markets only, aggregate the rest
    DETAIL_N = 10
    for i, cond in enumerate(picks):
        trades = sorted(by_cond[cond], key=lambda t: t.ts)
        meta = metas.get(cond) or MarketMeta(cond, "", trades[0].slug if trades else "", None)
        ws = window_start_from_slug(meta.slug or (trades[0].slug if trades else ""))
        if ws is None:
            continue
        md = MarketData(meta=meta, start_ts=ws,
                        snaps_by_outcome=ob_by_cond.get(cond, {}))
        stats = analyze_market(
            trades, md, binance, chainlink,
            print_detail=(i < DETAIL_N),
            match_ts_by_tx=match_ts_by_tx,
        )
        stats["_cond"] = cond
        all_stats.append(stats)

    aggregate_summary(all_stats, metas)

    # --- Task C: naïve simulation at 3 aggressiveness levels -----------------
    print()
    print("=" * 130)
    print("TASK C: NAÏVE 'BOTH-SIDED LIMIT' SIMULATION")
    print("=" * 130)
    print("Strategy: at t+2s place a limit bid at (opening_mid - offset) on BOTH Up and Down,")
    print("fixed $50 per side; fill if best_ask dips to limit during window; held to settle.\n")
    for offset in (0.05, 0.10, 0.15, 0.20):
        sim = simulate_naive_strategy(picks, metas, ob_by_cond,
                                      mid_offset=offset, per_side_usd=50.0)
        a = sim["attempts"]; f = sim["fills"]; w = sim["wins"]; l = sim["losses"]
        pnl = sim["pnl"]; notl = sim["notional"]
        fill_rate = f / a * 100 if a else 0
        hit_rate = w / f * 100 if f else 0
        roi = pnl / notl * 100 if notl else 0
        print(f"  offset={offset:.2f}:  attempts={a:>3}  fills={f:>3} ({fill_rate:.0f}%)  "
              f"W/L={w}/{l}  hit={hit_rate:.0f}%  "
              f"notl=${notl:>9,.2f}  PnL=${pnl:+9.2f}  ROI={roi:+.1f}%")

    # --- Task D: bag-holder analysis -----------------------------------------
    print()
    print("=" * 130)
    print("TASK D: BAG-HOLDER PATTERN (markets with ≥5 trades)")
    print("=" * 130)
    by_cond_picks = {c: by_cond[c] for c in picks}
    bag_holder_analysis(by_cond_picks, metas, binance, picks)

    return 0


if __name__ == "__main__":
    sys.exit(main())
