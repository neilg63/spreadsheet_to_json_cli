use std::str::FromStr;

use clap::Parser;
use spreadsheet_to_json::heck::ToSnakeCase;
use spreadsheet_to_json::serde_json::{Number, Value};
use spreadsheet_to_json::FieldNameMode;
use spreadsheet_to_json::{is_truthy::*, options::{Column, OptionSet}, Format, ReadMode};
use simple_string_patterns::{SimpleMatch, StripCharacters};
use to_segments::ToSegments;

const DEFAULT_MAX_FOR_PREVIEW: u32 = 10;

/// Command line arguments configuration
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
  
  #[clap(short, long, value_parser) ]
  pub sheet: Option<String>,

  #[clap(short, long, value_parser, default_value_t = 0)]
  pub index: u32,
  
  pub path: Option<String>,

  #[clap(short = 'k', long, value_parser) ]
  pub keys: Option<String>,

  #[clap(short, long, value_parser) ]
  pub max: Option<u32>,

  #[clap(short = 't', long, value_parser, default_value_t = 0) ]
  pub header_row: u8,

  #[clap(long, value_parser, default_value_t = false) ]
  pub omit_header: bool, // no short flag: -o is --output's

  #[clap(short = 'x',long, value_parser, default_value_t = false) ]
  pub exclude_cells: bool, // test validity only and show options

  #[clap(short = 'd',long, value_parser, default_value_t = false) ]
  pub deferred: bool, // test validity only and show options

  #[clap(short = 'p',long, value_parser, default_value_t = false) ]
  pub preview: bool, // show preview only

  #[clap(short = 'l', long, value_parser, default_value_t = false) ]
  pub lines: bool, // debug mode

  #[clap(short = 'r', long, value_parser, default_value_t = false) ]
  pub rows: bool, // debug mode

  #[clap(long, value_parser, default_value_t = false) ]
  pub debug: bool, // debug mode

  #[clap(short = 'c', long, value_parser) ]
  pub colstyle: Option<String>, // debug mode

  #[clap(short = 'j', long, value_parser, default_value_t = false) ]
  pub json: bool, // single structured JSON object covering every query mode, for piping to jq

  #[clap(short = 'o', long, value_parser) ]
  pub output: Option<String>, // export file path for --deferred; overrides the random UUID filename

  #[clap(long, value_parser, default_value_t = false) ]
  pub date_only: bool,

  #[clap(long, value_parser, default_value_t = false)]
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

        columns.push(Column::from_source_key_with_format(&source_key, new_key.as_deref(), fmt, default_val, false, false));
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
    Ok(OptionSet {
        selected,
        indices: vec![args.index],
        path: args.path.clone(),
        max,
        header_row: args.header_row,
        omit_header: args.omit_header,
        rows: crate::RowOptionSet {
            decimal_comma: args.euro_number_format,
            date_only: args.date_only,
            columns,
        },
        jsonl,
        read_mode,
        field_mode
    })
    }
}