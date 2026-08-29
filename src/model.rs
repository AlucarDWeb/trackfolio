use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Tbill,
    Deposit,
    Other,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub currency: String,
    pub positions: Vec<Position>,
}
