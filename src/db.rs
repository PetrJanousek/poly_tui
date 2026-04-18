use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime};

use crate::model::{Market, OrderbookSnapshot, Resolution, Trade, UserTrade};

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(b"0123456789ABCDEF"[(b >> 4) as usize]));
                out.push(char::from(b"0123456789ABCDEF"[(b & 0xf) as usize]));
            }
        }
    }
    out
}

pub struct Db {
    pub http: reqwest::Client,
    pub base_url: String,
}

pub async fn connect(host: &str) -> Result<Db> {
    let base_url = format!("http://{host}:9000");

    let http = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Quick connectivity check
    http.get(format!("{base_url}/exec?query={}&fmt=json", urlencoded("SELECT 1")))
        .send()
        .await
        .context(format!("Failed to connect to QuestDB at {host}:9000"))?;

    eprintln!("Connected to QuestDB at {base_url}");

    Ok(Db { http, base_url })
}

async fn query(db: &Db, sql: &str) -> Result<serde_json::Value> {
    let url = format!(
        "{}/exec?query={}&fmt=json",
        db.base_url,
        urlencoded(sql)
    );

    let resp: serde_json::Value = db
        .http
        .get(&url)
        .send()
        .await
        .context("HTTP request failed")?
        .json::<serde_json::Value>()
        .await
        .context("JSON parse failed")?;

    if let Some(err) = resp.get("error") {
        anyhow::bail!("QuestDB error: {err}");
    }

    Ok(resp)
}

fn get_str<'a>(row: &'a [serde_json::Value], idx: usize) -> &'a str {
    row[idx].as_str().unwrap_or("")
}

fn get_f64(row: &[serde_json::Value], idx: usize) -> f64 {
    row[idx].as_f64().unwrap_or(0.0)
}

fn parse_timestamp(s: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S%.f")
        .context("parse timestamp")
}

pub async fn fetch_markets(
    db: &Db,
    crypto: Option<&str>,
    date_from: NaiveDate,
    date_to: NaiveDate,
) -> Result<Vec<Market>> {
    let from_ts = date_from
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp_micros();
    let to_ts = date_to
        .and_hms_opt(23, 59, 59)
        .unwrap()
        .and_utc()
        .timestamp_micros();

    let crypto_filter = match crypto {
        Some(c) => format!("AND crypto = '{c}' "),
        None => String::new(),
    };

    let sql = format!(
        "SELECT condition_id, crypto, question, slug, end_date \
         FROM resolutions \
         WHERE timestamp >= cast({from_ts} as timestamp) \
           AND timestamp <= cast({to_ts} as timestamp) \
           {crypto_filter}\
         LATEST ON timestamp PARTITION BY condition_id \
         ORDER BY end_date DESC"
    );

    let resp = query(db, &sql).await.context("fetch_markets")?;
    let dataset = resp["dataset"].as_array().context("missing dataset")?;

    let mut markets = Vec::new();
    for row in dataset {
        let arr = row.as_array().context("row not array")?;
        let end_date = arr[4]
            .as_str()
            .and_then(|s| parse_timestamp(s).ok());

        markets.push(Market {
            condition_id: get_str(arr, 0).to_string(),
            crypto: get_str(arr, 1).to_string(),
            question: get_str(arr, 2).to_string(),
            slug: get_str(arr, 3).to_string(),
            end_date,
        });
    }

    Ok(markets)
}

