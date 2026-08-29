use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    const FIXTURE: &str = r#"{
        "id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        "kind": "tbill",
        "name": "T-Bill 4 weeks",
        "principal_usd": "50000.00",
        "yield_pct": "5.12",
        "maturity": "2026-09-26",
        "source_ccy": "USD",
        "source_amount": null,
        "fx_rate": null,
        "fx_date": null
    }"#;

    fn book_with(p: Position) -> Book {
        Book {
            currency: "USD".to_string(),
            positions: vec![p],
        }
    }

    #[test]
    fn deserializes_schema_fixture() {
        let p: Position = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(
            p.principal_usd,
            Decimal::from_str_exact("50000.00").unwrap()
        );
        assert_eq!(p.yield_pct, Decimal::from_str_exact("5.12").unwrap());
        assert_eq!(p.maturity.as_deref(), Some("2026-09-26"));
        assert_eq!(p.source_ccy, "USD");
        assert_eq!(p.source_amount, None);
        assert_eq!(p.fx_rate, None);
        assert_eq!(p.fx_date, None);
    }

    #[test]
    fn book_roundtrip_is_identical() {
        let p: Position = serde_json::from_str(FIXTURE).unwrap();
        let book = book_with(p);
        let json = serde_json::to_string(&book).unwrap();
        assert!(json.contains("\"principal_usd\":\"50000.00\""));
        assert!(json.contains("\"yield_pct\":\"5.12\""));
        let back: Book = serde_json::from_str(&json).unwrap();
        assert_eq!(back, book);
    }

    #[test]
    fn null_maturity_deserializes_to_none() {
        let json = FIXTURE.replace("\"2026-09-26\"", "null");
        let p: Position = serde_json::from_str(&json).unwrap();
        assert_eq!(p.maturity, None);
    }

    #[test]
    fn kind_roundtrip_all_variants() {
        for k in [Kind::Tbill, Kind::Deposit, Kind::Other] {
            let json = serde_json::to_string(&k).unwrap();
            let back: Kind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, k);
        }
        assert_eq!(serde_json::to_string(&Kind::Tbill).unwrap(), "\"tbill\"");
        assert_eq!(
            serde_json::to_string(&Kind::Deposit).unwrap(),
            "\"deposit\""
        );
        assert_eq!(serde_json::to_string(&Kind::Other).unwrap(), "\"other\"");
    }

    #[test]
    fn eur_source_fields_deserialize() {
        let json = FIXTURE
            .replace("tbill", "deposit")
            .replace("\"source_ccy\": \"USD\"", "\"source_ccy\": \"EUR\"")
            .replace("\"source_amount\": null", "\"source_amount\": \"45000.00\"")
            .replace("\"fx_rate\": null", "\"fx_rate\": \"1.0850\"")
            .replace("\"fx_date\": null", "\"fx_date\": \"2026-08-29\"");
        let p: Position = serde_json::from_str(&json).unwrap();
        assert_eq!(p.source_amount, Some(Decimal::from_str_exact("45000.00").unwrap()));
        assert_eq!(p.fx_rate, Some(Decimal::from_str_exact("1.0850").unwrap()));
        assert_eq!(p.fx_date.as_deref(), Some("2026-08-29"));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Tbill,
    Deposit,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub id: ulid::Ulid,
    pub kind: Kind,
    pub name: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub principal_usd: rust_decimal::Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub yield_pct: rust_decimal::Decimal,
    pub maturity: Option<String>,
    pub source_ccy: String,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub source_amount: Option<rust_decimal::Decimal>,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub fx_rate: Option<rust_decimal::Decimal>,
    pub fx_date: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Book {
    pub currency: String,
    pub positions: Vec<Position>,
}
