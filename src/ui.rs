use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState};
use rust_decimal::{Decimal, RoundingStrategy};

use crate::fx::{FxQuote, eur_to_usd};
use crate::model::{Book, Kind, Position};
use crate::{calc, fx};

pub fn fmt_money(d: Decimal) -> String {
    let s = fmt_decimal_2dp(d);
    if let Some(rest) = s.strip_prefix('-') {
        format!("-${rest}")
    } else {
        format!("${s}")
    }
}

pub fn fmt_pct(d: Decimal) -> String {
    format!("{}%", fmt_decimal_2dp(d))
}

pub fn fmt_date(d: Option<&str>) -> String {
    d.unwrap_or("—").to_string()
}

fn fmt_decimal_2dp(d: Decimal) -> String {
    let rounded = d.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    let neg = rounded < Decimal::ZERO;
    let abs = if neg { -rounded } else { rounded };
    let text = abs.to_string();
    let (int_part, cents) = match text.split_once('.') {
        Some((i, c)) => (i.to_string(), format!("{c:0<2}")),
        None => (text, "00".to_string()),
    };
    let sign = if neg { "-" } else { "" };
    format!("{sign}{}.{cents}", group_thousands(&int_part))
}

fn group_thousands(digits: &str) -> String {
    let len = digits.len();
    digits
        .char_indices()
        .map(|(i, c)| {
            if i > 0 && (len - i).is_multiple_of(3) {
                format!(",{c}")
            } else {
                c.to_string()
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayMode {
    Add,
    Edit { index: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Kind,
    Currency,
    Amount,
    Yield,
    Maturity,
}

#[derive(Clone, Debug)]
pub struct OverlayState {
    pub mode: OverlayMode,
    pub focus: Field,
    pub name: String,
    pub kind: Kind,
    pub currency: String,
    pub amount: String,
    pub yield_pct: String,
    pub maturity: String,
    pub error: Option<String>,
}

impl OverlayState {
    pub fn new_add() -> Self {
        Self {
            mode: OverlayMode::Add,
            focus: Field::Name,
            name: String::new(),
            kind: Kind::Tbill,
            currency: "USD".to_string(),
            amount: String::new(),
            yield_pct: String::new(),
            maturity: String::new(),
            error: None,
        }
    }

    pub fn from_position(index: usize, p: &Position) -> Self {
        let amount = match (p.source_ccy.as_str(), p.source_amount) {
            ("EUR", Some(a)) => a.to_string(),
            _ => p.principal_usd.to_string(),
        };
        Self {
            mode: OverlayMode::Edit { index },
            focus: Field::Name,
            name: p.name.clone(),
            kind: p.kind.clone(),
            currency: p.source_ccy.clone(),
            amount,
            yield_pct: p.yield_pct.to_string(),
            maturity: if p.kind == Kind::Deposit {
                p.start_date.clone().unwrap_or_default()
            } else {
                p.maturity.clone().unwrap_or_default()
            },
            error: None,
        }
    }

    fn focus_next(&mut self) {
        self.focus = match self.focus {
            Field::Name => Field::Kind,
            Field::Kind => Field::Currency,
            Field::Currency => Field::Amount,
            Field::Amount => Field::Yield,
            Field::Yield => Field::Maturity,
            Field::Maturity => Field::Name,
        };
    }

    fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Field::Name => Field::Maturity,
            Field::Kind => Field::Name,
            Field::Currency => Field::Kind,
            Field::Amount => Field::Currency,
            Field::Yield => Field::Amount,
            Field::Maturity => Field::Yield,
        };
    }

    fn text_buffer_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            Field::Name => Some(&mut self.name),
            Field::Amount => Some(&mut self.amount),
            Field::Yield => Some(&mut self.yield_pct),
            Field::Maturity => Some(&mut self.maturity),
            Field::Kind | Field::Currency => None,
        }
    }

    fn cycle_kind(&mut self, forward: bool) {
        self.kind = match (&self.kind, forward) {
            (Kind::Tbill, true) | (Kind::Other, false) => Kind::Deposit,
            (Kind::Deposit, true) | (Kind::Tbill, false) => Kind::Other,
            (Kind::Other, true) | (Kind::Deposit, false) => Kind::Tbill,
        };
    }

    fn toggle_currency(&mut self) {
        self.currency = if self.currency == "EUR" {
            "USD"
        } else {
            "EUR"
        }
        .to_string();
    }
}

#[derive(Debug)]
pub struct ParsedInput {
    pub amount: Decimal,
    pub yield_pct: Decimal,
}

pub fn validate_overlay(state: &OverlayState) -> Result<ParsedInput, String> {
    if state.name.trim().is_empty() {
        return Err("name is required".to_string());
    }
    let amount = parse_buffer(&state.amount, "amount")?;
    if amount <= Decimal::ZERO {
        return Err("amount must be greater than zero".to_string());
    }
    let yield_pct = parse_buffer(&state.yield_pct, "yield")?;
    if yield_pct < Decimal::ZERO {
        return Err("yield must be zero or greater".to_string());
    }
    if !state.maturity.is_empty() && !is_iso_date(&state.maturity) {
        return Err(format!(
            "{} must be YYYY-MM-DD",
            date_field_label(&state.kind)
        ));
    }
    Ok(ParsedInput { amount, yield_pct })
}

fn parse_buffer(buffer: &str, field: &str) -> Result<Decimal, String> {
    Decimal::from_str_exact(buffer.trim()).map_err(|_| format!("{field} must be a number"))
}

fn is_iso_date(s: &str) -> bool {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|d| d.format("%Y-%m-%d").to_string() == s)
        .unwrap_or(false)
}

