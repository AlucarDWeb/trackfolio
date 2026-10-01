# Trackfolio

A terminal UI, and an Omarchy status-bar plugin, for tracking a personal book of US T-Bills and USD/EUR deposits: capital, weighted yield, and interest projections (day / week / month / year), with full CRUD over positions.

Built with Rust and [ratatui](https://github.com/ratatui/ratatui). The bar plugin shells out to this binary so the decimal math stays exact.

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

On start the TUI fetches EUR/USD and EUR/JPY once from Frankfurter and shows them above the KPIs as EURUSD and EURYEN, each with its daily percentage change versus the previous BCE close (e.g. `+0.3%`, green up, red down). A failed fetch still opens the book.

## Interest compounding

**Deposits** grow with daily compounding: `value = principal × (1 + yield_pct/100 / 365)^n`, where `n` is the number of whole days from the position's start date to today. The start date is optional; if it is missing, the value stays at the entered nominal, so 0.1.0 files open unchanged.

**T-Bills and other** positions stay at face value (zero-coupon): the kind selects the formula, the yield alone is not enough.

The grown value is computed at read time and never written back to the JSON file — `principal_usd` remains the original nominal. Day count is 365 (not 365.25, not 30/360).

In the UI overlay, the date field is labeled **start date** for deposits and **maturity** for T-Bills/other. The PRINCIPAL column and the CAPITAL KPI show the current (grown) value for deposits.

## Status bar

`manifest.json` is at the repository root, so this repo is the Omarchy plugin. The pill shows capital and yield and refreshes EURUSD / EURYEN from Frankfurter every hour. Click it for the same book as the TUI. Middle-click refreshes now.

The plugin is a display frontend and does not bundle the `trackfolio` executable. Both are required. They install separately: the binary with Cargo, the widget with `omarchy plugin add`. The marketplace Install button copies only the plugin line, and the bar will say `trackfolio is not installed` until the binary is installed too.

```bash
cargo install --git https://github.com/AlucarDWeb/trackfolio &&
  omarchy plugin add https://github.com/AlucarDWeb/trackfolio.git --enable
```

Install asks where to put the pill: `left`, `center`, or `right`. The same choice is available after install:

```bash
omarchy plugin enable io.github.alucardweb.trackfolio --section left
omarchy bar move io.github.alucardweb.trackfolio --section center
omarchy bar move io.github.alucardweb.trackfolio --section left --after tornikegomareli.spaces
```

`defaultSection` in the manifest is only the suggested choice. It does not lock the widget to one side.

Panel keys match the TUI: `j`/`k` move, `a` add, `e` or Enter edit, `d` twice to delete, `q` or Esc close. Optional `shell.json` keys on the widget: `binary` (path to the executable) and `file` (portfolio path; otherwise `TRACKFOLIO_FILE` or `~/.local/share/trackfolio/portfolio.json`).

Machine interface, same validation and the same file:

```bash
trackfolio json show
trackfolio json show --fx
trackfolio json apply '{"op":"add","name":"Bill","kind":"tbill","currency":"USD","amount":"1000","yield":"5","date":"2026-12-31"}'
```

`show --fx` still returns the book if Frankfurter fails. `apply` accepts `add`, `edit`, and `delete`. `--file PATH` overrides the portfolio path.


## License

MIT — see [LICENSE](LICENSE).
