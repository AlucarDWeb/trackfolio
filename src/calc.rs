use crate::model::{Kind, Position};
use chrono::NaiveDate;
use rust_decimal::Decimal;

pub struct RowCalc {
    pub year: Decimal,
    pub month: Decimal,
    pub week: Decimal,
    pub day: Decimal,
}

pub struct BookCalc {
    pub capital: Decimal,
    pub book_yield: Decimal,
    pub year: Decimal,
    pub month: Decimal,
    pub week: Decimal,
    pub day: Decimal,
}

pub fn current_value(p: &Position, today: NaiveDate) -> Decimal {
    if p.kind != Kind::Deposit {
        return p.principal_usd;
    }
    let start = match p
        .start_date
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
    {
        Some(s) => s,
        None => return p.principal_usd,
    };
    let n = (today - start).num_days();
    if n <= 0 {
        return p.principal_usd;
    }
    let factor = Decimal::from(1) + p.yield_pct / Decimal::from(100) / Decimal::from(365);
    let mut value = p.principal_usd;
    for _ in 0..n {
        value *= factor;
    }
    value
}

pub fn row(p: &Position, today: NaiveDate) -> RowCalc {
    let year = current_value(p, today) * p.yield_pct / Decimal::from(100);
    RowCalc {
        year,
        month: year / Decimal::from(12),
        week: year / Decimal::from(52),
        day: year / Decimal::from(365),
    }
}

