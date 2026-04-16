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
    #[serde(rename = "transactionHash")]
    transaction_hash: String,
}

pub async fn fetch_trades_for_market(
    http: &reqwest::Client,
    user: &str,
    condition_id: &str,
) -> Result<Vec<UserTrade>> {
    let mut out = Vec::new();
    let mut offset: u32 = 0;

    loop {
        let raw: Vec<RawTrade> = http
            .get(TRADES_URL)
            .query(&[
                ("user", user),
                ("market", condition_id),
                ("limit", &PAGE_SIZE.to_string()),
                ("offset", &offset.to_string()),
                ("takerOnly", "false"),
            ])
            .send()
            .await
            .context("polymarket /trades request")?
            .error_for_status()
            .context("polymarket /trades status")?
            .json()
            .await
            .context("polymarket /trades json")?;

        let page_len = raw.len();
        for t in raw {
            let ts = DateTime::from_timestamp(t.timestamp, 0)
                .context("invalid trade timestamp")?
                .naive_utc();
            out.push(UserTrade {
                timestamp: ts,
                side: t.side,
                outcome: t.outcome,
                price: t.price,
                size: t.size,
                transaction_hash: t.transaction_hash,
            });
        }

        if page_len < PAGE_SIZE as usize {
            break;
        }
        offset += PAGE_SIZE;
    }

    out.sort_by_key(|t| t.timestamp);
    Ok(out)
}
