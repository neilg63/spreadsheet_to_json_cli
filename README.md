# Spreadsheet to JSON CLI

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

If all columns from the left are populated, then automatic column field assignment should match columns in the *A1+* format. If the first column is empty, then it will be skipped. the same logic applies to rows. The default header keys come from the first populated row unless overridden with the ```--keys``` flag.


## Options:
- ```path``` Local path on the file system to the source spreadsheet
- ```--sheet, -s``` case-insensitive sheet name ignoring spaces and punctuation
- ```--index, -i``` sheet index (0 is the first) for spreadsheets
- ```--euro_number_format, -e```: convert European-style decimal commas, when converting from formatted strings to numbers
- ```--date_only``` date-times columns are processed as dates only default, unless overridden
- ```--keys, -k```: comma-separated list of column overrides, each in the form ```source_key[:new_key][|format[|default]]```. `source_key` is matched against the column's natural (auto-detected, snake_cased) header key wherever that column actually is, so you only need to list the columns you want to change -- not pad out the ones ahead of them. A `source_key` that doesn't match any column in the file is silently ignored. Omit `:new_key` to change only the format/default and keep the natural name. A single `--keys` value can mix and match several overrides, comma-separated:
  - `--keys "start_date|date"` casts `start_date` to a date, keeping its natural name
  - `--keys "start_date:start|date"` renames `start_date` to `start` and casts it to a date
  - `--keys "start_date:start|date,total_price:total"` does both of the above, and renames `total_price` to `total` with no format change
- ```--max, -m``` max number of rows
- ```--header_row, -t``` row index used for the header row, if it is not the first row. This is only applicable to spreadsheets and useful if the top rows contain headers or descriptions
- ```--omit_header, -o``` skip the header and assign columns to letters (a, b, c, d .... z, aa, ab etc..)
- ```--colstyle, -c```: overrides the fallback column-naming convention for columns without a usable header, in the form ```style[:mode]```. `style` is `a1` for spreadsheet-style letters (`a`, `b`, ... `z`, `aa`, `ab`, ...) or `c01`/`n` for zero-padded numbers (`c01`, `c02`, ...). `mode` controls whether this replaces *every* column's name or only fills in for columns lacking a real header: `all` (or the default when `:mode` is omitted entirely, e.g. `-c c01`) renames every column, matching what you'd see as column letters in a spreadsheet app; anything else (e.g. `-c a1:auto`) only applies to columns without their own header text, leaving named columns alone.
- ```--deferred, -d``` Defer row processing to an asynchronous task
- ```--json, -j``` Formats JSON output as indented, multi-line JSON. Does not change *what* gets printed -- that's still up to `--rows`/`--lines` (or neither) exactly as without `--json`; see [Using with jq](#using-with-jq) below
- ```--preview``` show preview of the first 10 lines only
- ```--rows, -r``` print just the data rows (no parsing metadata), as a JSON array
- ```--lines, -l``` JSON lines: one compact JSON object per row, with no surrounding array (JSONL/NDJSON). Implies `--rows` on its own -- no need to pass both -- and if you do, `--lines` wins
- ```--debug``` debug mode

## Using with `jq`

`--json` is a *formatting* flag, not a mode switch: it makes JSON output properly indented and multi-line, without changing which content gets printed. What gets printed is still decided by `--rows`/`--lines` (or neither) exactly as without `--json`:

- neither `-r` nor `-l`: the full result -- parsing metadata plus the data, nested under `"data"`
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

NB: This is still an alpha release
