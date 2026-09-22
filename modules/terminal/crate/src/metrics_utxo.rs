//! BRK daily observations. Dates come from the provider, never from guessed offsets.
use super::{json, number, Row};
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn fields(source: &str) -> &'static [&'static str] {
    match source {
        "utxo-profit" => &[
            "sopr_24h",
            "asopr_24h",
            "lth_sopr_24h",
            "sth_sopr_24h",
            "realized_profit_sum_24h",
            "realized_loss_sum_24h",
            "net_realized_pnl_sum_24h",
            "unrealized_profit_to_mcap_ratio",
            "unrealized_loss_to_mcap_ratio",
        ],
        "utxo-supply" => &[
            "supply_in_profit_share",
            "supply_in_loss_share",
            "lth_supply",
            "sth_supply",
            "utxos_over_1y_old_supply",
            "sth_supply_in_profit_share",
            "lth_supply_in_profit_share",
        ],
        "utxo-value" => &[
            "sth_realized_price",
            "lth_realized_price",
            "sth_mvrv",
            "lth_mvrv",
            "sth_nupl",
            "lth_nupl",
            "thermo_cap",
            "thermo_cap_multiple",
            "investor_cap",
        ],
        "utxo-activity" => &[
            "liveliness",
            "dormancy_24h",
            "coindays_destroyed_sum_24h",
            "reserve_risk",
            "rhodl_ratio",
            "new_addr_count_sum_24h",
        ],
        _ => &[],
    }
}

fn parse(body: &Value, fields: &[&str], today: i64) -> Result<Vec<Row>, String> {
    let series = body.as_array().ok_or("BRK returned no series array")?;
    if series.len() != fields.len() + 1 {
        return Err("BRK returned an incomplete series batch".into());
    }
    let dates = &series[0];
    let start = dates["start"].as_u64().ok_or("BRK missing start index")?;
    let end = dates["end"].as_u64().ok_or("BRK missing end index")?;
    let dates_data = dates["data"].as_array().ok_or("BRK missing dates")?;
    if dates["index"] != "day1" || end < start || end - start != dates_data.len() as u64 {
        return Err("BRK date range is truncated or not daily".into());
    }
    for item in &series[1..] {
        if item["index"] != "day1"
            || item["start"].as_u64() != Some(start)
            || item["end"].as_u64() != Some(end)
            || item["data"].as_array().map(Vec::len) != Some(dates_data.len())
        {
            return Err("BRK series/date alignment mismatch".into());
        }
    }
    let mut rows = Vec::new();
    for (i, date) in dates_data.iter().enumerate() {
        let time = NaiveDate::parse_from_str(date.as_str().ok_or("BRK invalid date")?, "%Y-%m-%d")
            .map_err(|e| format!("BRK date: {e}"))?
            .and_hms_opt(0, 0, 0)
            .ok_or("BRK invalid midnight")?
            .and_utc()
            .timestamp();
        if time >= today {
            continue;
        }
        let values: BTreeMap<String, f64> = fields
            .iter()
            .enumerate()
            .filter_map(|(j, field)| {
                number(&series[j + 1]["data"][i]).map(|v| ((*field).into(), v))
            })
            .collect();
        if !values.is_empty() {
            rows.push(Row {
                time,
                values,
                provisional: vec![],
            });
        }
    }
    Ok(rows)
}

pub async fn history(source: &str) -> Result<Vec<Row>, String> {
    let fields = fields(source);
    let now = Utc::now();
    let mut result: BTreeMap<i64, Row> = BTreeMap::new();
    // Respect the provider's 320,000 request-weight limit (daily values cost 10 each).
    for chunk in fields.chunks(3) {
        let names = std::iter::once("date")
            .chain(chunk.iter().copied())
            .collect::<Vec<_>>()
            .join(",");
        let body = json(
            "https://bitview.space/api/series/bulk",
            &[
                ("series", names),
                ("index", "day1".into()),
                ("start", "2009-01-01".into()),
                ("end", now.format("%Y-%m-%d").to_string()),
                ("limit", "10000".into()),
            ],
        )
        .await?;
        let rows = parse(&body, chunk, now.timestamp().div_euclid(86400) * 86400)?;
        if body[0]["data"][0] != "2009-01-01" {
            return Err("BRK omitted the requested start of history".into());
        }
        for row in rows {
            result
                .entry(row.time)
                .or_insert_with(|| Row {
                    time: row.time,
                    values: BTreeMap::new(),
                    provisional: vec![],
                })
                .values
                .extend(row.values);
        }
    }
    Ok(result.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exact_dates_missing_values_and_signed_pnl() {
        let body = json!([
            {"index":"day1","start":730,"end":733,"data":["2011-01-01","2011-01-02","2011-01-03"]},
            {"index":"day1","start":730,"end":733,"data":[null,-5,0]}
        ]);
        let rows = parse(&body, &["pnl"], 1294099200).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].time, 1293926400);
        assert_eq!(rows[0].values["pnl"], -5.0);
        assert_eq!(rows[1].values["pnl"], 0.0);
        let mut bad = body.clone();
        bad[1]["start"] = json!(729);
        assert!(parse(&bad, &["pnl"], i64::MAX).is_err());
    }
}
