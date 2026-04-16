# /// script
# requires-python = ">=3.11"
# dependencies = ["requests>=2.31"]
# ///
"""Probe whether maker/taker role for a user can be derived by diffing
`takerOnly=true` vs `takerOnly=false` responses from Polymarket /trades.

Usage:
    uv run scripts/probe_maker_taker.py --user 0x... [--market 0x...]

If --market is omitted, probes the 5 most recent markets the user traded.
"""

import argparse
import sys
from collections import Counter

import requests

API = "https://data-api.polymarket.com/trades"
PAGE = 500


def fetch_all(user: str, market: str | None, taker_only: bool) -> list[dict]:
    out: list[dict] = []
    offset = 0
    while True:
        params = {
            "user": user,
            "limit": PAGE,
            "offset": offset,
            "takerOnly": "true" if taker_only else "false",
        }
        if market:
            params["market"] = market
        r = requests.get(API, params=params, timeout=30)
        r.raise_for_status()
        page = r.json()
        out.extend(page)
        if len(page) < PAGE:
            break
        offset += PAGE
    return out


def trade_key(t: dict) -> tuple:
    return (t["transactionHash"], str(t["asset"]), int(t["timestamp"]), float(t["size"]), float(t["price"]))


def probe(user: str, market: str | None) -> dict:
    all_trades = fetch_all(user, market, taker_only=False)
    taker_trades = fetch_all(user, market, taker_only=True)

    all_keys = {trade_key(t): t for t in all_trades}
    taker_keys = {trade_key(t) for t in taker_trades}

    taker_not_in_all = taker_keys - all_keys.keys()
    all_not_in_taker = set(all_keys.keys()) - taker_keys  # these should be makers

    roles = Counter()
    for k in all_keys:
        roles["taker" if k in taker_keys else "maker"] += 1

    return {
        "market": market or "(all)",
        "all_count": len(all_trades),
        "taker_count": len(taker_trades),
        "maker_inferred": len(all_not_in_taker),
        "taker_inferred": roles["taker"],
        "sanity_taker_not_in_all": len(taker_not_in_all),
        "sample_maker": next((all_keys[k] for k in all_not_in_taker), None),
        "sample_taker": next((t for t in taker_trades), None),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--user", required=True)
    ap.add_argument("--market", default=None, help="condition_id; omit to probe recent markets")
    ap.add_argument("--n-markets", type=int, default=5)
    args = ap.parse_args()

    if args.market:
        r = probe(args.user, args.market)
        print_summary(r)
        return 0

    # Grab a handful of recent distinct markets and probe each (single page
    # — unscoped /trades has a strict offset cap)
    r = requests.get(
        API,
        params={"user": args.user, "limit": PAGE, "offset": 0, "takerOnly": "false"},
        timeout=30,
    )
    r.raise_for_status()
    recent = r.json()
    seen: list[str] = []
    for t in recent:
        cid = t["conditionId"]
        if cid not in seen:
            seen.append(cid)
        if len(seen) >= args.n_markets:
            break

    print(f"Probing {len(seen)} recent markets for {args.user}\n")
    totals = Counter()
    for cid in seen:
        r = probe(args.user, cid)
        print_summary(r)
        totals["all"] += r["all_count"]
        totals["taker"] += r["taker_inferred"]
        totals["maker"] += r["maker_inferred"]
        totals["anomaly"] += r["sanity_taker_not_in_all"]
        print()

    print("=" * 60)
    print(f"TOTALS across {len(seen)} markets:")
    print(f"  trades            : {totals['all']}")
    print(f"  inferred taker    : {totals['taker']} "
          f"({totals['taker'] / max(totals['all'], 1):.1%})")
    print(f"  inferred maker    : {totals['maker']} "
          f"({totals['maker'] / max(totals['all'], 1):.1%})")
    print(f"  anomalies (taker-not-in-all): {totals['anomaly']}")
    return 0


def print_summary(r: dict) -> None:
    print(f"market={r['market']}")
    print(f"  all={r['all_count']:4d}  takerOnly={r['taker_count']:4d}  "
          f"→ inferred taker={r['taker_inferred']:4d}  maker={r['maker_inferred']:4d}  "
          f"anomaly={r['sanity_taker_not_in_all']}")
    if r["sample_maker"]:
        m = r["sample_maker"]
        print(f"  sample MAKER: ts={m['timestamp']} side={m['side']} "
              f"outcome={m['outcome']} price={m['price']} size={m['size']} "
              f"tx={m['transactionHash'][:14]}")
    if r["sample_taker"]:
        t = r["sample_taker"]
        print(f"  sample TAKER: ts={t['timestamp']} side={t['side']} "
              f"outcome={t['outcome']} price={t['price']} size={t['size']} "
              f"tx={t['transactionHash'][:14]}")


if __name__ == "__main__":
    sys.exit(main())
