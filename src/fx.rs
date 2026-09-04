use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;

use rust_decimal::Decimal;
use serde_json::Value;

const EUR_USD_URL: &str = "https://api.frankfurter.dev/v1/latest?from=EUR&to=USD";

#[derive(Debug, Clone, PartialEq)]
pub struct FxQuote {
    pub rate: Decimal,
    pub date: String,
}

fn prefer_ipv4(mut addrs: Vec<SocketAddr>) -> Vec<SocketAddr> {
    addrs.sort_by_key(|a| !a.is_ipv4());
    addrs
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .resolver(|netloc: &str| {
            netloc
                .to_socket_addrs()
                .map(|iter| prefer_ipv4(iter.collect()))
        })
        .build()
}

fn fetch(url: &str) -> Result<String, String> {
    let response = agent()
        .get(url)
        .call()
        .map_err(|e| format!("FX request failed: {e}"))?;
    response
        .into_string()
        .map_err(|e| format!("FX request failed: could not read body: {e}"))
}

pub fn eur_usd() -> Result<FxQuote, String> {
    parse_quote(&fetch(EUR_USD_URL)?)
}

pub fn board_series_url(today: chrono::NaiveDate) -> String {
    let start = today - chrono::Duration::days(10);
    format!("https://api.frankfurter.dev/v1/{start}..?from=EUR&to=USD,JPY")
}

pub fn eur_board() -> Result<FxBoard, String> {
    let url = board_series_url(chrono::Utc::now().date_naive());
    parse_series(&fetch(&url)?)
}

pub fn eur_to_usd(amount: Decimal, q: &FxQuote) -> Decimal {
    amount * q.rate
}

fn rate_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            let v = value.clone();
            if v.is_null() {
                None
            } else {
                Some(v.to_string())
            }
        })
}

fn decimal_rate(rate_value: &str) -> Result<Decimal, String> {
    Decimal::from_str_exact(rate_value)
        .map_err(|e| format!("FX rate \"{rate_value}\" is not a valid decimal: {e}"))
}

pub fn pct_change(curr: Decimal, prev: Decimal) -> Option<Decimal> {
    if prev.is_zero() {
        None
    } else {
        Some((curr - prev) / prev * Decimal::from(100))
    }
}

fn optional_rate(value: &Value) -> Result<Option<Decimal>, String> {
    match rate_string(value) {
        None => Ok(None),
        Some(s) => Ok(Some(decimal_rate(&s)?)),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FxBoard {
    pub usd: Option<Decimal>,
    pub jpy: Option<Decimal>,
    pub usd_pct: Option<Decimal>,
    pub jpy_pct: Option<Decimal>,
    pub date: Option<String>,
}

pub fn parse_board(body: &str) -> Result<FxBoard, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| format!("FX response is not valid JSON: {e}"))?;
    let date = value["date"]
        .as_str()
        .ok_or_else(|| "FX response missing \"date\"".to_string())?;
    let usd = match rate_string(&value["rates"]["USD"]) {
        None => None,
        Some(s) => Some(decimal_rate(&s)?),
    };
    let jpy = match rate_string(&value["rates"]["JPY"]) {
        None => None,
        Some(s) => Some(decimal_rate(&s)?),
    };
    Ok(FxBoard {
        usd,
        jpy,
        usd_pct: None,
        jpy_pct: None,
        date: Some(date.to_string()),
    })
}

pub fn parse_series(body: &str) -> Result<FxBoard, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| format!("FX response is not valid JSON: {e}"))?;
    let rates = value["rates"]
        .as_object()
        .ok_or_else(|| "FX response missing \"rates\"".to_string())?;
    let mut dates: Vec<&String> = rates.keys().collect();
    dates.sort();
    let curr_date = dates
        .last()
        .ok_or_else(|| "FX response has no rate dates".to_string())?;
    let curr = &rates[*curr_date];
    let usd = optional_rate(&curr["USD"])?;
    let jpy = optional_rate(&curr["JPY"])?;
    let (usd_pct, jpy_pct) = match dates.len().checked_sub(2).map(|i| &rates[dates[i]]) {
        Some(prev) => {
            let prev_usd = optional_rate(&prev["USD"])?;
            let prev_jpy = optional_rate(&prev["JPY"])?;
            (
                usd.zip(prev_usd).and_then(|(c, p)| pct_change(c, p)),
                jpy.zip(prev_jpy).and_then(|(c, p)| pct_change(c, p)),
            )
        }
        None => (None, None),
    };
    Ok(FxBoard {
        usd,
        jpy,
        usd_pct,
        jpy_pct,
        date: Some(curr_date.to_string()),
    })
}