pub async fn fetch_orderbook(
    db: &Db,
    condition_id: &str,
) -> Result<Vec<OrderbookSnapshot>> {
    // SAMPLE BY 1s reduces tens-of-thousands of deltas to ~1 row/sec per outcome,
    // keeping response size manageable while preserving 1-second replay fidelity.
    let sql = format!(
        "SELECT last(timestamp) as timestamp, outcome, last(bids) as bids, last(asks) as asks \
         FROM orderbook \
         WHERE condition_id = '{condition_id}' \
         SAMPLE BY 100T ALIGN TO CALENDAR \
         ORDER BY timestamp ASC"
    );

    let resp = query(db, &sql).await.context("fetch_orderbook")?;
    let dataset = resp["dataset"].as_array().context("missing dataset")?;

    let mut snapshots = Vec::new();
    for row in dataset {
        let arr = row.as_array().context("row not array")?;
        let timestamp = parse_timestamp(get_str(arr, 0))?;
        let outcome = get_str(arr, 1).to_string();

        let bids = parse_json_2d_array(&arr[2]);
        let asks = parse_json_2d_array(&arr[3]);

        snapshots.push(OrderbookSnapshot {
            timestamp,
            outcome,
            bid_prices: bids.first().cloned().unwrap_or_default(),
            bid_sizes: bids.get(1).cloned().unwrap_or_default(),
            ask_prices: asks.first().cloned().unwrap_or_default(),
            ask_sizes: asks.get(1).cloned().unwrap_or_default(),
        });
    }

    eprintln!("fetch_orderbook: {} snapshots", snapshots.len());
    Ok(snapshots)
}

fn parse_json_2d_array(val: &serde_json::Value) -> Vec<Vec<f64>> {
    val.as_array()
        .map(|outer| {
            outer
                .iter()
                .map(|inner| {
                    inner
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default()
}

pub async fn fetch_market_trades(db: &Db, condition_id: &str) -> Result<Vec<Trade>> {
    let sql = format!(
        "SELECT timestamp, outcome, side, price, size, transaction_hash \
         FROM trades \
         WHERE condition_id = '{condition_id}' \
         ORDER BY timestamp ASC"
    );

    let resp = query(db, &sql).await.context("fetch_market_trades")?;
    let dataset = resp["dataset"].as_array().context("missing dataset")?;

    let mut trades = Vec::new();
    for row in dataset {
        let arr = row.as_array().context("row not array")?;
        let timestamp = parse_timestamp(get_str(arr, 0))?;
        trades.push(Trade {
            timestamp,
            outcome: get_str(arr, 1).to_string(),
            side: get_str(arr, 2).to_string(),
            price: get_f64(arr, 3),
            size: get_f64(arr, 4),
            transaction_hash: get_str(arr, 5).to_string(),
            is_user: false,
            is_taker: None,
        });
    }

    Ok(trades)
}

pub async fn fetch_user_trades(
    db: &Db,
    condition_id: &str,
    user_addresses: &[String],
) -> Result<Vec<UserTrade>> {
    if user_addresses.is_empty() {
        return Ok(vec![]);
    }

    let addresses: Vec<String> = user_addresses.iter().map(|a| format!("'{a}'")).collect();
    let addr_list = addresses.join(",");

    let sql = format!(
        "SELECT timestamp, side, outcome, price, size, transaction_hash \
         FROM user_trades \
         WHERE condition_id = '{condition_id}' \
           AND user_address IN ({addr_list}) \
         ORDER BY timestamp ASC"
    );

    let resp = query(db, &sql).await.context("fetch_user_trades")?;
    let dataset = resp["dataset"].as_array().context("missing dataset")?;

    let mut trades = Vec::new();
    for row in dataset {
        let arr = row.as_array().context("row not array")?;
        let timestamp = parse_timestamp(get_str(arr, 0))?;

        trades.push(UserTrade {
            timestamp,
            side: get_str(arr, 1).to_string(),
            outcome: get_str(arr, 2).to_string(),
            price: get_f64(arr, 3),
            size: get_f64(arr, 4),
            transaction_hash: get_str(arr, 5).to_string(),
            is_taker: false,
        });
    }

    Ok(trades)
}

pub async fn fetch_resolution(
    db: &Db,
    condition_id: &str,
) -> Result<Option<Resolution>> {
    let sql = format!(
        "SELECT winning_outcome, yes_price, no_price \
         FROM resolutions \
         WHERE condition_id = '{condition_id}' \
         LATEST ON timestamp PARTITION BY condition_id"
    );

    let resp = query(db, &sql).await.context("fetch_resolution")?;
    let dataset = resp["dataset"].as_array().context("missing dataset")?;

    if let Some(row) = dataset.first() {
        let arr = row.as_array().context("row not array")?;
        Ok(Some(Resolution {
            winning_outcome: get_str(arr, 0).to_string(),
            yes_price: get_f64(arr, 1),
            no_price: get_f64(arr, 2),
        }))
    } else {
        Ok(None)
    }
}
