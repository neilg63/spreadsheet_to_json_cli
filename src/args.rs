use std::str::FromStr;

use clap::Parser;
use spreadsheet_to_json::heck::ToSnakeCase;
use spreadsheet_to_json::serde_json::{Number, Value};
use spreadsheet_to_json::FieldNameMode;
use spreadsheet_to_json::{is_truthy::*, options::{Column, DateTimeMode, OptionSet}, Format, ReadMode};
use simple_string_patterns::{SimpleMatch, StripCharacters};
use to_segments::ToSegments;

const DEFAULT_MAX_FOR_PREVIEW: u32 = 10;

/// Command line arguments configuration
#[derive(Parser, Debug)]
#[clap(
  author,
  version,
  about,
  long_about = "Converts spreadsheets (xlsx, xlsm, xls, xlsb, ods) and CSV/TSV files to JSON, \
    JSON Lines, or plain text, with control over which sheet(s), rows, columns, and field \
    names/types come out the other end. Designed to pipe straight into jq/yq -- see \
    https://github.com/neilg63/spreadsheet_to_json_cli for the full README.",
  after_help = "EXAMPLES:\n    \
    spread-cli sales.xlsx                        Read the first sheet as JSON\n    \
    spread-cli sales.xlsx -s Q1 -r               Just the rows from the sheet named \"Q1\"\n    \
    spread-cli sales.xlsx -n 2                    Read the 2nd sheet (1-based)\n    \
    spread-cli big.csv -d -o out.jsonl            Stream a large CSV to a JSONL file in the background\n    \
    spread-cli workbook.xlsx -px --json | jq .columns   Structural overview of every worksheet\n\n\
    Run with no arguments (or no PATH) to see this help again."
)]
pub struct Args {

  #[clap(help = "Path to the source spreadsheet or CSV/TSV file")]
  pub path: Option<String>,

  #[clap(short, long, value_parser, help = "Sheet name to select (case-insensitive, ignores spaces/punctuation); falls back to the first sheet if unmatched") ]
  pub sheet: Option<String>,

