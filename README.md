# Trackfolio

A terminal UI for tracking a personal book of US T-Bills and USD deposits: capital, weighted yield, and interest projections (day / week / month / year), with full CRUD over positions.

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

Money and yields are stored as decimal strings and computed with exact decimal arithmetic. EUR positions are converted to USD once via the [Frankfurter](https://frankfurter.app) API at entry time; the FX rate and date are persisted with the position.

## License

MIT — see [LICENSE](LICENSE).
