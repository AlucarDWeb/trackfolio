use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::calc;
use crate::fx::{self, FxBoard};
use crate::model::{Book, Kind, Position};
use crate::store;
use crate::ui::{
    build_position, fmt_date, fmt_fx_pct, fmt_money, fmt_pct, fx_trend, kind_label, Field,
    OverlayMode, OverlayState,
};

#[derive(Deserialize)]
struct ApplyRequest {
    op: String,
    index: Option<usize>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    amount: String,
    #[serde(default, rename = "yield")]
    yield_pct: String,
    #[serde(default)]
    date: String,
}

pub fn run(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Value, String> {
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    if args.is_empty() {
        return Err(usage());
    }
    let cmd = args[0].as_str();
    if cmd != "show" && cmd != "apply" {
        return Err(format!("unknown json command '{cmd}'"));
    }
    let mut file: Option<PathBuf> = None;
    let mut fetch_fx = false;
    let mut payload: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--fx" if cmd == "show" => fetch_fx = true,
            "--file" => {
                i += 1;
                let path = args
                    .get(i)
                    .ok_or_else(|| "--file requires a path".to_string())?;
                file = Some(PathBuf::from(path));
            }
            other if other.starts_with('-') || cmd != "apply" || payload.is_some() => {
                return Err(format!("unknown argument '{other}'"));
            }
            other => payload = Some(other.to_string()),
        }
        i += 1;
    }
    let path = match file {
        Some(path) => path,
        None => store::data_path().ok_or_else(|| "unable to determine data path".to_string())?,
    };
    match cmd {
        "show" => show(&path, fetch_fx),
        "apply" => {
            let raw = payload.ok_or_else(|| "apply requires a JSON argument".to_string())?;
            apply(&path, &raw)
        }
        _ => Err(usage()),
    }
}

pub fn show(path: &Path, fetch_fx: bool) -> Result<Value, String> {
    let book = store::load(path)?;
    let today = chrono::Local::now().date_naive();
    if !fetch_fx {
        return Ok(snapshot(&book, None, None, today));
    }
    match fx::eur_board() {
        Ok(board) => Ok(snapshot(&book, Some(Ok(&board)), None, today)),
        Err(error) => Ok(snapshot(
            &book,
            Some(Err(error.as_str())),
            Some("FX unavailable"),
            today,
        )),
    }
}

pub fn apply(path: &Path, raw: &str) -> Result<Value, String> {
    let req: ApplyRequest =
        serde_json::from_str(raw).map_err(|e| format!("invalid request: {e}"))?;
    let mut book = store::load(path)?;
    let today = chrono::Local::now().date_naive();
    let message = match req.op.as_str() {
        "add" => {
            place(&mut book, &req, None)?;
            None
        }
        "edit" => {
            let index = req.index.ok_or_else(|| "index is required".to_string())?;
            place(&mut book, &req, Some(index))?;
            None
        }
        "delete" => {
            let index = req.index.ok_or_else(|| "index is required".to_string())?;
            Some(delete_at(&mut book, index)?)
        }
        other => return Err(format!("unknown op \"{other}\"")),
    };
    store::save(path, &book)?;
    Ok(snapshot(&book, None, message.as_deref(), today))
}

fn usage() -> String {
    "usage: trackfolio json show [--fx] [--file PATH] | trackfolio json apply [--file PATH] <json>"
        .to_string()
}

fn place(book: &mut Book, req: &ApplyRequest, index: Option<usize>) -> Result<(), String> {
    if let Some(i) = index {
        if i >= book.positions.len() {
            return Err("index out of range".to_string());
        }
    }
    let state = form(req, index)?;
    let quote = if state.currency == "EUR" {
        Some(fx::eur_usd()?)
    } else {
        None
    };
    let id = match index {
        None => ulid::Ulid::new(),
        Some(i) => book.positions[i].id,
    };
    let position = build_position(&state, quote.as_ref(), id)?;
    match index {
        None => book.positions.push(position),
        Some(i) => book.positions[i] = position,
    }
    Ok(())
}

