use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime};
use tokio_postgres::NoTls;

use crate::model::{Market, OrderbookSnapshot, Resolution, UserTrade};

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
    pub pg: tokio_postgres::Client,
    pub http: reqwest::Client,
    pub http_base: String,
}

pub async fn connect(host: &str) -> Result<Db> {
    let pg_conn_str = format!("host={host} port=8812 user=admin password=quest dbname=qdb");
    let (pg, connection) = tokio_postgres::connect(&pg_conn_str, NoTls)
        .await
        .context(format!("Failed to connect to QuestDB at {host}"))?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("QuestDB connection error: {e}");
        }
    });

    Ok(Db {
        pg,
        http: reqwest::Client::new(),
        http_base: format!("http://{host}:9000"),
    })
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

    let query = format!(
        "SELECT condition_id, crypto, question, slug, end_date \
         FROM resolutions \
         WHERE timestamp >= cast({from_ts} as timestamp) \
           AND timestamp <= cast({to_ts} as timestamp) \
           {crypto_filter}\
         LATEST ON timestamp PARTITION BY condition_id \
         ORDER BY end_date DESC"
    );

    let rows = db.pg.query(&query, &[]).await.context("fetch_markets")?;

    let mut markets = Vec::new();
    for row in rows {
        let condition_id: &str = row.get(0);
        let crypto: &str = row.get(1);
        let question: &str = row.get(2);
        let slug: &str = row.get(3);
        let end_date: Option<NaiveDateTime> = row.try_get(4).ok();

        markets.push(Market {
            condition_id: condition_id.to_string(),
            crypto: crypto.to_string(),
            question: question.to_string(),
            slug: slug.to_string(),
            end_date,
        });
    }

    Ok(markets)
}

/// Fetch orderbook snapshots via QuestDB REST API (port 9000).
/// PG wire protocol can't deserialize QuestDB's 2D arrays, so we use HTTP+JSON.
pub async fn fetch_orderbook(
    db: &Db,
    condition_id: &str,
) -> Result<Vec<OrderbookSnapshot>> {
    let query = format!(
        "SELECT timestamp, outcome, bids, asks \
         FROM orderbook \
         WHERE condition_id = '{condition_id}' \
         ORDER BY timestamp ASC"
    );

    let url = format!(
        "{}/exec?query={}&fmt=json",
        db.http_base,
        urlencoded(&query)
    );

    let resp: serde_json::Value = db
        .http
        .get(&url)
        .send()
        .await
        .context("orderbook HTTP request")?
        .json::<serde_json::Value>()
        .await
        .context("orderbook JSON parse")?;

    if let Some(err) = resp.get("error") {
        anyhow::bail!("QuestDB error: {err}");
    }

    let dataset = resp["dataset"]
        .as_array()
        .context("missing dataset")?;

    let mut snapshots = Vec::new();
    for row in dataset {
        let arr: &Vec<serde_json::Value> = row.as_array().context("row not array")?;

        let ts_str = arr[0].as_str().context("timestamp")?;
        let timestamp = NaiveDateTime::parse_from_str(
            ts_str.trim_end_matches('Z'),
            "%Y-%m-%dT%H:%M:%S%.f",
        )
        .context("parse timestamp")?;

        let outcome = arr[1].as_str().context("outcome")?.to_string();

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

    let query = format!(
        "SELECT timestamp, side, outcome, price, size, transaction_hash \
         FROM user_trades \
         WHERE condition_id = '{condition_id}' \
           AND user_address IN ({addr_list}) \
         ORDER BY timestamp ASC"
    );

    let rows = db
        .pg
        .query(&query, &[])
        .await
        .context("fetch_user_trades")?;

    let mut trades = Vec::new();
    for row in rows {
        let timestamp: NaiveDateTime = row.get(0);
        let side: &str = row.get(1);
        let outcome: &str = row.get(2);
        let price: f64 = row.get(3);
        let size: f64 = row.get(4);
        let transaction_hash: &str = row.get(5);

        trades.push(UserTrade {
            timestamp,
            side: side.to_string(),
            outcome: outcome.to_string(),
            price,
            size,
            transaction_hash: transaction_hash.to_string(),
        });
    }

    Ok(trades)
}

pub async fn fetch_resolution(
    db: &Db,
    condition_id: &str,
) -> Result<Option<Resolution>> {
    let query = format!(
        "SELECT winning_outcome, yes_price, no_price \
         FROM resolutions \
         WHERE condition_id = '{condition_id}' \
         LATEST ON timestamp PARTITION BY condition_id"
    );

    let rows = db
        .pg
        .query(&query, &[])
        .await
        .context("fetch_resolution")?;

    if let Some(row) = rows.first() {
        Ok(Some(Resolution {
            winning_outcome: row.get::<_, &str>(0).to_string(),
            yes_price: row.get(1),
            no_price: row.get(2),
        }))
    } else {
        Ok(None)
    }
}
