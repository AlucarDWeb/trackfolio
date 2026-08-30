# Trackfolio

A terminal UI for tracking a personal book of US T-Bills and USD/EUR deposits: capital, weighted yield, and interest projections (day / week / month / year), with full CRUD over positions.

Built with Rust and [ratatui](https://github.com/ratatui/ratatui).

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable)

## Install

```bash
cargo install --path .
```

## Usage

Run `trackfolio` in a terminal (minimum 80×24). Keys:

| Key | Action |
|---|---|
| `j` / `k` / arrows | move row selection |
| `a` | add a position |
| `e` / Enter | edit selected position |
| `d` | confirm, then delete |
| `q` / Esc | quit |

Positions are saved immediately on every add/edit/delete — there is no save command.

## Data

The portfolio is stored as a single JSON file at `~/.local/share/trackfolio/portfolio.json` (override with the `TRACKFOLIO_FILE` environment variable).

Money and yields are stored as decimal strings and computed with exact decimal arithmetic. EUR positions are converted to USD once via the [Frankfurter](https://frankfurter.dev) API at entry time; the FX rate and date are persisted with the position.

## Interest compounding

**Deposits** grow with daily compounding: `value = principal × (1 + yield_pct/100 / 365)^n`, where `n` is the number of whole days from the position's start date to today. The start date is optional; if it is missing, the value stays at the entered nominal, so 0.1.0 files open unchanged.

**T-Bills and other** positions stay at face value (zero-coupon): the kind selects the formula, the yield alone is not enough.

The grown value is computed at read time and never written back to the JSON file — `principal_usd` remains the original nominal. Day count is 365 (not 365.25, not 30/360).

In the UI overlay, the date field is labeled **start date** for deposits and **maturity** for T-Bills/other. The PRINCIPAL column and the CAPITAL KPI show the current (grown) value for deposits.

## License

MIT — see [LICENSE](LICENSE).