pub fn book(positions: &[Position], today: NaiveDate) -> BookCalc {
    let mut c = BookCalc {
        capital: Decimal::ZERO,
        book_yield: Decimal::ZERO,
        year: Decimal::ZERO,
        month: Decimal::ZERO,
        week: Decimal::ZERO,
        day: Decimal::ZERO,
    };
    for p in positions {
        let r = row(p, today);
        c.capital += current_value(p, today);
        c.year += r.year;
        c.month += r.month;
        c.week += r.week;
        c.day += r.day;
    }
    if c.capital > Decimal::ZERO {
        c.book_yield = c.year / c.capital;
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Kind, Position};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn pos(principal: &str, yield_pct: &str) -> Position {
        Position {
            id: ulid::Ulid::new(),
            kind: Kind::Tbill,
            name: "test".to_string(),
            principal_usd: Decimal::from_str(principal).unwrap(),
            yield_pct: Decimal::from_str(yield_pct).unwrap(),
            maturity: None,
            start_date: None,
            source_ccy: "USD".to_string(),
            source_amount: None,
            fx_rate: None,
            fx_date: None,
        }
    }

    fn pos_with(kind: Kind, principal: &str, yield_pct: &str, start: Option<&str>) -> Position {
        Position {
            id: ulid::Ulid::new(),
            kind,
            name: "test".to_string(),
            principal_usd: Decimal::from_str(principal).unwrap(),
            yield_pct: Decimal::from_str(yield_pct).unwrap(),
            maturity: None,
            start_date: start.map(|s| s.to_string()),
            source_ccy: "USD".to_string(),
            source_amount: None,
            fx_rate: None,
            fx_date: None,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 30).unwrap()
    }

    fn oracle(principal: Decimal, yield_pct: Decimal, n: i64) -> Decimal {
        let factor = Decimal::from(1) + yield_pct / Decimal::from(100) / Decimal::from(365);
        let mut v = principal;
        for _ in 0..n {
            v = v * factor;
        }
        v
    }

    fn d(v: i64) -> Decimal {
        Decimal::from(v)
    }

    fn approx_eq(a: Decimal, b: Decimal, eps: Decimal) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn deposit_compounds_daily_from_start_date() {
        let today = today();
        let start = today - chrono::Duration::days(365);
        let p = pos_with(Kind::Deposit, "30000", "3.8", Some(&start.to_string()));
        let expected = oracle(p.principal_usd, p.yield_pct, 365);
        assert_eq!(current_value(&p, today), expected);
        assert!(current_value(&p, today) > p.principal_usd);
    }

    #[test]
    fn deposit_start_today_is_nominal() {
        let today = today();
        let p = pos_with(Kind::Deposit, "30000", "3.8", Some(&today.to_string()));
        assert_eq!(current_value(&p, today), Decimal::from_str("30000").unwrap());
    }

    #[test]
    fn deposit_future_start_is_nominal() {
        let today = today();
        let start = today + chrono::Duration::days(1);
        let p = pos_with(Kind::Deposit, "30000", "3.8", Some(&start.to_string()));
        assert_eq!(current_value(&p, today), Decimal::from_str("30000").unwrap());
    }

    #[test]
    fn deposit_without_start_date_is_nominal() {
        let today = today();
        let p = pos_with(Kind::Deposit, "30000", "3.8", None);
        assert_eq!(current_value(&p, today), Decimal::from_str("30000").unwrap());
    }

    #[test]
    fn deposit_invalid_start_date_is_nominal() {
        let today = today();
        let p = pos_with(Kind::Deposit, "30000", "3.8", Some("not-a-date"));
        assert_eq!(current_value(&p, today), Decimal::from_str("30000").unwrap());
    }

    #[test]
    fn tbill_with_start_date_is_nominal() {
        let today = today();
        let start = today - chrono::Duration::days(365);
        let p = pos_with(Kind::Tbill, "30000", "3.8", Some(&start.to_string()));
        assert_eq!(current_value(&p, today), Decimal::from_str("30000").unwrap());
    }

    #[test]
    fn deposit_row_projects_off_grown_value() {
        let today = today();
        let start = today - chrono::Duration::days(365);
        let p = pos_with(Kind::Deposit, "30000", "3.8", Some(&start.to_string()));
        let r = row(&p, today);
        let expected_year = current_value(&p, today) * p.yield_pct / d(100);
        assert_eq!(r.year, expected_year);
        assert_eq!(r.month, expected_year / d(12));
        assert_eq!(r.week, expected_year / d(52));
        assert_eq!(r.day, expected_year / d(365));
    }

    #[test]
    fn deposit_book_uses_grown_capital() {
        let today = today();
        let start = today - chrono::Duration::days(365);
        let p = pos_with(Kind::Deposit, "30000", "3.8", Some(&start.to_string()));
        let b = book(&[p.clone()], today);
        let grown = current_value(&p, today);
        assert_eq!(b.capital, grown);
        assert_eq!(b.year, grown * p.yield_pct / d(100));
        assert_eq!(b.book_yield, b.year / b.capital);
    }

    #[test]
    fn row_projections_exact() {
        let r = row(&pos("50000", "5.12"), today());
        assert_eq!(r.year, Decimal::from_str_exact("2560").unwrap());
        assert_eq!(r.month, d(2560) / d(12));
        assert_eq!(r.week, d(2560) / d(52));
        assert_eq!(r.day, d(2560) / d(365));
    }

    #[test]
    fn book_three_positions_weighted_yield() {
        let positions = vec![
            pos("50000", "5.12"),
            pos("30000", "3.80"),
            pos("40000", "4.00"),
        ];
        let b = book(&positions, today());
        assert_eq!(b.capital, d(120000));
        assert_eq!(b.book_yield, d(5300) / d(120000));
        assert_eq!(b.year, d(5300));
        let eps = Decimal::new(1, 24);
        assert!(approx_eq(b.day * d(365), b.year, eps));
        assert!(approx_eq(b.month * d(12), b.year, eps));
        assert!(approx_eq(b.week * d(52), b.year, eps));
    }

    #[test]
    fn book_empty_is_all_zero() {
        let b = book(&[], today());
        assert_eq!(b.capital, Decimal::ZERO);
        assert_eq!(b.book_yield, Decimal::ZERO);
        assert_eq!(b.year, Decimal::ZERO);
        assert_eq!(b.month, Decimal::ZERO);
        assert_eq!(b.week, Decimal::ZERO);
        assert_eq!(b.day, Decimal::ZERO);
    }

    #[test]
    fn book_single_row_yield_is_ratio() {
        let b = book(&[pos("50000", "5.12")], today());
        assert_eq!(b.book_yield, Decimal::from_str_exact("5.12").unwrap() / d(100));
    }
}
