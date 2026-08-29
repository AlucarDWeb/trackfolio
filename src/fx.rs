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

pub fn eur_usd() -> Result<FxQuote, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .resolver(|netloc: &str| {
            netloc
                .to_socket_addrs()
                .map(|iter| prefer_ipv4(iter.collect()))
        })
        .build();
    let response = agent
        .get(EUR_USD_URL)
        .call()
        .map_err(|e| format!("FX request failed: {e}"))?;
    let body = response
        .into_string()
        .map_err(|e| format!("FX request failed: could not read body: {e}"))?;
    parse_quote(&body)
}

pub fn eur_to_usd(amount: Decimal, q: &FxQuote) -> Decimal {
    amount * q.rate
}

pub fn parse_quote(body: &str) -> Result<FxQuote, String> {
    let value: Value =
        serde_json::from_str(body).map_err(|e| format!("FX response is not valid JSON: {e}"))?;
    let date = value["date"]
        .as_str()
        .ok_or_else(|| "FX response missing \"date\"".to_string())?;
    let rate_value = value["rates"]["USD"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| {
            let v = value["rates"]["USD"].clone();
            if v.is_null() {
                None
            } else {
                Some(v.to_string())
            }
        })
        .ok_or_else(|| "FX response missing \"rates.USD\"".to_string())?;
    let rate = Decimal::from_str_exact(&rate_value)
        .map_err(|e| format!("FX rate \"{rate_value}\" is not a valid decimal: {e}"))?;
    Ok(FxQuote {
        rate,
        date: date.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

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
