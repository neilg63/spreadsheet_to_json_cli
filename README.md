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
- ```--keys, -k```: comma-separated list of column overrides, each in the form ```source_key[:new_key][|format[|default]]```. `source_key` is matched against the column's natural (auto-detected, snake_cased) header key wherever that column actually is, so you only need to list the columns you want to change -- not pad out the ones ahead of them. A `source_key` that doesn't match any column in the file is silently ignored. Omit `:new_key` to change only the format/default and keep the natural name (e.g. `weight_kg|int`). Example: `--keys "weight_kg:weight|int"` matches the column naturally keyed `weight_kg`, renames it to `weight`, and casts its values to integers.
- ```--max, -m``` max number of rows
- ```--header_row, -t``` row index used for the header row, if it is not the first row. This is only applicable to spreadsheets and useful if the top rows contain headers or descriptions
- ```--omit_header, -o``` skip the header and assign columns to letters (a, b, c, d .... z, aa, ab etc..)
- ```--deferred, -d``` Defer row processing to an asynchronous task
- ```--json, -j``` Output all info as JSON with data rows in "data"
- ```--preview``` show preview of the first 10 lines only
- ```--lines, -l``` JSON lines, one json object per line. Ideal for debugging and reading long files asynchronously
- ```--debug``` debug mode

NB: This is still an alpha release
