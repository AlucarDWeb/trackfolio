use crate::model::Book;

pub struct Totals {
    pub principal: rust_decimal::Decimal,
    pub yearly_interest: rust_decimal::Decimal,
    pub weighted_yield: rust_decimal::Decimal,
}

pub fn book(_book: &Book) -> Totals {
    Totals {
        principal: rust_decimal::Decimal::ZERO,
        yearly_interest: rust_decimal::Decimal::ZERO,
        weighted_yield: rust_decimal::Decimal::ZERO,
    }
}