pub fn parse_quote(body: &str) -> Result<FxQuote, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| format!("FX response is not valid JSON: {e}"))?;
    let date = value["date"]
        .as_str()
        .ok_or_else(|| "FX response missing \"date\"".to_string())?;
    let rate_value = rate_string(&value["rates"]["USD"])
        .ok_or_else(|| "FX response missing \"rates.USD\"".to_string())?;
    let rate = decimal_rate(&rate_value)?;
    Ok(FxQuote {
        rate,
        date: date.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "amount": 1.0,
        "base": "EUR",
        "date": "2026-08-28",
        "rates": { "USD": 1.0842 }
    }"#;

    #[test]
    fn prefer_ipv4_sorts_v4_before_v6() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
        let v6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 443);
        let v4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 443);
        let out = prefer_ipv4(vec![v6, v4]);
        assert!(out[0].is_ipv4());
        assert!(out[1].is_ipv6());
    }

    #[test]
    fn eur_usd_url_is_frankfurter_dev_v1() {
        assert_eq!(
            EUR_USD_URL,
            "https://api.frankfurter.dev/v1/latest?from=EUR&to=USD"
        );
        assert!(!EUR_USD_URL.contains("frankfurter.app"));
    }

    #[test]
    fn parses_fixture_rate_and_date() {
        let q = parse_quote(FIXTURE).unwrap();
        assert_eq!(q.rate, Decimal::from_str_exact("1.0842").unwrap());
        assert_eq!(q.date, "2026-08-28");
    }

    #[test]
    fn string_rate_parses() {
        let q = parse_quote(r#"{"date": "2026-08-28", "rates": {"USD": "1.0842"}}"#).unwrap();
        assert_eq!(q.rate, Decimal::from_str_exact("1.0842").unwrap());
    }

    #[test]
    fn missing_rates_is_err() {
        assert!(parse_quote(r#"{"date": "2026-08-28"}"#).is_err());
    }

    #[test]
    fn missing_usd_rate_is_err() {
        assert!(parse_quote(r#"{"date": "2026-08-28", "rates": {"CHF": 0.9}}"#).is_err());
    }

    #[test]
    fn non_numeric_rate_is_err() {
        assert!(parse_quote(r#"{"date": "2026-08-28", "rates": {"USD": "abc"}}"#).is_err());
    }

    #[test]
    fn malformed_json_is_err() {
        assert!(parse_quote("{not json").is_err());
    }

    #[test]
    fn missing_date_is_err() {
        assert!(parse_quote(r#"{"rates": {"USD": 1.0842}}"#).is_err());
    }

    #[test]
    fn eur_to_usd_is_exact() {
        let q = FxQuote {
            rate: Decimal::from_str_exact("1.0842").unwrap(),
            date: "2026-08-28".to_string(),
        };
        let amount = Decimal::from_str_exact("10000").unwrap();
        assert_eq!(eur_to_usd(amount, &q), Decimal::from_str_exact("10842").unwrap());
    }

    const BOARD_FIXTURE: &str = r#"{
        "amount": 1.0,
        "base": "EUR",
        "date": "2026-08-28",
        "rates": { "USD": 1.0842, "JPY": 157.32 }
    }"#;

    #[test]
    fn board_series_url_is_frankfurter_dev_range() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
        let url = board_series_url(today);
        assert_eq!(
            url,
            "https://api.frankfurter.dev/v1/2026-08-25..?from=EUR&to=USD,JPY"
        );
        assert!(!url.contains("frankfurter.app"));
    }

    #[test]
    fn pct_change_positive_and_negative() {
        let up = pct_change(Decimal::from(110), Decimal::from(100));
        assert_eq!(up, Some(Decimal::from(10)));
        let down = pct_change(Decimal::from(90), Decimal::from(100));
        assert_eq!(down, Some(Decimal::from(-10)));
    }

    #[test]
    fn pct_change_zero_prev_is_none() {
        assert_eq!(
            pct_change(Decimal::from_str_exact("1.16").unwrap(), Decimal::ZERO),
            None
        );
    }

    const SERIES_FIXTURE: &str = r#"{
        "amount": 1.0,
        "base": "EUR",
        "start_date": "2026-08-25",
        "end_date": "2026-09-03",
        "rates": {
            "2026-09-02": { "JPY": 184.78, "USD": 1.1578 },
            "2026-09-03": { "JPY": 181.21, "USD": 1.1615 }
        }
    }"#;

    #[test]
    fn parse_series_sets_current_and_pct() {
        let b = parse_series(SERIES_FIXTURE).unwrap();
        assert_eq!(b.usd, Some(Decimal::from_str_exact("1.1615").unwrap()));
        assert_eq!(b.jpy, Some(Decimal::from_str_exact("181.21").unwrap()));
        assert_eq!(b.date.as_deref(), Some("2026-09-03"));
        assert_eq!(
            b.usd_pct,
            pct_change(
                Decimal::from_str_exact("1.1615").unwrap(),
                Decimal::from_str_exact("1.1578").unwrap(),
            )
        );
        assert_eq!(
            b.jpy_pct,
            pct_change(
                Decimal::from_str_exact("181.21").unwrap(),
                Decimal::from_str_exact("184.78").unwrap(),
            )
        );
    }

    #[test]
    fn parse_series_single_day_has_no_pct() {
        let b = parse_series(r#"{"rates":{"2026-09-03":{"USD":1.1615,"JPY":181.21}}}"#).unwrap();
        assert_eq!(b.usd_pct, None);
        assert_eq!(b.jpy_pct, None);
        assert_eq!(b.date.as_deref(), Some("2026-09-03"));
    }

    #[test]
    fn parse_series_empty_rates_is_err() {
        assert!(parse_series(r#"{"rates":{}}"#).is_err());
        assert!(parse_series("{not json").is_err());
    }

    #[test]
    fn parse_board_reads_usd_jpy_and_date() {
        let b = parse_board(BOARD_FIXTURE).unwrap();
        assert_eq!(b.usd, Some(Decimal::from_str_exact("1.0842").unwrap()));
        assert_eq!(b.jpy, Some(Decimal::from_str_exact("157.32").unwrap()));
        assert_eq!(b.date.as_deref(), Some("2026-08-28"));
    }

    #[test]
    fn parse_board_string_rates() {
        let b = parse_board(
            r#"{"date":"2026-08-28","rates":{"USD":"1.0842","JPY":"157.32"}}"#,
        )
        .unwrap();
        assert_eq!(b.usd, Some(Decimal::from_str_exact("1.0842").unwrap()));
        assert_eq!(b.jpy, Some(Decimal::from_str_exact("157.32").unwrap()));
    }

    #[test]
    fn parse_board_missing_usd_keeps_jpy_and_date() {
        let b = parse_board(r#"{"date":"2026-08-28","rates":{"JPY":157.32}}"#).unwrap();
        assert_eq!(b.usd, None);
        assert_eq!(b.jpy, Some(Decimal::from_str_exact("157.32").unwrap()));
        assert_eq!(b.date.as_deref(), Some("2026-08-28"));
    }

    #[test]
    fn parse_board_missing_jpy_keeps_usd_and_date() {
        let b = parse_board(r#"{"date":"2026-08-28","rates":{"USD":1.0842}}"#).unwrap();
        assert_eq!(b.usd, Some(Decimal::from_str_exact("1.0842").unwrap()));
        assert_eq!(b.jpy, None);
        assert_eq!(b.date.as_deref(), Some("2026-08-28"));
    }

    #[test]
    fn parse_board_missing_date_is_err() {
        assert!(parse_board(r#"{"rates":{"USD":1.0842,"JPY":157.32}}"#).is_err());
    }

    #[test]
    fn parse_board_malformed_json_is_err() {
        assert!(parse_board("{not json").is_err());
    }

    #[test]
    fn eur_to_usd_keeps_full_precision() {
        let q = FxQuote {
            rate: Decimal::from_str_exact("1.0842").unwrap(),
            date: "2026-08-28".to_string(),
        };
        let amount = Decimal::from_str_exact("12345.67").unwrap();
        assert_eq!(
            eur_to_usd(amount, &q),
            Decimal::from_str_exact("13385.175414").unwrap()
        );
    }
}