  #[clap(
    short = 'n', long, value_parser, conflicts_with = "index",
    help = "Sheet number, 1-based",
    long_help = "Sheet number, 1-based (1 is the first sheet). Equivalent to --index but \
      1-based, for matching how you'd count sheets off in a spreadsheet app (\"the 3rd \
      sheet\") without the usual off-by-one. `-n 1` is the same as `-i 0` (or `-s sheet1`, \
      if the first sheet happens to be named \"sheet1\"). Cannot be combined with --index."
  ) ]
  pub number: Option<u32>,

  #[clap(short, long, value_parser, help = "Maximum number of rows to return (per sheet, when combined with --preview)") ]
  pub max: Option<u32>,

  #[clap(
    short = 't', long, value_parser, conflicts_with = "header_index",
    help = "Header row number, 1-based (1 is the first row) -- if the headers aren't on the first row",
    long_help = "Header row number, 1-based (1 is the first row), if the headers aren't \
      on the first row. Equivalent to --header-index but 1-based, for matching the row \
      numbers you'd see in a spreadsheet app. `-t 1` is the same as `--header-index 0` \
      (both mean \"the first row\"). Cannot be combined with --header-index. If neither \
      is given, the header row is guessed automatically from the sheet's own layout \
      (title/notes rows are detected and skipped)."
  ) ]
  pub top: Option<u32>,
  #[clap(
    short = 'b', long, value_parser, conflicts_with = "body_index",
    help = "Row number, 1-based, where actual data begins -- if there's a gap below the header row",
    long_help = "Row number, 1-based, where actual data begins, if there's a gap below \
      the header row -- e.g. a blank or subtitle row before the real data starts. Rows \
      between the header row and this one are skipped entirely -- neither captured as \
      headers nor as data. Equivalent to --body-index but 1-based. Defaults to \
      immediately after the header row when unset. Setting it equal to the header row is \
      valid -- e.g. a CSV with predefined/external headers (via --keys) and \
      --omit-header, where no line is consumed as a header at all; only a value strictly \
      before the header row is rejected and falls back to the default. Cannot be \
      combined with --body-index."
  ) ]
  pub body_start: Option<u32>,

  #[clap(long, value_parser, default_value_t = false, help = "Skip the header row; assign fallback keys (a, b, c... or c01, c02... -- see --colstyle) instead") ]
  pub omit_header: bool, // no short flag: -o is --output's

  #[clap(
    short = 'x', long, value_parser, default_value_t = false,
    help = "Structural overview only: sheet names, row counts, field names -- no cell values",
    long_help = "Drops row *data* from the result while keeping everything structural -- \
      sheet names, row counts, column/field names -- with no actual cell values. Alone, it \
      just omits an always-empty \"data\" array from --json output for the single selected \
      sheet. Combined with --preview (-xp), it surveys the whole workbook: every sheet's \
      name, field names (\"columns\"), and row count (\"row_counts\"), with zero cell data \
      -- handy for large multi-sheet files (e.g. statistics-agency spreadsheets) where you \
      want to see what's in the file before deciding what to pull out of it."
  ) ]
  pub exclude_cells: bool,

  #[clap(
    short = 'k', long, value_parser,
    help = "Column overrides: source_key[:new_key][|format[|default]], comma-separated",
    long_help = "Comma-separated list of column overrides, each in the form \
      source_key[:new_key][|format[|default]]. source_key is matched against the column's \
      natural (auto-detected, snake_cased) header key wherever that column actually is, so \
      you only need to list the columns you want to change; an unmatched source_key is \
      silently ignored. Omit :new_key to change only the format/default and keep the \
      natural name. Examples: \"start_date|date\" casts start_date to a date; \
      \"start_date:start|date\" also renames it to start; \"a:b|int,c:d|text\" mixes \
      multiple overrides in one value."
  ) ]
  pub keys: Option<String>,

  #[clap(
    short = 'c', long, value_parser,
    help = "Fallback naming style for columns with no usable header: a1 or c01[:mode]",
    long_help = "Overrides the fallback column-naming convention for columns without a \
      usable header, in the form style[:mode]. style is a1 for spreadsheet-style letters \
      (a, b, ... z, aa, ab, ...) or c01/n/r1/r1c1 for zero-padded numbers (c01, c02, ...). \
      mode controls whether this replaces *every* column's name (\"all\", or the default \
      when :mode is omitted) or only fills in for columns lacking a real header (anything \
      else, e.g. \"a1:auto\")."
  ) ]
  pub colstyle: Option<String>,

  #[clap(short = 'j', long, value_parser, default_value_t = false, help = "Format JSON output as indented, multi-line JSON") ]
  pub json: bool,

  #[clap(
    short = 'd', long, value_parser, default_value_t = false,
    help = "Stream rows to a JSONL export file instead of returning them directly (for large files)",
    long_help = "For large files: streams rows straight to a .jsonl file one at a time \
      rather than holding them all in memory. The file defaults to a random-UUID filename \
      under EXPORT_FILE_DIRECTORY (a .env variable, default ./); use --output/-o to name it \
      yourself. On Linux and macOS, this also hands the export off to a detached background \
      process and returns control to the shell immediately; on Windows it falls back to the \
      same in-process, streamed-but-blocking behavior, still memory-efficient, just not \
      backgrounded."
  ) ]
  pub deferred: bool,

  #[clap(
    short = 'p', long, value_parser, default_value_t = false,
    help = "Sample rows from every worksheet (multi-sheet mode), not just the selected one",
    long_help = "Switches to multi-sheet mode and samples up to --max/-m rows (default 10) \
      from *every* worksheet, not just the selected one -- --sheet/--index/--number are \
      ignored in this mode. Field names for every sheet come back in a top-level \"columns\" \
      map instead of a single \"fields\" array; each worksheet's own row count and rows live \
      under \"data\", one block per sheet."
  ) ]
  pub preview: bool,

  #[clap(short = 'l', long, value_parser, default_value_t = false, help = "Output JSON Lines: one compact JSON object per row, no surrounding array (implies --rows)") ]
  pub lines: bool,

  #[clap(short = 'r', long, value_parser, default_value_t = false, help = "Output just the data rows, as a JSON array, with no metadata wrapper") ]
  pub rows: bool,

    // Indexed options
  #[clap(short, long, value_parser, default_value_t = 0, help = "Sheet index, 0-based (0 is the first sheet), same as --number but 0-based") ]
  pub index: u32,
  #[clap(
    long, value_parser,
    help = "Header row index, 0-based (0 is the first row) -- same as --top but 0-based",
    long_help = "Header row index, 0-based (0 is the first row), if the headers aren't \
      on the first row. Equivalent to --top but 0-based -- this is the raw value passed \
      straight through to the underlying library's OptionSet.header_row. Cannot be \
      combined with --top. If neither is given, the header row is guessed automatically \
      from the sheet's own layout (title/notes rows are detected and skipped)."
  ) ]
  pub header_index: Option<u32>,

  #[clap(
    long, value_parser, conflicts_with = "body_start",
    help = "Row index, 0-based, where actual data begins -- same as --body-start but 0-based",
    long_help = "Row index, 0-based, where actual data begins -- same as --body-start but \
      0-based. This is the raw value passed straight through to the underlying library's \
      OptionSet.data_row_index. Cannot be combined with --body-start."
  ) ]
  pub body_index: Option<u32>,

  #[clap(long, value_parser, default_value_t = false, help = "Print processing time, and extra diagnostic detail on error") ]
  pub debug: bool,

  #[clap(long, value_parser, default_value_t = false, help = "Format date-time columns as dates only, with no time component") ]
  pub date_only: bool,

  #[clap(long, value_parser, default_value_t = false, help = "Format date-time columns as times only, with no date component (--date-only wins if both are set)") ]
  pub time_only: bool,

  #[clap(long, value_parser, default_value_t = false, help = "Format date-time columns as hours:minutes only, discarding seconds and any date component (--date-only/--time-only win if set too)") ]
  pub hm_only: bool,

  #[clap(long, value_parser, default_value_t = false, help = "Format date-time columns without milliseconds or a trailing Z, e.g. 2026-07-18T18:07:34 (--date-only/--time-only/--hm-only win if set too)") ]
  pub simple: bool,

  #[clap(long, value_parser, default_value_t = false, help = "Parse decimal commas when converting formatted strings to numbers") ]
  pub euro_number_format: bool,

  // Internal: set when this invocation IS the detached background worker spawned by a
  // user-facing --deferred run, so it knows to do the actual (blocking, from its own
  // point of view) export work instead of spawning yet another worker. Not for direct use.
  #[clap(long, hide = true, default_value_t = false)]
  pub background_worker: bool,

}