fn form(req: &ApplyRequest, index: Option<usize>) -> Result<OverlayState, String> {
    if req.currency != "USD" && req.currency != "EUR" {
        return Err("currency must be USD or EUR".to_string());
    }
    Ok(OverlayState {
        mode: match index {
            None => OverlayMode::Add,
            Some(i) => OverlayMode::Edit { index: i },
        },
        focus: Field::Name,
        name: req.name.clone(),
        kind: parse_kind(&req.kind)?,
        currency: req.currency.clone(),
        amount: req.amount.clone(),
        yield_pct: req.yield_pct.clone(),
        maturity: req.date.clone(),
        error: None,
    })
}

fn parse_kind(raw: &str) -> Result<Kind, String> {
    match raw {
        "tbill" => Ok(Kind::Tbill),
        "deposit" => Ok(Kind::Deposit),
        "other" => Ok(Kind::Other),
        _ => Err("kind must be tbill, deposit, or other".to_string()),
    }
}

fn delete_at(book: &mut Book, index: usize) -> Result<String, String> {
    if index >= book.positions.len() {
        return Err("index out of range".to_string());
    }
    let removed = book.positions.remove(index);
    Ok(format!("deleted \"{}\"", removed.name))
}

fn snapshot(
    book: &Book,
    fx: Option<Result<&FxBoard, &str>>,
    message: Option<&str>,
    today: NaiveDate,
) -> Value {
    let totals = calc::book(&book.positions, today);
    let yield_text = fmt_pct(totals.book_yield * Decimal::from(100));
    let positions: Vec<Value> = book
        .positions
        .iter()
        .enumerate()
        .map(|(index, position)| position_json(index, position, today))
        .collect();
    json!({
        "ok": true,
        "bar": format!("{}  {yield_text}", fmt_money(totals.capital)),
        "barYield": yield_text,
        "fx": match fx {
            None => Value::Null,
            Some(Ok(board)) => fx_object(Some(board), None),
            Some(Err(error)) => fx_object(None, Some(error)),
        },
        "kpis": {
            "capital": fmt_money(totals.capital),
            "yield": yield_text,
            "day": fmt_money(totals.day),
            "week": fmt_money(totals.week),
            "month": fmt_money(totals.month),
            "year": fmt_money(totals.year),
        },
        "positions": positions,
        "message": message,
    })
}

fn position_json(index: usize, position: &Position, today: NaiveDate) -> Value {
    let row = calc::row(position, today);
    let expired = position
        .maturity
        .as_deref()
        .and_then(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok())
        .is_some_and(|date| date < today);
    let amount = match (position.source_ccy.as_str(), position.source_amount) {
        ("EUR", Some(amount)) => amount.to_string(),
        _ => position.principal_usd.to_string(),
    };
    let date_raw = if position.kind == Kind::Deposit {
        position.start_date.clone().unwrap_or_default()
    } else {
        position.maturity.clone().unwrap_or_default()
    };
    let date_shown = if position.kind == Kind::Deposit {
        fmt_date(position.start_date.as_deref())
    } else {
        fmt_date(position.maturity.as_deref())
    };
    json!({
        "index": index,
        "kind": kind_label(&position.kind),
        "kindId": kind_id(&position.kind),
        "name": position.name,
        "principal": fmt_money(calc::current_value(position, today)),
        "yield": fmt_pct(position.yield_pct),
        "date": date_shown,
        "day": fmt_money(row.day),
        "week": fmt_money(row.week),
        "month": fmt_money(row.month),
        "year": fmt_money(row.year),
        "expired": expired,
        "edit": {
            "name": position.name,
            "kind": kind_id(&position.kind),
            "currency": position.source_ccy,
            "amount": amount,
            "yield": position.yield_pct.to_string(),
            "date": date_raw,
        },
    })
}

fn kind_id(kind: &Kind) -> &'static str {
    match kind {
        Kind::Tbill => "tbill",
        Kind::Deposit => "deposit",
        Kind::Other => "other",
    }
}

