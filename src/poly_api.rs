use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::DateTime;
use serde::Deserialize;

use crate::model::UserTrade;

const TRADES_URL: &str = "https://data-api.polymarket.com/trades";
const PAGE_SIZE: u32 = 500;

#[derive(Debug, Deserialize)]
struct RawTrade {
    timestamp: i64,
    side: String,
    outcome: String,
    price: f64,
    size: f64,
    asset: String,
    #[serde(rename = "transactionHash")]
    transaction_hash: String,
}

type TradeKey = (String, String, i64, u64);

fn key_of(t: &RawTrade) -> TradeKey {
    (
        t.transaction_hash.clone(),
        t.asset.clone(),
        t.timestamp,
        t.size.to_bits(),
    )
}

async fn fetch_paginated(
    http: &reqwest::Client,
    user: &str,
    condition_id: &str,
    taker_only: bool,
) -> Result<Vec<RawTrade>> {
    let mut out = Vec::new();
    let mut offset: u32 = 0;

    loop {
        let page: Vec<RawTrade> = http
            .get(TRADES_URL)
            .query(&[
                ("user", user),
                ("market", condition_id),
                ("limit", &PAGE_SIZE.to_string()),
                ("offset", &offset.to_string()),
                ("takerOnly", if taker_only { "true" } else { "false" }),
            ])
            .send()
            .await
            .context("polymarket /trades request")?
            .error_for_status()
            .context("polymarket /trades status")?
            .json()
            .await
            .context("polymarket /trades json")?;

        let len = page.len();
        out.extend(page);
        if len < PAGE_SIZE as usize {
            break;
        }
        offset += PAGE_SIZE;
    }

    Ok(out)
}

pub async fn fetch_trades_for_market(
    http: &reqwest::Client,
    user: &str,
    condition_id: &str,
) -> Result<Vec<UserTrade>> {
    let (all, takers) = tokio::try_join!(
        fetch_paginated(http, user, condition_id, false),
        fetch_paginated(http, user, condition_id, true),
    )?;

    let taker_keys: HashSet<TradeKey> = takers.iter().map(key_of).collect();

    let mut out: Vec<UserTrade> = Vec::with_capacity(all.len());
    for raw in all {
        let is_taker = taker_keys.contains(&key_of(&raw));
        let ts = DateTime::from_timestamp(raw.timestamp, 0)
            .context("invalid trade timestamp")?
            .naive_utc();
        out.push(UserTrade {
            timestamp: ts,
            side: raw.side,
            outcome: raw.outcome,
            price: raw.price,
            size: raw.size,
            transaction_hash: raw.transaction_hash,
            is_taker,
        });
    }
    out.sort_by_key(|t| t.timestamp);
    Ok(out)
}