pub trait FromArgs {
    fn from_args(args: &Args) -> Result<Self, String> where Self: Sized;
}

impl FromArgs for OptionSet {
    fn from_args(args: &Args) -> Result<Self, String> {

    // --keys entries are `source_key[:new_key][|format[|default]]`. source_key is matched
    // against each column's natural (auto-detected, snake_cased) header key, wherever that
    // column actually is -- so overriding one field out of many doesn't require padding
    // out the columns ahead of it with empty entries. A source_key that doesn't match any
    // column in the file is silently ignored (e.g. a typo, or the wrong sheet/file).
    // The key mapping (source_key[:new_key]) and the datatype override (format[|default])
    // are separated by "|", e.g. "weight_kg:weight|int" or "weight_kg|int" (no rename).
    let mut columns: Vec<Column> = vec![];
    if let Some(k_string) = args.keys.clone() {
      let split_parts = k_string.to_parts(",");
      for ck in split_parts {
        // to_parts (not to_segments) is required throughout here: to_segments collapses
        // empty segments (e.g. "weight_kg||0" would lose the empty format slot entirely),
        // which would silently misalign the default onto the wrong field.
        let pipe_parts = ck.to_parts("|");
        let key_part = pipe_parts.first().cloned().unwrap_or_default();
        let key_sub_parts = key_part.to_parts(":");
        let source_key = key_sub_parts.first()
          .map(|s| s.to_snake_case())
          .filter(|s| !s.is_empty());
        let Some(source_key) = source_key else {
          continue;
        };
        let new_key = key_sub_parts.get(1)
          .map(|s| s.trim())
          .filter(|s| !s.is_empty())
          .map(|s| s.to_snake_case());
        let fmt = pipe_parts.get(1)
          .map(|s| Format::from_str(s).unwrap_or(Format::Auto))
          .unwrap_or(Format::Auto);
        let mut default_val = None;
        if let Some(def_val) = pipe_parts.get(2).filter(|s| !s.is_empty()) {
          default_val = match fmt {
            Format::Integer => {
              let parsed = i128::from_str(def_val).map_err(|_| {
                format!("invalid --keys entry '{}': '{}' is not a valid integer default", ck, def_val)
              })?;
              let num = Number::from_i128(parsed).ok_or_else(|| {
                format!("invalid --keys entry '{}': integer default '{}' is out of range", ck, def_val)
              })?;
              Some(Value::Number(num))
            },
            Format::Boolean => {
              is_truthy_core(def_val, false).map(Value::Bool)
            },
            _ => Some(Value::String(def_val.clone()))
          };
        }

        columns.push(Column::from_source_key_with_format(&source_key, new_key.as_deref(), fmt, default_val, DateTimeMode::Full, false));
      }
    }
    let read_mode = if args.preview {
        ReadMode::PreviewMultiple
    } else if args.deferred {
        ReadMode::Async
    } else {
        ReadMode::Sync
    };
    let mut field_mode = FieldNameMode::AutoA1;
    if let Some(colstyle) = args.colstyle.clone() {
        let (col_key, col_mode) = colstyle.to_head_tail(":");
        if let Some(col_key) = col_key {
            // No ":mode" suffix at all (e.g. just "-c c01") means "apply to every field",
            // same as an explicit "-c c01:all" -- not "leave the default A1-auto style
            // in place", which is what a bare value used to do (silently, since matching
            // both halves of the tuple failed and the whole block was skipped).
            let override_all = col_mode.is_none_or(|m| m.starts_with_ci_alphanum("all"));
            let colkey = col_key.strip_non_alphanum();
            // r1c1 and r1 are also interpreted as c1
            let col_key = if colkey.starts_with_ci("r1") {
                "c1"
            } else {
                &colkey
            };
            field_mode = FieldNameMode::from_key(col_key, override_all);
        }
    }
    let jsonl = args.lines || read_mode.is_async();
    let selected = args.sheet.clone().map(|sheet| sheet.to_segments(","));
    let max = if args.preview {
        Some(DEFAULT_MAX_FOR_PREVIEW)
    } else {
        args.max
    };
    // --number/-n is 1-based ("the 1st sheet"); the core library only knows --index's
    // 0-based position, so this is just --number - 1 wherever it's set. --index and
    // --number are mutually exclusive (see conflicts_with above), so there's no
    // precedence to resolve between them.
    let index = match args.number {
        Some(0) => return Err("invalid --number value: sheets are numbered starting at 1".to_string()),
        Some(n) => n - 1,
        None => args.index,
    };
    // Same 1-based/0-based pairing as --number/--index, but for the header row: --top is
    // 1-based, --header-index is the library's own 0-based OptionSet.header_row directly.
    // Unlike --index (which always has a concrete default), leaving *both* unset here
    // stays None -- that's what tells the library to auto-detect the header row instead
    // of assuming row 0.
    let header_row = match (args.top, args.header_index) {
        (Some(0), _) => return Err("invalid --top value: rows are numbered starting at 1".to_string()),
        (Some(t), _) => Some((t - 1) as usize),
        (None, Some(hi)) => Some(hi as usize),
        (None, None) => None,
    };
    // --body-start (1-based) / --body-index (0-based) map to OptionSet.data_row_index the
    // same way. Unlike --index/--number, both sides are optional here -- None means
    // "unset", matching data_row_index's own Option<usize>.
    let data_row_index = match (args.body_start, args.body_index) {
        (Some(0), _) => return Err("invalid --body-start value: rows are numbered starting at 1".to_string()),
        (Some(b), _) => Some((b - 1) as usize),
        (None, Some(bi)) => Some(bi as usize),
        (None, None) => None,
    };
    // --date-only/--time-only/--hm-only are mutually exclusive row-wide defaults,
    // checked in this order of precedence when more than one is somehow set.
    let datetime_mode = if args.date_only {
        DateTimeMode::DateOnly
    } else if args.time_only {
        DateTimeMode::TimeOnly
    } else if args.hm_only {
        DateTimeMode::HmOnly
    } else if args.simple {
        DateTimeMode::Simple
    } else {
        DateTimeMode::Full
    };
    Ok(OptionSet {
        selected,
        indices: vec![index],
        path: args.path.clone(),
        max,
        header_row,
        data_row_index,
        // spread-cli's own default UX: guess the header/data row from the sheet's layout
        // when neither --top/--header-index nor --body-start/--body-index is given,
        // rather than the library's own plain "assume row 0" default.
        detect_header: true,
        omit_header: args.omit_header,
        rows: crate::RowOptionSet {
            decimal_comma: args.euro_number_format,
            datetime_mode,
            columns,
        },
        jsonl,
        read_mode,
        field_mode
    })
    }
}