pub fn build_position(
    state: &OverlayState,
    quote: Option<&FxQuote>,
    id: ulid::Ulid,
) -> Result<Position, String> {
    let input = validate_overlay(state)?;
    let (principal_usd, source_ccy, source_amount, fx_rate, fx_date) = match state.currency.as_str()
    {
        "EUR" => {
            let q = quote.ok_or_else(|| "FX rate unavailable".to_string())?;
            (
                eur_to_usd(input.amount, q),
                "EUR".to_string(),
                Some(input.amount),
                Some(q.rate),
                Some(q.date.clone()),
            )
        }
        _ => (
            input.amount,
            "USD".to_string(),
            None,
            None,
            None,
        ),
    };
    let date = if state.maturity.is_empty() {
        None
    } else {
        Some(state.maturity.clone())
    };
    let (maturity, start_date) = if state.kind == Kind::Deposit {
        (None, date)
    } else {
        (date, None)
    };
    Ok(Position {
        id,
        kind: state.kind.clone(),
        name: state.name.trim().to_string(),
        principal_usd,
        yield_pct: input.yield_pct,
        maturity,
        start_date,
        source_ccy,
        source_amount,
        fx_rate,
        fx_date,
    })
}

pub fn display_principal(p: &Position, today: NaiveDate) -> Decimal {
    calc::current_value(p, today)
}

fn row_date(p: &Position) -> Option<&str> {
    if p.kind == Kind::Deposit {
        p.start_date.as_deref()
    } else {
        p.maturity.as_deref()
    }
}

pub struct App {
    pub book: Book,
    pub selected: Option<usize>,
    pub overlay: Option<OverlayState>,
    pub message: Option<String>,
    pub dirty: bool,
    delete_armed: bool,
}

impl App {
    pub fn new(book: Book) -> Self {
        let selected = if book.positions.is_empty() {
            None
        } else {
            Some(0)
        };
        Self {
            book,
            selected,
            overlay: None,
            message: None,
            dirty: false,
            delete_armed: false,
        }
    }

    pub fn next_row(&mut self) {
        self.move_selection(1);
    }

    pub fn prev_row(&mut self) {
        self.move_selection(-1);
    }

    fn move_selection(&mut self, delta: i64) {
        let len = self.book.positions.len();
        if len == 0 {
            self.selected = None;
            self.delete_armed = false;
            return;
        }
        self.selected = Some(match self.selected {
            Some(i) => (i as i64 + delta).clamp(0, len as i64 - 1) as usize,
            None => 0,
        });
        self.delete_armed = false;
        self.message = None;
    }

    pub fn open_add(&mut self) {
        self.overlay = Some(OverlayState::new_add());
        self.delete_armed = false;
        self.message = None;
    }

    pub fn open_edit(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        if i >= self.book.positions.len() {
            return;
        }
        self.overlay = Some(OverlayState::from_position(i, &self.book.positions[i]));
        self.delete_armed = false;
        self.message = None;
    }

    pub fn confirm_delete(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        if i >= self.book.positions.len() {
            return;
        }
        if self.delete_armed {
            let removed = self.book.positions.remove(i);
            self.delete_armed = false;
            self.dirty = true;
            let len = self.book.positions.len();
            self.selected = if len == 0 {
                None
            } else {
                Some(i.min(len - 1))
            };
            self.message = Some(format!("deleted \"{}\"", removed.name));
        } else {
            self.delete_armed = true;
            self.message = Some(format!(
                "press d again to delete \"{}\"",
                self.book.positions[i].name
            ));
        }
    }

