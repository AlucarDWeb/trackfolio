use std::time::Duration;

use rust_decimal::Decimal;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct FxQuote {
    pub rate: Decimal,
    pub date: String,
}

pub fn eur_usd() -> Result<FxQuote, String> {
    let response = ureq::get("https://api.frankfurter.app/latest?from=EUR&to=USD")
        .timeout(Duration::from_secs(5))
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
