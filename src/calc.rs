use crate::model::Position;
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

pub fn row(p: &Position) -> RowCalc {
    let year = p.principal_usd * p.yield_pct / Decimal::from(100);
    RowCalc {
        year,
        month: year / Decimal::from(12),
        week: year / Decimal::from(52),
        day: year / Decimal::from(365),
    }
}

pub fn book(positions: &[Position]) -> BookCalc {
    let mut c = BookCalc {
        capital: Decimal::ZERO,
        book_yield: Decimal::ZERO,
        year: Decimal::ZERO,
        month: Decimal::ZERO,
        week: Decimal::ZERO,
        day: Decimal::ZERO,
    };
    for p in positions {
        let r = row(p);
        c.capital += p.principal_usd;
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

    fn d(v: i64) -> Decimal {
        Decimal::from(v)
    }

    fn approx_eq(a: Decimal, b: Decimal, eps: Decimal) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn row_projections_exact() {
        let r = row(&pos("50000", "5.12"));
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
        let b = book(&positions);
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
        let b = book(&[]);
        assert_eq!(b.capital, Decimal::ZERO);
        assert_eq!(b.book_yield, Decimal::ZERO);
        assert_eq!(b.year, Decimal::ZERO);
        assert_eq!(b.month, Decimal::ZERO);
        assert_eq!(b.week, Decimal::ZERO);
        assert_eq!(b.day, Decimal::ZERO);
    }

    #[test]
    fn book_single_row_yield_is_ratio() {
        let b = book(&[pos("50000", "5.12")]);
        assert_eq!(b.book_yield, Decimal::from_str_exact("5.12").unwrap() / d(100));
    }
}