    pub fn submit(&mut self) {
        let Some(mut overlay) = self.overlay.take() else {
            return;
        };
        let quote = if overlay.currency == "EUR" {
            match fx::eur_usd() {
                Ok(q) => Some(q),
                Err(e) => {
                    overlay.error = Some(e);
                    self.overlay = Some(overlay);
                    return;
                }
            }
        } else {
            None
        };
        let id = match overlay.mode {
            OverlayMode::Add => ulid::Ulid::new(),
            OverlayMode::Edit { index } => self
                .book
                .positions
                .get(index)
                .map(|p| p.id)
                .unwrap_or_else(ulid::Ulid::new),
        };
        match build_position(&overlay, quote.as_ref(), id) {
            Ok(p) => {
                match overlay.mode {
                    OverlayMode::Add => self.book.positions.push(p),
                    OverlayMode::Edit { index } => {
                        if index < self.book.positions.len() {
                            self.book.positions[index] = p;
                        } else {
                            self.book.positions.push(p);
                        }
                    }
                }
                self.overlay = None;
                self.message = None;
                self.dirty = true;
            }
            Err(e) => {
                overlay.error = Some(e);
                self.overlay = Some(overlay);
            }
        }
    }
}

pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if app.overlay.is_some() {
        overlay_key(app, key);
        return false;
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.next_row();
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.prev_row();
            false
        }
        KeyCode::Char('a') => {
            app.open_add();
            false
        }
        KeyCode::Char('e') | KeyCode::Enter => {
            app.open_edit();
            false
        }
        KeyCode::Char('d') => {
            app.confirm_delete();
            false
        }
        KeyCode::Char('q') | KeyCode::Esc => true,
        _ => false,
    }
}

fn overlay_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.overlay = None;
            return;
        }
        KeyCode::Enter => {
            app.submit();
            return;
        }
        _ => {}
    }
    let Some(overlay) = app.overlay.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Tab | KeyCode::Down => overlay.focus_next(),
        KeyCode::BackTab | KeyCode::Up => overlay.focus_prev(),
        KeyCode::Backspace => {
            if let Some(buffer) = overlay.text_buffer_mut() {
                buffer.pop();
            }
        }
        KeyCode::Left => match overlay.focus {
            Field::Kind => overlay.cycle_kind(false),
            Field::Currency => overlay.toggle_currency(),
            _ => {}
        },
        KeyCode::Right => match overlay.focus {
            Field::Kind => overlay.cycle_kind(true),
            Field::Currency => overlay.toggle_currency(),
            _ => {}
        },
        KeyCode::Char(' ')
            if matches!(overlay.focus, Field::Kind | Field::Currency) =>
        {
            match overlay.focus {
                Field::Kind => overlay.cycle_kind(true),
                Field::Currency => overlay.toggle_currency(),
                _ => {}
            }
        }
        KeyCode::Char(c)
            if !key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
        {
            if let Some(buffer) = overlay.text_buffer_mut() {
                buffer.push(c);
            }
        }
        _ => {}
    }
}

fn kind_label(kind: &Kind) -> &'static str {
    match kind {
        Kind::Tbill => "T-Bill",
        Kind::Deposit => "Deposit",
        Kind::Other => "Other",
    }
}

fn date_field_label(kind: &Kind) -> &'static str {
    match kind {
        Kind::Deposit => "start date",
        Kind::Tbill | Kind::Other => "maturity",
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = ratatui::layout::Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(area);

    f.render_widget(kpi_bar(app), chunks[0]);
    f.render_widget(separator(chunks[1].width), chunks[1]);
    render_book(f, app, chunks[2]);
    f.render_widget(footer(app), chunks[3]);

    if app.overlay.is_some() {
        render_overlay(f, app, area);
    }
}

fn kpi_bar(app: &App) -> Paragraph<'static> {
    let totals = calc::book(&app.book.positions, chrono::Local::now().date_naive());
    let label = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let value = Style::default().add_modifier(Modifier::BOLD);
    let mut spans = Vec::new();
    let mut push = |name: &str, text: String| {
        spans.push(Span::styled(format!(" {name} "), label));
        spans.push(Span::styled(text, value));
    };
    push("CAPITAL", fmt_money(totals.capital));
    push("YIELD", fmt_pct(totals.book_yield * Decimal::from(100)));
    push("DAY", fmt_money(totals.day));
    push("WEEK", fmt_money(totals.week));
    push("MONTH", fmt_money(totals.month));
    push("YEAR", fmt_money(totals.year));
    Paragraph::new(Line::from(spans))
}