fn fx_object(board: Option<&FxBoard>, error: Option<&str>) -> Value {
    let Some(board) = board else {
        return json!({
            "usd": null,
            "usdPct": null,
            "usdTrend": null,
            "jpy": null,
            "jpyPct": null,
            "jpyTrend": null,
            "date": null,
            "error": error.unwrap_or("FX unavailable"),
        });
    };
    json!({
        "usd": board.usd.map(|rate| rate.to_string()),
        "usdPct": board.usd_pct.map(fmt_fx_pct),
        "usdTrend": board.usd_pct.map(fx_trend),
        "jpy": board.jpy.map(|rate| rate.to_string()),
        "jpyPct": board.jpy_pct.map(fmt_fx_pct),
        "jpyTrend": board.jpy_pct.map(fx_trend),
        "date": board.date,
        "error": null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use tempfile::tempdir;

    fn dec(raw: &str) -> Decimal {
        Decimal::from_str_exact(raw).unwrap()
    }

    fn empty() -> Book {
        Book {
            currency: "USD".to_string(),
            positions: Vec::new(),
        }
    }

    fn add_usd(path: &Path, name: &str, amount: &str, yield_pct: &str, date: &str) -> Value {
        let raw = format!(
            r#"{{"op":"add","name":"{name}","kind":"tbill","currency":"USD","amount":"{amount}","yield":"{yield_pct}","date":"{date}"}}"#
        );
        apply(path, &raw).unwrap()
    }

    #[test]
    fn show_missing_file_is_empty_book_and_does_not_create_it() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        let snap = show(&path, false).unwrap();
        assert_eq!(snap["ok"], true);
        assert_eq!(snap["bar"], "$0.00  0.00%");
        assert_eq!(snap["kpis"]["capital"], "$0.00");
        assert_eq!(snap["kpis"]["yield"], "0.00%");
        assert!(snap["fx"].is_null());
        assert!(snap["message"].is_null());
        assert!(snap["positions"].as_array().unwrap().is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn snapshot_matches_tui_formatters() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let position = Position {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap(),
            kind: Kind::Tbill,
            name: "Bill".to_string(),
            principal_usd: dec("1000.00"),
            yield_pct: dec("5.12"),
            maturity: Some("2026-09-01".to_string()),
            start_date: None,
            source_ccy: "USD".to_string(),
            source_amount: None,
            fx_rate: None,
            fx_date: None,
        };
        let book = Book {
            currency: "USD".to_string(),
            positions: vec![position.clone()],
        };
        let totals = calc::book(&book.positions, today);
        let snap = snapshot(&book, None, None, today);
        assert_eq!(snap["kpis"]["capital"], fmt_money(totals.capital));
        assert_eq!(
            snap["kpis"]["yield"],
            fmt_pct(totals.book_yield * Decimal::from(100))
        );
        assert_eq!(snap["kpis"]["day"], fmt_money(totals.day));
        assert_eq!(snap["kpis"]["year"], fmt_money(totals.year));
        assert_eq!(
            snap["bar"],
            format!(
                "{}  {}",
                fmt_money(totals.capital),
                fmt_pct(totals.book_yield * Decimal::from(100))
            )
        );
        let row = &snap["positions"][0];
        assert_eq!(row["kind"], "T-Bill");
        assert_eq!(
            row["principal"],
            fmt_money(calc::current_value(&position, today))
        );
        assert_eq!(row["yield"], fmt_pct(position.yield_pct));
        assert_eq!(row["expired"], true);
        assert_eq!(row["edit"]["amount"], "1000.00");
        assert_eq!(row["edit"]["yield"], "5.12");
        assert_eq!(row["edit"]["date"], "2026-09-01");
    }

    #[test]
    fn fx_snapshot_uses_tui_percent_and_trend() {
        let board = FxBoard {
            usd: Some(dec("1.1")),
            jpy: Some(dec("160.5")),
            usd_pct: Some(dec("0.3195")),
            jpy_pct: Some(dec("-1.931")),
            date: Some("2026-10-01".to_string()),
        };
        let snap = snapshot(
            &empty(),
            Some(Ok(&board)),
            None,
            NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
        );
        assert_eq!(snap["fx"]["usd"], "1.1");
        assert_eq!(snap["fx"]["usdPct"], "+0.3%");
        assert_eq!(snap["fx"]["usdTrend"], "up");
        assert_eq!(snap["fx"]["jpyPct"], "-1.9%");
        assert_eq!(snap["fx"]["jpyTrend"], "down");
        assert_eq!(snap["fx"]["date"], "2026-10-01");
        assert!(snap["fx"]["error"].is_null());
        assert!(snap["message"].is_null());
    }

    #[test]
    fn apply_add_edit_delete_roundtrip_preserves_id() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("portfolio.json");
        let added = add_usd(&path, "Bill", "1000.00", "5", "2026-12-31");
        assert!(added["message"].is_null());
        assert_eq!(added["positions"][0]["name"], "Bill");
        let id = store::load(&path).unwrap().positions[0].id;

        let edited = apply(
            &path,
            r#"{"op":"edit","index":0,"name":"Bill 2","kind":"other","currency":"USD","amount":"2000","yield":"4","date":"2027-01-01"}"#,
        )
        .unwrap();
        let loaded = store::load(&path).unwrap();
        assert_eq!(loaded.positions.len(), 1);
        assert_eq!(loaded.positions[0].id, id);
        assert_eq!(loaded.positions[0].name, "Bill 2");
        assert_eq!(loaded.positions[0].kind, Kind::Other);
        assert_eq!(loaded.positions[0].principal_usd, dec("2000"));
        assert!(loaded.positions[0].maturity.is_some());
        assert!(loaded.positions[0].start_date.is_none());
        assert_eq!(edited["positions"][0]["kind"], "Other");

        add_usd(&path, "Second", "10", "1", "");
        let deleted = apply(&path, r#"{"op":"delete","index":0}"#).unwrap();
        assert_eq!(deleted["message"], "deleted \"Bill 2\"");
        let loaded = store::load(&path).unwrap();
        assert_eq!(loaded.positions.len(), 1);
        assert_eq!(loaded.positions[0].name, "Second");
    }

    #[test]
    fn deposit_date_is_start_date_and_display_uses_current_value() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        apply(
            &path,
            r#"{"op":"add","name":"Sav","kind":"deposit","currency":"USD","amount":"1000","yield":"5","date":"2020-01-01"}"#,
        )
        .unwrap();
        let position = store::load(&path).unwrap().positions.remove(0);
        assert!(position.maturity.is_none());
        assert_eq!(position.start_date.as_deref(), Some("2020-01-01"));
        let today = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let book = Book {
            currency: "USD".to_string(),
            positions: vec![position.clone()],
        };
        let snap = snapshot(&book, None, None, today);
        assert_eq!(
            snap["positions"][0]["principal"],
            fmt_money(calc::current_value(&position, today))
        );
        assert_ne!(snap["positions"][0]["principal"], fmt_money(dec("1000")));
        assert_eq!(snap["positions"][0]["expired"], false);
        assert_eq!(snap["positions"][0]["edit"]["date"], "2020-01-01");
    }

    #[test]
    fn invalid_apply_does_not_create_or_change_the_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        let err = apply(
            &path,
            r#"{"op":"add","name":"","kind":"tbill","currency":"USD","amount":"1","yield":"1","date":""}"#,
        );
        assert_eq!(err.unwrap_err(), "name is required");
        assert!(!path.exists());

        add_usd(&path, "Bill", "10", "1", "");
        let before = std::fs::read(&path).unwrap();
        let err = apply(&path, r#"{"op":"delete","index":4}"#);
        assert_eq!(err.unwrap_err(), "index out of range");
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let err = apply(
            &path,
            r#"{"op":"add","name":"x","kind":"nope","currency":"USD","amount":"1","yield":"1","date":""}"#,
        );
        assert_eq!(err.unwrap_err(), "kind must be tbill, deposit, or other");
        let err = apply(
            &path,
            r#"{"op":"add","name":"x","kind":"tbill","currency":"GBP","amount":"1","yield":"1","date":""}"#,
        );
        assert_eq!(err.unwrap_err(), "currency must be USD or EUR");
        let err = apply(
            &path,
            r#"{"op":"add","name":"x","kind":"tbill","currency":"USD","amount":"0","yield":"1","date":"bad"}"#,
        );
        assert_eq!(err.unwrap_err(), "amount must be greater than zero");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn run_reads_file_flag_and_rejects_unknown_commands() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("portfolio.json");
        let snap = run(["show", "--file", path.to_str().unwrap()]).unwrap();
        assert_eq!(snap["ok"], true);
        assert!(snap["fx"].is_null());
        assert!(!path.exists());
        assert!(run(["nope"]).is_err());
        assert!(run(std::iter::empty::<&str>()).is_err());
        assert!(run(["apply", "--file", path.to_str().unwrap()]).is_err());
    }
}
