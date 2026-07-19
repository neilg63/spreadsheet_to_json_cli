[![mirror](https://img.shields.io/badge/mirror-github-blue)](https://github.com/neilg63/spreadsheet_to_json_cli)
[![crates.io](https://img.shields.io/crates/v/spread-cli.svg)](https://crates.io/crates/spread-cli)
[![docs.rs](https://docs.rs/spread-cli/badge.svg)](https://docs.rs/spread-cli)

# Spreadsheet to JSON CLI (spread-cli)

Converts spreadsheets and CSV/TSV files to JSON, JSON Lines, or plain text -- designed to pipe straight into `jq`/`yq`.

Supported formats:

- Excel 2007+ Workbook (*.xlsx*)
- Excel 2007+ Macro-Enabled Workbook (*.xlsm*) -- read as plain data; macros are ignored
- Excel 2007+ Binary (*.xlsb*)
- Excel 97-2004 Legacy (*.xls*)
- OpenDocument Spreadsheets (*.ods*) compatible with LibreOffice
- CSV: comma separated values (*.csv*)
- TSV: tab-separated values (*.tsv*)

## Installation

```sh
cargo install spread-cli
```

Installs the `spread-cli` binary to `~/.cargo/bin`, already on your `PATH` via [rustup](https://rustup.rs) -- ready to use right away. (This is `cargo install`, not `cargo add` -- `cargo add` only adds a dependency to a Rust project; it won't put a binary on your `PATH`.)

## Quick start

```sh
spread-cli sales.xlsx                     # full result + metadata, human-readable (not valid JSON on its own)
spread-cli sales.xlsx -j                  # the same result, as one valid JSON object
spread-cli sales.xlsx -r | jq '.[0]'      # just the rows, as a JSON array -- for piping into jq/yq
spread-cli sales.xlsx -l | jq -c .        # just the rows, one compact JSON object per line (JSONL)
spread-cli big.xlsx -xpj | jq '.columns'  # explore a large multi-sheet workbook before pulling data out of it
```

Four things worth knowing up front:

1. **Piping into `jq`/`yq`?** You almost always want `-r` (rows as a JSON array) or `-l` (JSON Lines) -- see [Using with jq/yq](#using-with-jq-or-yq).
2. **Want one clean JSON object, metadata included?** Add `-j`/`--json`. Without it, the default full-result output is plain text (readable lines like `row count: 42`) with a JSON array only after the `data:` line -- the output as a whole is *not* valid JSON without `-j`. `-r`/`-l` output is always valid JSON/JSONL either way; `-j` there only changes indentation.
3. **Exploring an unfamiliar, large, multi-sheet workbook?** Start with `-x` (structural overview -- sheet names, row counts, field names, no cell values) combined with `-p` (every sheet, not just one); `-xpj` gives a clean JSON version. Then target the sheet(s)/field(s) you actually want with `-s`/`--keys`/`-r`/`-l`.
4. **Sheet and row numbers are 1-based** by default (`-n`, `-t`, `-b`), matching what you'd see in a spreadsheet app. 0-based equivalents exist for scripts that already track things by index -- see [Options](#options).

## Spreadsheet notes

Field names come from the header row, snake_cased (e.g. "Gross Annual Salary (USD)" becomes `gross_annual_salary_usd`). If the header isn't on the first row, `spread-cli` detects it automatically -- title/notes rows above it, and a gap before the real data starts, are recognized and skipped. Override with `-t`/`-b` if it guesses wrong, or `--omit-header` if there's no header row at all.

For wide spreadsheets (20+ columns) with long or awkward header text, it's often easier to force every column to a short A1 letter with `-c a1`, then reassign the ones you care about by letter with `--keys`, rather than typing out each header name in full:

```sh
spread-cli my-spreadsheet.xlsx -c a1 --keys "a:first_name,b:last_name,c:salary,d:start_date"
```

## Options

- ```path``` Local path to the source spreadsheet or CSV/TSV file
- ```-s, --sheet``` case-insensitive sheet name, ignoring spaces/punctuation; falls back to the first sheet if unmatched
- ```-n, --number``` sheet number, 1-based (`-n 1` is the first sheet)
- ```-k, --keys```: comma-separated column overrides, `source_key[:new_key][|format[|default]]`. `source_key` matches the column's natural (auto-detected, snake_cased) header name wherever it actually is, so you only list the columns you want to change -- an unmatched `source_key` is silently ignored. Omit `:new_key` to change only the format/default:
  - `--keys "start_date|date"` casts `start_date` to a date, keeping its name
  - `--keys "start_date:start|date"` also renames it to `start`
  - `--keys "start_date:start|date,total_price:total"` mixes several overrides in one value
  - `date` is one of several date/time format codes -- see the table under `--date-only` below
- ```-m, --max``` max rows *per sheet* (with `-p`, every sheet gets its own cap, default 10)
- ```-t, --top``` header row number, 1-based, if the header isn't on the first row -- e.g. a title/notes row above it. If not given, the header row is detected automatically.
- ```-b, --body-start``` row number, 1-based, where the real data begins, if there's a gap below the header (a blank or subtitle row). Rows between the header and this one are skipped entirely. Defaults to immediately after the header row.
  - Example: title in row 1, notes in row 2, real header in row 3, blank row before data in row 5 -- `-t 3 -b 5` skips straight past the blank row instead of capturing it as a row of nulls.
  - With `-j`, the row indices *actually used* -- whether from `-t`/`-b` or auto-detection -- come back as `header_row`/`body_start` (1-based) and `header_index`/`body_index` (0-based).
- ```--omit-header``` treat the file as having no header row at all; columns get fallback letter names (`a`, `b`, `c`, ... or `c01`, `c02`, ... with `--colstyle`) instead. Pair with `--keys` to give them real names: `--omit-header --keys "a:region,b:team_size,c:revenue"`.
- ```-c, --colstyle```: fallback column-naming style for columns with no usable header, `style[:mode]` -- `style` is `a1` (letters) or `c01`/`r1`/`r1c1` (zero-padded numbers); `mode` is `all` (every column, the default) or anything else to only fill in columns lacking a real header.
- ```-d, --deferred``` For large files: streams rows to a `.jsonl` file instead of holding them all in memory. Defaults to a random-UUID filename under `EXPORT_FILE_DIRECTORY` (a `.env` variable, default `./`) -- name it yourself with `-o`. On Linux/macOS this runs as a detached background process and returns immediately; check `{path}.log` afterward to confirm it finished. On Windows it runs in-process instead, still memory-efficient, just blocking.
- ```-o, --output``` export file path for `-d`; has no effect without it
- ```-j, --json``` outputs one valid, indented JSON object/array for the full result -- see [Quick start](#quick-start) above for why this matters
- ```-p, --preview``` reads every worksheet (up to `-m` rows each, default 10) instead of just one -- `-s`/`-n` are ignored in this mode. Field names for every sheet come back in a top-level `columns` map; each sheet's rows live under `data`.
- ```-x, --exclude-cells``` drops row values, keeping only structure -- sheet names, row counts, field names. Combine with `-p` (`-xp`) for a full-workbook overview with no cell data at all: `columns` (fields per sheet) and `row_counts` (rows per sheet), no `data` key. See [Quick start](#quick-start).
- ```-r, --rows``` just the data rows, as a JSON array, no metadata wrapper
- ```-l, --lines``` JSON Lines: one compact object per row, no wrapper -- implies `-r`
- ```--euro-number-format```: parse decimal commas when converting formatted strings to numbers
- ```--date-only``` / ```--time-only``` / ```--hm-only``` / ```--simple``` format every date-time column as a date, a time, hours:minutes, or a full date-time without milliseconds/`Z`, respectively. Each wins over the ones after it if more than one is set (`--date-only` > `--time-only` > `--hm-only` > `--simple`).

  To override just one column instead, use `--keys "col|<code>"`:

  | code | example output | notes |
  | --- | --- | --- |
  | `dt`/`datetime` (or no override) | `2026-07-19T19:58:45.000Z` | the default: full precision, JS/JSON-interop-friendly |
  | `ds`/`simple` | `2026-07-19T19:58:45` | full date-time, no milliseconds or `Z` |
  | `da`/`date` | `2026-07-19` | date only |
  | `ti`/`time` | `19:58:45` | time only, with seconds |
  | `hm` | `19:58` | time only, hours and minutes |

  e.g. `--keys "logged_at|hm"` renders just `logged_at` as `"19:58"`, leaving every other date-time column at its row-wide default. A cell that's genuinely just a time of day (e.g. an Excel cell formatted as plain `hh:mm`, which Excel stores internally as a full datetime with no real date) is rendered as a bare time automatically under the default or `ds` output; explicitly forcing `da`/`date` on such a cell surfaces Excel's placeholder date (`1899-12-31`) instead, since you're asking for a date component it doesn't really have.

  Other date/time transformations (reformatting, day-of-week, timezone conversion) are out of scope here -- pipe into `jq`/`yq`, or use the `spreadsheet-to-json` library crate directly for Rust code.
- ```--debug``` prints processing time and extra diagnostic detail on error, to stderr when the main output is JSON (so it never corrupts a piped result)

0-based equivalents, for scripts that already track rows/sheets by index rather than the friendly 1-based numbers above: `--index` (= `-n`), `--header-index` (= `-t`), `--body-index` (= `-b`).

## Using with `jq` (or `yq`)

For piping, reach for `-r` (rows as a JSON array) or `-l` (JSON Lines) -- both are always valid JSON/JSONL on their own, with or without `-j`. `yq` handles either shape just as well as `jq` -- including line-delimited JSON, which it reads as a stream of separate documents:

```sh
spread-cli sales.xlsx -l | yq -p json 'select(.price > 10)'
```

`-j`/`--json` only matters for the *full* result (no `-r`/`-l`) -- see [Quick start](#quick-start) -- and it's a pure formatting flag there too: indentation, not content.

```sh
# full result: metadata (extension, sheets, row_count, fields, ...) + data together
spread-cli sales.xlsx -j | jq '.data[] | {sku, price}'
spread-cli sales.xlsx -j | jq 'del(.data)'                    # metadata only

# -p: every worksheet, not just the selected one -- field names per sheet in "columns",
# rows come back per-sheet under "data"
spread-cli workbook.xlsx -pj | jq '.columns'                     # {sheet: [fields], ...}
spread-cli workbook.xlsx -pj | jq '.data[] | {sheet, row_count}' # every sheet's row count

# -xpj: a quick structural overview of every worksheet, no row data at all -- just sheet
# names, field names ("columns"), and row counts ("row_counts"). Ideal for large
# multi-sheet files (e.g. statistics-agency spreadsheets) before deciding what to pull.
spread-cli workbook.xlsx -xpj | jq '.columns'
spread-cli workbook.xlsx -xpj | jq '.row_counts'

# -rj (or -r --json): just the rows, as a pretty-printed array, no metadata wrapper
spread-cli sales.xlsx -rj | jq '.[] | select(.price > 10)'
spread-cli sales.xlsx -rj -k "date|date" | jq '.[] | { date, total_price }'
spread-cli sales.xlsx -rj | jq -r '.[] | [.sku, .name, .price] | @csv'

# -l: plain JSON Lines, one row per line -- best for streaming into another
# NDJSON-consuming tool, or very large files (jq/yq consume it line-by-line rather than
# waiting for one big array to finish printing)
spread-cli sales.xlsx -l | jq -c 'select(.price > 10)'
spread-cli sales.xlsx -l | jq -c '{sku, total: (.price * .qty)}' > sales.ndjson
spread-cli sales.xlsx -l | yq -p json -o yaml 'select(.price > 10)'
```