fn separator(width: u16) -> Paragraph<'static> {
    Paragraph::new("─".repeat(width.max(1) as usize))
        .style(Style::default().fg(Color::DarkGray))
}

fn book_headers() -> [&'static str; 10] {
    [
        "#", "TYPE", "NAME", "PRINCIPAL", "YIELD", "MATURITY", "DAY", "WEEK", "MONTH", "YEAR",
    ]
}

fn book_column_widths() -> [Constraint; 10] {
    [
        Constraint::Length(4),
        Constraint::Length(9),
        Constraint::Min(16),
        Constraint::Length(14),
        Constraint::Length(8),
        Constraint::Length(12),
        Constraint::Length(11),
        Constraint::Length(11),
        Constraint::Length(13),
        Constraint::Length(13),
    ]
}

fn render_book(f: &mut Frame, app: &App, area: Rect) {
    if app.book.positions.is_empty() {
        f.render_widget(
            Paragraph::new("press a to add")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }
    let today = chrono::Local::now().date_naive();
    let rows = app.book.positions.iter().enumerate().map(|(i, p)| {
        let r = calc::row(p, chrono::Local::now().date_naive());
        let expired = p
            .maturity
            .as_deref()
            .and_then(|m| NaiveDate::parse_from_str(m, "%Y-%m-%d").ok())
            .map(|d| d < today)
            .unwrap_or(false);
        let style = if expired {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new(vec![
            (i + 1).to_string(),
            kind_label(&p.kind).to_string(),
            p.name.clone(),
            fmt_money(display_principal(p, today)),
            fmt_pct(p.yield_pct),
            fmt_date(row_date(p)),
            fmt_money(r.day),
            fmt_money(r.week),
            fmt_money(r.month),
            fmt_money(r.year),
        ])
        .style(style)
    });
    let header = Row::new(book_headers().map(str::to_string))
        .style(Style::default().add_modifier(Modifier::BOLD));
    let table = Table::new(rows, book_column_widths())
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = TableState::default().with_selected(app.selected);
    f.render_stateful_widget(table, area, &mut state);
}

fn footer(app: &App) -> Paragraph<'static> {
    let mut spans = vec![Span::styled(
        " a add  e edit  d delete  j/k move  q quit ",
        Style::default().fg(Color::DarkGray),
    )];
    if let Some(message) = &app.message {
        spans.push(Span::styled(
            format!(" {message}"),
            Style::default().fg(Color::Yellow),
        ));
    }
    Paragraph::new(Line::from(spans))
}

fn render_overlay(f: &mut Frame, app: &App, area: Rect) {
    let Some(overlay) = app.overlay.as_ref() else {
        return;
    };
    let rect = popup_rect(58, 11, area);
    f.render_widget(Clear, rect);
    let title = match overlay.mode {
        OverlayMode::Add => " add position ",
        OverlayMode::Edit { .. } => " edit position ",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let focused = Style::default().add_modifier(Modifier::REVERSED);
    let dim = Style::default().fg(Color::DarkGray);
    let fields = [
        ("name", overlay.name.clone(), Field::Name),
        ("kind", kind_label(&overlay.kind).to_string(), Field::Kind),
        ("currency", overlay.currency.clone(), Field::Currency),
        ("amount", overlay.amount.clone(), Field::Amount),
        ("yield %", overlay.yield_pct.clone(), Field::Yield),
        (
            date_field_label(&overlay.kind),
            if overlay.maturity.is_empty() {
                "—".to_string()
            } else {
                overlay.maturity.clone()
            },
            Field::Maturity,
        ),
    ];
    let mut lines: Vec<Line> = fields
        .into_iter()
        .map(|(label, value, field)| {
            let text = format!(" {label:<9} {value} ");
            if overlay.focus == field {
                Line::from(Span::styled(text, focused))
            } else {
                Line::from(Span::raw(text))
            }
        })
        .collect();
    lines.push(Line::from(""));
    match &overlay.error {
        Some(error) => lines.push(Line::from(Span::styled(
            format!(" {error}"),
            Style::default().fg(Color::Red),
        ))),
        None => lines.push(Line::from(Span::styled(
            " enter: save  esc: cancel  tab: next",
            dim,
        ))),
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn popup_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str_exact(s).unwrap()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            assert!(!handle_key(app, key(KeyCode::Char(c))));
        }
    }

    fn usd_position(name: &str, principal: &str, yield_pct: &str) -> Position {
        Position {
            id: ulid::Ulid::new(),
            kind: Kind::Tbill,
            name: name.to_string(),
            principal_usd: dec(principal),
            yield_pct: dec(yield_pct),
            maturity: None,
            start_date: None,
            source_ccy: "USD".to_string(),
            source_amount: None,
            fx_rate: None,
            fx_date: None,
        }
    }

    #[test]
    fn fmt_money_zero_and_half_up_edges() {
        assert_eq!(fmt_money(dec("0")), "$0.00");
        assert_eq!(fmt_money(dec("0.004")), "$0.00");
        assert_eq!(fmt_money(dec("0.005")), "$0.01");
        assert_eq!(fmt_money(dec("-0.005")), "-$0.01");
        assert_eq!(fmt_money(dec("1.5")), "$1.50");
    }

    #[test]
    fn fmt_money_thousands_separator() {
        assert_eq!(fmt_money(dec("1234567.891")), "$1,234,567.89");
        assert_eq!(fmt_money(dec("120000")), "$120,000.00");
        assert_eq!(fmt_money(dec("999.995")), "$1,000.00");
        assert_eq!(fmt_money(dec("999")), "$999.00");
        assert_eq!(fmt_money(dec("1000")), "$1,000.00");
    }

    #[test]
    fn fmt_pct_two_decimals_half_up() {
        assert_eq!(fmt_pct(dec("4.31")), "4.31%");
        assert_eq!(fmt_pct(dec("0")), "0.00%");
        assert_eq!(fmt_pct(dec("4.305")), "4.31%");
        assert_eq!(fmt_pct(dec("3.80")), "3.80%");
        assert_eq!(fmt_pct(dec("5.125")), "5.13%");
    }

    #[test]
    fn fmt_date_dash_for_none() {
        assert_eq!(fmt_date(None), "—");
        assert_eq!(fmt_date(Some("2026-09-26")), "2026-09-26");
    }

    #[test]
    fn book_table_assigns_a_width_to_the_year_column() {
        let headers = super::book_headers();
        let widths = super::book_column_widths();
        assert_eq!(headers.len(), widths.len());
        assert_eq!(headers.last().copied(), Some("YEAR"));
    }

    #[test]
    fn new_selects_first_row_and_moves_clamped() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![usd_position("a", "1000", "5"), usd_position("b", "2000", "4")],
        });
        assert_eq!(app.selected, Some(0));
        app.next_row();
        assert_eq!(app.selected, Some(1));
        app.next_row();
        assert_eq!(app.selected, Some(1));
        app.prev_row();
        assert_eq!(app.selected, Some(0));
        app.prev_row();
        assert_eq!(app.selected, Some(0));
    }

    #[test]
    fn empty_book_selection_stays_none() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert_eq!(app.selected, None);
        app.next_row();
        app.prev_row();
        assert_eq!(app.selected, None);
    }

    #[test]
    fn delete_is_two_stage() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![usd_position("a", "1000", "5"), usd_position("b", "2000", "4")],
        });
        app.confirm_delete();
        assert_eq!(app.book.positions.len(), 2);
        assert!(app.message.as_deref().unwrap().contains("press d again"));
        app.confirm_delete();
        assert_eq!(app.book.positions.len(), 1);
        assert_eq!(app.book.positions[0].name, "b");
        assert!(app.dirty);
    }

    #[test]
    fn delete_clamps_selection() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![usd_position("a", "1000", "5"), usd_position("b", "2000", "4")],
        });
        app.next_row();
        app.confirm_delete();
        app.confirm_delete();
        assert_eq!(app.selected, Some(0));
        app.confirm_delete();
        app.confirm_delete();
        assert_eq!(app.book.positions.len(), 0);
        assert_eq!(app.selected, None);
    }

    #[test]
    fn movement_disarms_pending_delete() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![usd_position("a", "1000", "5"), usd_position("b", "2000", "4")],
        });
        app.confirm_delete();
        app.next_row();
        app.confirm_delete();
        assert_eq!(app.book.positions.len(), 2);
    }

    #[test]
    fn delete_without_selection_is_noop() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        app.confirm_delete();
        app.confirm_delete();
        assert!(app.book.positions.is_empty());
        assert!(!app.dirty);
    }

    fn overlay(name: &str, amount: &str, yield_pct: &str, maturity: &str) -> OverlayState {
        OverlayState {
            mode: OverlayMode::Add,
            focus: Field::Name,
            name: name.to_string(),
            kind: Kind::Tbill,
            currency: "USD".to_string(),
            amount: amount.to_string(),
            yield_pct: yield_pct.to_string(),
            maturity: maturity.to_string(),
            error: None,
        }
    }

    fn eur_overlay(name: &str, amount: &str, yield_pct: &str) -> OverlayState {
        let mut state = overlay(name, amount, yield_pct, "");
        state.currency = "EUR".to_string();
        state
    }

    #[test]
    fn validation_rejects_missing_fields() {
        assert_eq!(
            validate_overlay(&overlay("", "100", "5", "")).unwrap_err(),
            "name is required"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "abc", "5", "")).unwrap_err(),
            "amount must be a number"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "0", "5", "")).unwrap_err(),
            "amount must be greater than zero"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "-1", "5", "")).unwrap_err(),
            "amount must be greater than zero"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "100", "xyz", "")).unwrap_err(),
            "yield must be a number"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "100", "-0.1", "")).unwrap_err(),
            "yield must be zero or greater"
        );
    }

    #[test]
    fn validation_maturity_is_strict_iso_date() {
        assert!(validate_overlay(&overlay("a", "100", "5", "")).is_ok());
        assert!(validate_overlay(&overlay("a", "100", "5", "2026-09-26")).is_ok());
        assert_eq!(
            validate_overlay(&overlay("a", "100", "5", "garbage")).unwrap_err(),
            "maturity must be YYYY-MM-DD"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "100", "5", "2026-13-01")).unwrap_err(),
            "maturity must be YYYY-MM-DD"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "100", "5", "2026-02-30")).unwrap_err(),
            "maturity must be YYYY-MM-DD"
        );
        assert_eq!(
            validate_overlay(&overlay("a", "100", "5", "2026-9-6")).unwrap_err(),
            "maturity must be YYYY-MM-DD"
        );
    }

    #[test]
    fn build_position_usd_has_null_fx_metadata() {
        let p = build_position(&overlay("  T-Bill 4w  ", "50000", "5.12", "2026-09-26"), None, ulid::Ulid::new())
            .unwrap();
        assert_eq!(p.name, "T-Bill 4w");
        assert_eq!(p.principal_usd, dec("50000"));
        assert_eq!(p.yield_pct, dec("5.12"));
        assert_eq!(p.maturity.as_deref(), Some("2026-09-26"));
        assert_eq!(p.source_ccy, "USD");
        assert_eq!(p.source_amount, None);
        assert_eq!(p.fx_rate, None);
        assert_eq!(p.fx_date, None);
    }

    #[test]
    fn build_position_deposit_writes_start_date_and_clears_maturity() {
        let mut state = overlay("Deposit", "30000", "3.8", "2025-08-30");
        state.kind = Kind::Deposit;
        let p = build_position(&state, None, ulid::Ulid::new()).unwrap();
        assert_eq!(p.start_date.as_deref(), Some("2025-08-30"));
        assert_eq!(p.maturity, None);
    }

    #[test]
    fn build_position_tbill_sets_maturity_and_start_date_is_none() {
        let p = build_position(
            &overlay("T-Bill", "50000", "5.12", "2026-09-26"),
            None,
            ulid::Ulid::new(),
        )
        .unwrap();
        assert_eq!(p.maturity.as_deref(), Some("2026-09-26"));
        assert_eq!(p.start_date, None);
    }

    #[test]
    fn validation_deposit_date_error_names_start_date() {
        let mut state = overlay("a", "100", "5", "garbage");
        state.kind = Kind::Deposit;
        assert_eq!(
            validate_overlay(&state).unwrap_err(),
            "start date must be YYYY-MM-DD"
        );
    }

    #[test]
    fn edit_overlay_on_deposit_restores_start_date() {
        let p = Position {
            id: ulid::Ulid::new(),
            kind: Kind::Deposit,
            name: "Deposit USD".to_string(),
            principal_usd: dec("30000"),
            yield_pct: dec("3.80"),
            maturity: None,
            start_date: Some("2025-08-30".to_string()),
            source_ccy: "USD".to_string(),
            source_amount: None,
            fx_rate: None,
            fx_date: None,
        };
        let state = OverlayState::from_position(0, &p);
        assert_eq!(state.kind, Kind::Deposit);
        assert_eq!(state.maturity, "2025-08-30");
    }

    #[test]
    fn display_principal_grows_deposits_not_tbills() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 30).unwrap();
        let start = (today - chrono::Duration::days(365)).to_string();
        let mut dep = usd_position("dep", "30000", "3.8");
        dep.kind = Kind::Deposit;
        dep.start_date = Some(start.clone());
        assert!(display_principal(&dep, today) > dec("30000"));
        let mut tb = usd_position("tb", "30000", "3.8");
        tb.start_date = Some(start);
        assert_eq!(display_principal(&tb, today), dec("30000"));
    }

    #[test]
    fn row_date_shows_start_date_for_deposits_and_maturity_otherwise() {
        let mut dep = usd_position("dep", "30000", "3.8");
        dep.kind = Kind::Deposit;
        dep.start_date = Some("2025-08-30".to_string());
        dep.maturity = Some("2026-12-31".to_string());
        assert_eq!(row_date(&dep), Some("2025-08-30"));
        let mut tb = usd_position("tb", "30000", "3.8");
        tb.start_date = Some("2025-08-30".to_string());
        tb.maturity = Some("2026-12-31".to_string());
        assert_eq!(row_date(&tb), Some("2026-12-31"));
    }

    #[test]
    fn build_position_eur_uses_quote_and_keeps_source_amount() {
        let quote = FxQuote {
            rate: dec("1.085"),
            date: "2026-08-28".to_string(),
        };
        let p = build_position(&eur_overlay("Deposit", "45000", "3.10"), Some(&quote), ulid::Ulid::new())
            .unwrap();
        assert_eq!(p.principal_usd, dec("48825"));
        assert_eq!(p.source_ccy, "EUR");
        assert_eq!(p.source_amount, Some(dec("45000")));
        assert_eq!(p.fx_rate, Some(dec("1.085")));
        assert_eq!(p.fx_date.as_deref(), Some("2026-08-28"));
    }

    #[test]
    fn build_position_eur_without_quote_is_err() {
        assert_eq!(
            build_position(&eur_overlay("Deposit", "45000", "3.10"), None, ulid::Ulid::new())
                .unwrap_err(),
            "FX rate unavailable"
        );
    }

    #[test]
    fn edit_overlay_on_eur_row_restores_source_amount() {
        let p = Position {
            id: ulid::Ulid::new(),
            kind: Kind::Deposit,
            name: "Deposit EUR".to_string(),
            principal_usd: dec("48825"),
            yield_pct: dec("3.10"),
            maturity: None,
            start_date: None,
            source_ccy: "EUR".to_string(),
            source_amount: Some(dec("45000")),
            fx_rate: Some(dec("1.085")),
            fx_date: Some("2026-08-28".to_string()),
        };
        let state = OverlayState::from_position(2, &p);
        assert_eq!(state.mode, OverlayMode::Edit { index: 2 });
        assert_eq!(state.currency, "EUR");
        assert_eq!(state.amount, "45000");
        assert_eq!(state.yield_pct, "3.10");
    }

    #[test]
    fn keys_navigate_and_open_overlays() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![usd_position("a", "1000", "5"), usd_position("b", "2000", "4")],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('j'))));
        assert_eq!(app.selected, Some(1));
        assert!(!handle_key(&mut app, key(KeyCode::Down)));
        assert_eq!(app.selected, Some(1));
        assert!(!handle_key(&mut app, key(KeyCode::Char('k'))));
        assert_eq!(app.selected, Some(0));

        assert!(!handle_key(&mut app, key(KeyCode::Char('a'))));
        assert_eq!(
            app.overlay.as_ref().unwrap().mode,
            OverlayMode::Add
        );
        assert!(!handle_key(&mut app, key(KeyCode::Esc)));
        assert!(app.overlay.is_none());

        assert!(!handle_key(&mut app, key(KeyCode::Char('e'))));
        assert_eq!(
            app.overlay.as_ref().unwrap().mode,
            OverlayMode::Edit { index: 0 }
        );
        assert!(!handle_key(&mut app, key(KeyCode::Esc)));

        assert!(!handle_key(&mut app, key(KeyCode::Enter)));
        assert_eq!(
            app.overlay.as_ref().unwrap().mode,
            OverlayMode::Edit { index: 0 }
        );
        assert!(!handle_key(&mut app, key(KeyCode::Esc)));
    }

    #[test]
    fn edit_key_without_selection_is_ignored() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('e'))));
        assert!(app.overlay.is_none());
    }

    #[test]
    fn q_quits() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(handle_key(&mut app, key(KeyCode::Char('q'))));
    }

    #[test]
    fn overlay_add_usd_via_keys_submits() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('a'))));
        type_text(&mut app, "Cash");
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        assert!(!handle_key(&mut app, key(KeyCode::Right)));
        assert_eq!(app.overlay.as_ref().unwrap().kind, Kind::Deposit);
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        type_text(&mut app, "50000");
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        type_text(&mut app, "5.12");
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        assert_eq!(app.overlay.as_ref().unwrap().focus, Field::Maturity);
        assert!(!handle_key(&mut app, key(KeyCode::Enter)));

        assert!(app.overlay.is_none());
        assert_eq!(app.book.positions.len(), 1);
        let p = &app.book.positions[0];
        assert_eq!(p.name, "Cash");
        assert_eq!(p.kind, Kind::Deposit);
        assert_eq!(p.principal_usd, dec("50000"));
        assert_eq!(p.yield_pct, dec("5.12"));
        assert_eq!(p.source_ccy, "USD");
        assert_eq!(p.start_date, None);
        assert_eq!(p.maturity, None);
        assert!(app.dirty);
    }

    #[test]
    fn overlay_esc_closes_without_saving() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('a'))));
        type_text(&mut app, "Cash");
        assert!(!handle_key(&mut app, key(KeyCode::Esc)));
        assert!(app.overlay.is_none());
        assert!(app.book.positions.is_empty());
        assert!(!app.dirty);
    }

    #[test]
    fn overlay_invalid_submit_stays_open_with_error() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('a'))));
        assert!(!handle_key(&mut app, key(KeyCode::Enter)));
        let ov = app.overlay.as_ref().unwrap();
        assert_eq!(ov.error.as_deref(), Some("name is required"));
        assert!(app.book.positions.is_empty());
    }

    #[test]
    fn overlay_typing_backspace_and_backtab() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('a'))));
        type_text(&mut app, "1234");
        assert!(!handle_key(&mut app, key(KeyCode::Backspace)));
        assert_eq!(app.overlay.as_ref().unwrap().name, "123");
        assert!(!handle_key(&mut app, key(KeyCode::BackTab)));
        assert_eq!(app.overlay.as_ref().unwrap().focus, Field::Maturity);
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        assert_eq!(app.overlay.as_ref().unwrap().focus, Field::Name);
    }

    #[test]
    fn overlay_kind_cycles_with_left_right_and_space() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('a'))));
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        let ov = app.overlay.as_mut().unwrap();
        assert_eq!(ov.focus, Field::Kind);
        ov.cycle_kind(true);
        assert_eq!(ov.kind, Kind::Deposit);
        ov.cycle_kind(true);
        assert_eq!(ov.kind, Kind::Other);
        ov.cycle_kind(true);
        assert_eq!(ov.kind, Kind::Tbill);
        ov.cycle_kind(false);
        assert_eq!(ov.kind, Kind::Other);
    }

    #[test]
    fn overlay_currency_cycles_with_left_right() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![],
        });
        assert!(!handle_key(&mut app, key(KeyCode::Char('a'))));
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        assert!(!handle_key(&mut app, key(KeyCode::Tab)));
        assert_eq!(app.overlay.as_ref().unwrap().focus, Field::Currency);
        assert!(!handle_key(&mut app, key(KeyCode::Left)));
        assert_eq!(app.overlay.as_ref().unwrap().currency, "EUR");
        assert!(!handle_key(&mut app, key(KeyCode::Right)));
        assert_eq!(app.overlay.as_ref().unwrap().currency, "USD");
        assert!(!handle_key(&mut app, key(KeyCode::Char(' '))));
        assert_eq!(app.overlay.as_ref().unwrap().currency, "EUR");
    }

    #[test]
    fn usd_edit_preserves_id_and_replaces_in_place() {
        let mut app = App::new(Book {
            currency: "USD".to_string(),
            positions: vec![usd_position("a", "1000", "5")],
        });
        let id = app.book.positions[0].id;
        assert!(!handle_key(&mut app, key(KeyCode::Char('e'))));
        app.submit();
        assert!(app.overlay.is_none());
        assert_eq!(app.book.positions.len(), 1);
        assert_eq!(app.book.positions[0].id, id);
        assert_eq!(app.book.positions[0].name, "a");
        assert_eq!(app.book.positions[0].principal_usd, dec("1000"));
        assert!(app.dirty);
    }
}
