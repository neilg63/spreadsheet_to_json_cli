[![mirror](https://img.shields.io/badge/mirror-github-blue)](https://github.com/neilg63/spreadsheet_to_json_cli)
[![crates.io](https://img.shields.io/crates/v/spread-cli.svg)](https://crates.io/crates/spread-cli)
[![docs.rs](https://docs.rs/spread-cli/badge.svg)](https://docs.rs/spread-cli)

# Spreadsheet to JSON CLI (spread-cli)

This crate provides a simple command line interface to convert common spreadsheet and CSV files into JSON or JSONL (JSON Lines) files suitable data interchange.

It supports the following formats:

- Excel 2007+ Workbook (*.xlsx*)
- Excel 2007+ Binary (*.xlsb*)
- Excel 97-2004 Legacy (*.xls*)
- OpenDocument Spreadsheets (*.ods*) compatible with LibreOffice
- CSV: comma separated values (*.csv*)
- TSV: tab-separated values (*.tsv*)

Spreadsheets are processed via the *Calamine* library and CSV/TSV files by the CSV library.

## Spreadsheet notes

By default, field names come from the header row (the first row, unless you point `--header_row` at a different one), snake_cased -- e.g. a header of "Gross Annual Salary (USD)" becomes the field key `gross_annual_salary_usd`. A1-style letters (`a`, `b`, `c`, ... `z`, `aa`, `ab`, ...) are only used as a *fallback*, and only for individual columns that don't have usable header text (an empty header cell, or when `--omit_header` is set) -- they are not the default naming scheme.

For wide spreadsheets (20+ columns) where the original headers are long or awkwardly worded, it's often easier to force *every* column to use short A1 letters (or `c01`/`c02`/... zero-padded numbers) with `--colstyle`/`-c`, then reassign the ones you care about by letter with `--keys` -- rather than typing out each long snake_cased header name in full:

```sh
spread-cli my-spreadsheet.xlsx -c "a1" --keys "a:first_name,b:last_name,c:salary,d:start_date"
```

This is especially handy when a header is genuinely unwieldy to reference by name, e.g. "Gross Annual Salary (USD)" -- `-c "a1"` turns it (and every other column) into a plain letter first, so you only need to know its position (`c`), not retype the header text.


## Options:
- ```path``` Local path on the file system to the source spreadsheet
- ```--sheet, -s``` case-insensitive sheet name ignoring spaces and punctuation
- ```--index, -i``` sheet index (0 is the first) for spreadsheets
- ```--euro_number_format, -e```: convert European-style decimal commas, when converting from formatted strings to numbers
- ```--date_only``` date-times columns are processed as dates only by default, unless overridden
- ```--keys, -k```: comma-separated list of column overrides, each in the form ```source_key[:new_key][|format[|default]]```. `source_key` is matched against the column's natural (auto-detected, snake_cased) header key wherever that column actually is, so you only need to list the columns you want to change. A `source_key` that doesn't match any column in the file is silently ignored. Omit `:new_key` to change only the format/default and keep the natural name. A single `--keys` value can mix and match several overrides, comma-separated:
  - `--keys "start_date|date"` casts `start_date` to a date, keeping its natural name
  - `--keys "start_date:start|date"` renames `start_date` to `start` and casts it to a date
  - `--keys "start_date:start|date,total_price:total"` does both of the above, and renames `total_price` to `total` with no format change
- ```--max, -m``` max number of rows
- ```--header_row, -t``` row index used for the header row, if it is not the first row. This is only applicable to spreadsheets and useful if the top rows contain headers or descriptions
- ```--omit_header, -o``` skip the header and assign columns to letters (a, b, c, d .... z, aa, ab etc..)
- ```--colstyle, -c```: overrides the fallback column-naming convention for columns without a usable header, in the form ```style[:mode]```. `style` is `a1` for spreadsheet-style letters (`a`, `b`, ... `z`, `aa`, `ab`, ...) or `c01`/`n`/`r1`/`r1c1` for zero-padded numbers (`c01`, `c02`, ...) -- `r1`/`r1c1` are accepted as aliases for `c01` since that's a more familiar convention if you're used to R1C1-style spreadsheet references, but a bare `r` (with no `1`) is not a recognised style. The zero-padding width scales with the sheet's total column count, so keys sort correctly regardless of width: `c01`..`c99` under 100 columns, `c001`..`c999` from 100 up to 1,000, `c0001`..`c9999` from 1,000 up to 10,000. `mode` controls whether this replaces *every* column's name or only fills in for columns lacking a real header: `all` (or the default when `:mode` is omitted entirely, e.g. `-c c01`) renames every column, matching what you'd see as column letters in a spreadsheet app; anything else (e.g. `-c a1:auto`) only applies to columns without their own header text, leaving named columns alone.
- ```--deferred, -d``` For large files: streams rows straight to a `.jsonl` file one at a time rather than holding them all in memory (the file is always plain JSON Lines, one object per line -- there's no "standard JSON array" mode for `--deferred`, since that would need to buffer the whole result to know where to put the closing bracket, defeating the point). By default the file goes to a random-UUID filename under `EXPORT_FILE_DIRECTORY` (a `.env` variable, default `./`); use `--file`/`-f` to name it yourself. **On Linux and macOS**, this also hands the export off to a detached background process and returns control to the shell immediately, rather than blocking until the whole file is processed -- worth it once you're talking millions of rows; for a few thousand it'll finish before you'd notice either way. Prints `exporting to {path} in the background (see {path}.log for progress and errors)` (or `{"output_reference": ..., "log_file": ..., "background": true}` with `--json`) right away. Since there's no terminal attached to the background process by the time it finishes (or fails), check `{path}.log` afterward to confirm it completed -- there's no other way to be notified. **On Windows** (or anywhere else non-Unix), `--deferred` falls back to the same in-process, streamed-but-blocking behavior it always had -- still memory-efficient, just not backgrounded.
- ```--file, -f``` export file path for `--deferred`; overrides the random UUID filename. Creates any missing parent directories. Has no effect without `--deferred`.
- ```--json, -j``` Formats JSON output as indented, multi-line JSON. Does not change *what* gets printed -- that's still up to `--rows`/`--lines` (or neither) exactly as without `--json`; see [Using with jq](#using-with-jq) below
- ```--preview``` show preview of the first 10 lines only
- ```--rows, -r``` print just the data rows (no parsing metadata), as a JSON array
- ```--lines, -l``` JSON lines: one compact JSON object per row, with no surrounding array (JSONL/NDJSON). Implies `--rows` on its own -- no need to pass both -- and if you do, `--lines` wins
- ```--debug``` prints processing time and, on error, extra diagnostic detail (the raw internal error code plus the options that were applied). This is CLI-side timing only -- there's no such thing as "debug mode" in the underlying spreadsheet-to-json library. It never writes to stdout when the output is JSON: with the full `--json` object it's added as a real `processing_time_ms` field instead; with `-r --json`/`-l` (both bare arrays or JSONL) it goes to stderr, since there's no metadata slot to embed it in without breaking those shapes.

## Using with `jq`

`--json` is a *formatting* flag, not a mode switch: it makes JSON output properly indented and multi-line, without changing which content gets printed. What gets printed is still decided by `--rows`/`--lines` (or neither) exactly as without `--json`:

- neither `-r` nor `-l`: the full result, parsing metadata plus the data, nested under `"data"`
- `-r` (rows only): just the data rows, as a JSON array
- `-l` (lines): one compact JSON object per row (JSONL/NDJSON) -- `--json` has no effect here, since one-record-per-line is a different structural format, not an indentation style

```sh
# full result: metadata (extension, sheets, row_count, fields, ...) + data together
spread-cli --json sales.xlsx | jq '.data[] | {sku, price}'
spread-cli --json sales.xlsx | jq 'del(.data)'                    # metadata only
spread-cli --json --preview workbook.xlsx | jq '.data[] | {sheet, row_count}'  # every sheet

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


