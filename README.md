[![mirror](https://img.shields.io/badge/mirror-github-blue)](https://github.com/neilg63/spreadsheet_to_json_cli)
[![crates.io](https://img.shields.io/crates/v/spread-cli.svg)](https://crates.io/crates/spread-cli)
[![docs.rs](https://docs.rs/spread-cli/badge.svg)](https://docs.rs/spread-cli)

# Spreadsheet to JSON CLI (spread-cli)

This crate provides a simple command line interface to convert common spreadsheet and CSV files into JSON or JSONL (JSON Lines) files suitable for data interchange.

It supports the following formats:

- Excel 2007+ Workbook (*.xlsx*)
- Excel 2007+ Macro-Enabled Workbook (*.xlsm*) -- read as plain data; macros are ignored
- Excel 2007+ Binary (*.xlsb*)
- Excel 97-2004 Legacy (*.xls*)
- OpenDocument Spreadsheets (*.ods*) compatible with LibreOffice
- CSV: comma separated values (*.csv*)
- TSV: tab-separated values (*.tsv*)

Spreadsheets are processed via the *Calamine* library and CSV/TSV files by the CSV library.

## Installation

```sh
cargo install spread-cli
```

This downloads, builds, and installs the `spread-cli` binary into `~/.cargo/bin`, which `rustup`'s installer already puts on your `PATH` -- so `spread-cli` is available as a normal shell command right away, no separate download or PATH setup needed. Requires the Rust toolchain (via [rustup](https://rustup.rs)). Note this is `cargo install`, not `cargo add` -- `cargo add` only adds a crate as a *dependency* of whatever project you're currently in; it won't put a `spread-cli` binary on your PATH.

## Spreadsheet notes

By default, field names come from the header row (the first row, unless you point `--top`/`-t` at a different one), snake_cased e.g. a header of "Gross Annual Salary (USD)" becomes the field key `gross_annual_salary_usd`. A1-style letters (`a`, `b`, `c`, ... `z`, `aa`, `ab`, ...) are only used as a *fallback*, and only for individual columns that don't have usable header text (an empty header cell, or when `--omit-header` is set) -- they are not the default naming scheme.

For wide spreadsheets (20+ columns) where the original headers are long or awkwardly worded, it's often easier to force *every* column to use short A1 letters (or `c01`/`c02`/... zero-padded numbers) with `--colstyle`/`-c`, then reassign the ones you care about by letter with `--keys` -- rather than typing out each long snake_cased header name in full:

```sh
spread-cli my-spreadsheet.xlsx -c "a1" --keys "a:first_name,b:last_name,c:salary,d:start_date"
```

This is especially handy when a header is genuinely unwieldy to reference by name, e.g. "Gross Annual Salary (USD)" -- `-c "a1"` turns it (and every other column) into a plain letter first, so you only need to know its position (`c`), not retype the header text.


## Options:
- ```path``` Local path on the file system to the source spreadsheet
- ```--sheet, -s``` case-insensitive sheet name ignoring spaces and punctuation
- ```--index, -i``` sheet index (0 is the first) for spreadsheets
- ```--number, -n``` sheet number (1 is the first) -- the same as `--index` but 1-based, for matching how you'd count sheets off in a spreadsheet app ("the 3rd sheet") without the usual off-by-one. `-n 1` is the same as `-i 0` (or `-s sheet1`, if the first sheet happens to be named "sheet1" -- `--sheet` is case-insensitive). Cannot be combined with `--index` -- they're two ways of saying the same thing, so passing both is rejected rather than silently picking one.
- ```--keys, -k```: comma-separated list of column overrides, each in the form ```source_key[:new_key][|format[|default]]```. `source_key` is matched against the column's natural (auto-detected, snake_cased) header key wherever that column actually is, so you only need to list the columns you want to change. A `source_key` that doesn't match any column in the file is silently ignored. Omit `:new_key` to change only the format/default and keep the natural name. A single `--keys` value can mix and match several overrides, comma-separated:
  - `--keys "start_date|date"` casts `start_date` to a date, keeping its natural name
  - `--keys "start_date:start|date"` renames `start_date` to `start` and casts it to a date
  - `--keys "start_date:start|date,total_price:total"` does both of the above, and renames `total_price` to `total` with no format change
  - `date` above is one of several date/time format codes -- see the full table under `--date-only` below
- ```--max, -m``` max number of rows *per sheet*. This is the only row-count cap there is -- with `--preview`, every worksheet in the file is always included, `--max`/`-m` just limits how many rows come back from each one (default 10 under `--preview`, see below)
- ```--top, -t``` header row number, 1-based (1 is the first row), if the headers aren't on the first row -- e.g. title or notes rows above the real header. Works for CSV/TSV as well as spreadsheets. Same `--index`/`--number` pairing pattern as sheet selection: `--top` is the friendly 1-based form; `--header-index` is its 0-based equivalent, passed straight through to the underlying library's `OptionSet.header_row`. `-t 1` is the same as `--header-index 0` (both mean "the first row"). Cannot be combined with `--header-index`. **If neither is given** (the default), the header row is guessed automatically: the sheet's first ~20 rows are sampled, and the header candidate is the first row that's *fully populated* (every column has a value) -- not just "wide", and not just "the most common width". This matters because individual data rows commonly leave optional fields blank (e.g. a "website" or "LinkedIn profile" column that's empty for some people), so the *most common* row width often reflects the data's incompleteness rather than the table's true shape; a header, by contrast, always labels every column. Data rows are then recognized as anything at least half as wide as the header, allowing for blank optional fields, while title/notes rows (which typically populate only one or two cells) are skipped either way. Beyond width, a header candidate is checked against what it actually contains: data cells are commonly numeric, boolean, or date/datetime-shaped, while header labels are almost always plain text -- except bare 4-digit years (e.g. "2020" columns in a year-by-year table), which are recognized as legitimate labels rather than data. For spreadsheets with no numeric/boolean/date content anywhere at all (common for pure content-migration/translation files) *and* no header row uniquely wider than the data below it, cell length is used as a last resort instead: a candidate whose cells are much shorter than the rows below it is treated as labels; if that's not confidently the case either, `spread-cli` concludes no real header exists at all and falls back to A1-style field names (`a`, `b`, `c`, ...) with every row treated as data -- the same outcome as `--omit-header`, so no row is ever silently lost by being misread as a header. Falls back to assuming row 0 is the header whenever none of this is confident enough (e.g. a single-column file). This detection is `spread-cli`'s own default behavior, not the underlying library's -- `spreadsheet-to-json` itself only detects when a caller opts in via `OptionSet::detect_header()`, so direct library use always gets the plain "assume row 0" default unless asked otherwise.
- ```--body-start, -b``` row number, 1-based, where the actual data begins, if there's a gap below the header row -- e.g. a blank or subtitle row before the real data starts. Rows between the header row and this one are skipped entirely, neither captured as headers nor as data. `--body-index` is the 0-based equivalent (`OptionSet.data_row_index` directly); cannot combine `--body-start` and `--body-index`. Defaults to immediately after the header row when unset. Setting it *equal to* the header row is valid and useful -- e.g. a CSV with predefined/external headers (supplied via `--keys`) combined with `--omit-header`, where no line in the file is consumed as a header at all; only a value *strictly before* the header row is rejected, falling back to the default instead. Applies to CSV/TSV as well as spreadsheets, like `--top`. Example: a file with a title in row 1, notes in row 2, real headers in row 3, and a blank row before data starts in row 5 -- `--top 3 --body-start 5` (or `--header-index 2 --body-index 4`) skips straight past the blank row instead of capturing it as a row of nulls. **If neither `--top`/`--header-index` nor `--body-start`/`--body-index` is given**, the same auto-detection pass picks the first data row too -- e.g. a title, a header, an explanatory-text row, then the real data: the text row is detected as too narrow to be a table row and skipped, without needing any of these four flags set explicitly.
- ```--omit-header``` treat the source as having no header row at all -- no line in the file is ever consumed as one, and columns are named with fallback letters (`a`, `b`, `c`, ... `z`, `aa`, `ab`, ...; or `c01`/`c02`/... with `--colstyle`) instead of derived from header text. Since no row is being consumed for headers, the default first data row shifts to row 0 (or `--top`/`--header-index`, if also set) rather than the row *after* it -- i.e. every row is data, nothing gets skipped, unless `--body-start`/`--body-index` says otherwise. Combine with `--keys` to give the fallback letters real names, e.g. for a predefined-schema CSV with no header line: `--omit-header --keys "a:region,b:team_size,c:revenue"`.
- ```--colstyle, -c```: overrides the fallback column-naming convention for columns without a usable header, in the form ```style[:mode]```. `style` is `a1` for spreadsheet-style letters (`a`, `b`, ... `z`, `aa`, `ab`, ...) or `c01`/`n`/`r1`/`r1c1` for zero-padded numbers (`c01`, `c02`, ...) -- `r1`/`r1c1` are accepted as aliases for `c01` since that's a more familiar convention if you're used to R1C1-style spreadsheet references. The zero-padding width scales with the sheet's total column count, so keys sort correctly regardless of width: `c01`..`c99` under 100 columns, `c001`..`c999` from 100 up to 1,000, `c0001`..`c9999` from 1,000 up to 10,000. `mode` controls whether this replaces *every* column's name or only fills in for columns lacking a real header: `all` (or the default when `:mode` is omitted entirely, e.g. `-c c01`) renames every column, matching what you'd see as column letters in a spreadsheet app; anything else (e.g. `-c a1:auto`) only applies to columns without their own header text, leaving named columns alone.
- ```--deferred, -d``` For large files: streams rows straight to a `.jsonl` file one at a time rather than holding them all in memory (the file is always plain JSON Lines, one object per line -- there's no "standard JSON array" mode for `--deferred`, since that would need to buffer the whole result to know where to put the closing bracket, defeating the point). By default the file goes to a random-UUID filename under `EXPORT_FILE_DIRECTORY` (a `.env` variable, default `./`); use `--output`/`-o` to name it yourself. **On Linux and macOS**, this also hands the export off to a detached background process and returns control to the shell immediately, rather than blocking until the whole file is processed -- worth it once you're talking millions of rows; for a few thousand it'll finish before you'd notice either way. Prints `exporting to {path} in the background (see {path}.log for progress and errors)` (or `{"output_reference": ..., "log_file": ..., "background": true}` with `--json`) right away. Since there's no terminal attached to the background process by the time it finishes (or fails), check `{path}.log` afterward to confirm it completed -- there's no other way to be notified. **On Windows** (or anywhere else non-Unix), `--deferred` falls back to the same in-process, streamed-but-blocking behavior it always had -- still memory-efficient, just not backgrounded.
- ```--output, -o``` export file path for `--deferred`; overrides the random UUID filename. Creates any missing parent directories. Has no effect without `--deferred`.
- ```--json, -j``` Formats JSON output as indented, multi-line JSON. Does not change *what* gets printed -- that's still up to `--rows`/`--lines` (or neither) exactly as without `--json`; see [Using with jq](#using-with-jq) below
- ```--preview, -p``` **which sheets get read**: switches to multi-sheet mode and samples up to `--max`/`-m` rows (default 10) from *every* worksheet, not just the selected one -- `--sheet`/`--index` are ignored in this mode, since the whole point is to see every sheet at once. A workbook with several sheets can therefore return more than 10 rows in total, since each sheet gets its own cap. With `--json`, sheet names throughout the output (`sheets`, and each sheet's `sheet` key) are shown snake_cased -- the same form `--sheet` matches against, so whatever's displayed can be pasted straight back in. Field names for every sheet are collected into a top-level `columns` map (`{sheet_key: [field_names]}`) instead of the single-sheet `fields` array; each worksheet's own row count and rows live under `data`, one block per sheet.
- ```--exclude-cells, -x``` **whether cell values are included**: drops row *data* from the result while keeping everything structural -- sheet names, row counts, column/field names -- with no actual cell values. It has two effective shapes, depending on whether `--preview` is also set:
  - `-x` alone: a **single-sheet preview**. Only the one selected (or default) sheet is read, same as without `-x`; `data` is simply omitted from `--json` output, since it would always be `[]`. Field names still come back as the usual top-level `fields` array.
  - `-x` combined with `--preview` (`-xp`): effectively becomes a **multi-sheet preview** -- every worksheet is read structurally (name, row count, field names), with no cell data at all. Since every sheet's fields already live in the top-level `columns` map (see `--preview` above), a `fields` array would just be a redundant copy of one sheet's entry in `columns` -- so it's dropped from the output entirely in this mode. `data` is replaced by `row_counts`, a plain `{sheet_key: row_count}` map -- with no rows and no fields left to carry, an array of `{sheet, row_count}` objects under `data` would be more ceremony than the one number per sheet it actually holds. Handy for large multi-sheet files with many worksheets (a common shape for spreadsheets published by statistics agencies) where you want to know what's in the file before deciding what to actually pull out of it:
    ```json
    {
      "columns": { "sheet_1": ["col_1", "col_2", "col_3"], "sheet_2": ["col_a", "col_b"] },
      "row_counts": { "sheet_1": 9876, "sheet_2": 34 }
    }
    ```

  Without `--json`, `-x` prints the configured options instead (unchanged, pre-existing behavior). Example for a quick multi-sheet overview: `spread-cli -px --json workbook.xlsx | jq '{columns, row_counts}'`.
- ```--rows, -r``` print just the data rows (no parsing metadata), as a JSON array
- ```--lines, -l``` JSON lines: one compact JSON object per row, with no surrounding array (JSONL/NDJSON). Implies `--rows` on its own -- no need to pass both -- and if you do, `--lines` wins
- ```--euro-number-format```: convert decimal commas, when converting from formatted strings to numbers
- ```--date-only``` formats every date-time column as a bare date, discarding the time entirely, e.g. `2026-07-19`. A column's own format override (`--keys "col|<code>"`, see the table below) always wins over this row-wide default; if more than one of `--date-only`/`--time-only`/`--hm-only`/`--simple` is set, `--date-only` wins.
- ```--time-only``` formats every date-time column as a bare time, discarding the date entirely, e.g. `19:58:45`. Same column-override precedence as `--date-only`; loses to `--date-only` if both are set.
- ```--hm-only``` formats every date-time column as hours and minutes only, discarding seconds and the date entirely, e.g. `19:58` -- handy for a start/end time or a recurring daily slot where second-level precision is just noise. Loses to `--date-only`/`--time-only` if either is set too.
- ```--simple``` formats every date-time column as a full date-time, but without the milliseconds or trailing `Z` that the default output carries for JS/JSON interop, e.g. `2026-07-19T19:58:45` instead of `2026-07-19T19:58:45.000Z`. Loses to `--date-only`/`--time-only`/`--hm-only` if any of those are set too.

  These four flags are row-wide defaults, applying to every date-time column at once. To format just one column differently -- or to override the row-wide default for a single column -- use `--keys "col|<code>"` with one of:

  | code | example output | notes |
  | --- | --- | --- |
  | `dt`/`datetime` (or no override at all) | `2026-07-19T19:58:45.000Z` | the default: full precision, JS/JSON-interop-friendly |
  | `ds`/`simple` | `2026-07-19T19:58:45` | full date-time, no milliseconds or `Z` |
  | `da`/`date` | `2026-07-19` | date only |
  | `ti`/`time` | `19:58:45` | time only, with seconds |
  | `hm` | `19:58` | time only, hours and minutes |

  e.g. `--keys "logged_at|hm"` renders just the `logged_at` column as `"19:58"` while every other date-time column keeps the default full-precision output (or whatever `--date-only`/`--time-only`/`--hm-only`/`--simple` says, if one of those is also set). A cell that's genuinely just a time of day to begin with -- e.g. an Excel cell formatted as plain `hh:mm`, which Excel actually stores as a full datetime with no real date component -- is automatically rendered as a bare time under the default (`dt`) or `ds` output, rather than carrying a meaningless placeholder date (`1899-12-31`) through to the output. Explicitly forcing `da`/`date` on such a cell still surfaces that placeholder date, since you're asking for a date component the cell doesn't really have; `ti`/`time` and `hm` are unaffected either way, since both discard any date component regardless.

  Any other transformation of a date/time value (reformatting, extracting a day-of-week, timezone conversion, etc.) is out of scope for `spread-cli` -- pipe the JSON output through `jq`/`yq` or a script instead. For programmatic Rust use, the `spreadsheet-to-json` library crate's `Format`/`DateTimeMode` types cover the same ground directly.
- ```--debug``` prints processing time and, on error, extra diagnostic detail (the raw internal error code plus the options that were applied). This is CLI-side timing only -- there's no such thing as "debug mode" in the underlying spreadsheet-to-json library. It never writes to stdout when the output is JSON: with the full `--json` object it's added as a real `processing_time_ms` field instead; with `-r --json`/`-l` (both bare arrays or JSONL) it goes to stderr, since there's no metadata slot to embed it in without breaking those shapes.

## Using with `jq` (or `yq`)

`--json` output is plain, standard JSON, so anything that reads JSON works -- these examples use `jq`, but `yq` (which speaks jq-like syntax and can read JSON directly, or convert it to YAML with `-o yaml`) works just as well: `spread-cli -p --json workbook.xlsx | yq -p json -o yaml '.columns'`.

`--json` is a *formatting* flag, not a mode switch: it makes JSON output properly indented and multi-line, without changing which content gets printed. What gets printed is still decided by `--rows`/`--lines` (or neither) exactly as without `--json`:

- neither `-r` nor `-l`: the full result, parsing metadata plus the data, nested under `"data"`
- `-r` (rows only): just the data rows, as a JSON array
- `-l` (lines): one compact JSON object per row (JSONL/NDJSON) -- `--json` has no effect here, since one-record-per-line is a different structural format, not an indentation style

```sh
# full result: metadata (extension, sheets, row_count, fields, ...) + data together
spread-cli --json sales.xlsx | jq '.data[] | {sku, price}'
spread-cli --json sales.xlsx | jq 'del(.data)'                    # metadata only

# --preview (-p): every worksheet, not just the selected one -- field names for every
# sheet come back in the top-level "columns" map, rows come back per-sheet under "data"
spread-cli --json --preview workbook.xlsx | jq '.columns'                     # {sheet: [fields], ...}
spread-cli --json --preview workbook.xlsx | jq '.data[] | {sheet, row_count}' # every sheet's row count

# -px (--preview --exclude-cells) --json: a quick structural overview of every worksheet
# in a workbook with no row data at all -- just sheet names, field names ("columns") and
# row counts ("row_counts"), each a {sheet_key: ...} map. Ideal for large multi-sheet
# files (e.g. statistics-agency spreadsheets) where you want to see what's in the
# workbook before deciding what to actually pull out of it.
spread-cli -px --json workbook.xlsx | jq '.columns'
spread-cli -px --json workbook.xlsx | jq '.row_counts'

# -r --json (or the bundled short form -rj): just the rows, as a pretty-printed array --
# no metadata wrapper. Single-letter flags can be bundled like this wherever it's handy.
spread-cli -rj sales.xlsx | jq '.[] | select(.price > 10)'
spread-cli -rj sales.xlsx -k "date|date" | jq '.[] | { date, total_price }'
spread-cli -r --json sales.xlsx | jq -r '.[] | [.sku, .name, .price] | @csv'

# -l: plain JSON Lines, one row per line, no wrapper -- best for streaming into another
# NDJSON-consuming tool, or very large files (jq can consume it line-by-line rather than
# waiting for one big array/object to finish printing). -l already implies rows-only on
# its own, same as -r; no need for both -- and if you do pass both, -l wins.
spread-cli -l sales.xlsx | jq -c 'select(.price > 10)'
spread-cli -l sales.xlsx | jq -c '{sku, total: (.price * .qty)}' > sales.ndjson
```


