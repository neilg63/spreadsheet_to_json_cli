use std::str::FromStr;

use clap::Parser;
use spreadsheet_to_json::heck::ToSnakeCase;
use spreadsheet_to_json::serde_json::{Number, Value};
use spreadsheet_to_json::FieldNameMode;
use spreadsheet_to_json::{is_truthy::*, options::{Column, OptionSet}, Format, ReadMode};
use spreadsheet_to_json::simple_string_patterns::{SimpleMatch, ToSegments};

/// Command line arguments configuration
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
  
  #[clap(short, long, value_parser) ]
  pub sheet: Option<String>,

  #[clap(short, long, value_parser, default_value_t = 0)]
  pub index: u32,
  
  pub path: Option<String>,

  #[clap(long, value_parser, default_value_t = false)]
  pub euro_number_format: bool,

  #[clap(long, value_parser, default_value_t = false) ]
  pub date_only: bool,

  #[clap(short = 'k', long, value_parser) ]
  pub keys: Option<String>,

  #[clap(short, long, value_parser) ]
  pub max: Option<u32>,

  #[clap(short = 't', long, value_parser, default_value_t = 0) ]
  pub header_row: u8,

  #[clap(short = 'o',long, value_parser, default_value_t = false) ]
  pub omit_header: bool,

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

  #[clap(short, long, value_parser) ]
  pub colstyle: Option<String>, // debug mode

}

pub trait FromArgs {
    fn from_args(args: &Args) -> Self;
}

impl FromArgs for OptionSet {
    fn from_args(args: &Args) -> Self {

    let mut columns: Vec<Column> = vec![];
    if let Some(k_string) = args.keys.clone() {
      let split_parts = k_string.to_parts(",");
      for ck in split_parts {
        let sub_parts = ck.to_segments(":");
        let num_subs = sub_parts.len();
        if num_subs < 2 {
          let key_opt = if ck.len() > 0 {
            let ck_sc = ck.to_snake_case();
            if ck_sc.len() > 0 {
              Some(ck_sc)
            } else {
              None
            }
          } else {
            None
          };
          columns.push(Column::new(key_opt.as_deref()));
        } else {
          let fmt = Format::from_str(sub_parts.get(1).unwrap_or(&"auto".to_string())).unwrap_or(Format::Auto);
          let mut default_val = None;
          if let Some(def_val) = sub_parts.get(2) {
            default_val = match fmt {
              Format::Integer => Some(Value::Number(Number::from_i128(i128::from_str(&def_val).unwrap()).unwrap())),
              Format::Boolean => {
                if let Some(is_true) = is_truthy_core(def_val, false) {
                  Some(Value::Bool(is_true))
                } else {
                  None
                }
              },
              _ => Some(Value::String(def_val.clone()))
            };
          }

          let key_name = sub_parts.get(0).unwrap_or(&ck).to_snake_case();
          columns.push(Column::from_key_ref_with_format(Some(&key_name), fmt, default_val, false, false));
        }
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
        let (col_key, col_mode) = colstyle.to_start_end(":");
        field_mode = FieldNameMode::from_key(&col_key, col_mode.starts_with_ci_alphanum("all"));
    }
    let jsonl = args.lines || read_mode.is_async();
    let selected = if let Some(sheet) = args.sheet.clone() {
        Some(sheet.to_segments(","))
    } else {
        None
    };
    OptionSet {
        selected,
        indices: vec![args.index],
        path: args.path.clone(),
        max: args.max,
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
    }
    }
}