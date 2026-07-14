mod args;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use args::*;
use spreadsheet_to_json::error::GenericError;
use spreadsheet_to_json::indexmap::IndexMap;
use spreadsheet_to_json::serde_json::{Value, to_string_pretty};
use spreadsheet_to_json::tokio::time::Instant;
use std::io::Write;
use uuid::Uuid;
use spreadsheet_to_json::{tokio, serde_json::json};
use spreadsheet_to_json::{process_spreadsheet_async, process_spreadsheet_immediate, OptionSet, RowOptionSet, PathData, ResultSet};
use std::fs::OpenOptions;

type RowCallback = Box<dyn Fn(IndexMap<String, Value>) -> Result<(), GenericError> + Send + Sync>;

#[tokio::main]
async fn main() -> ExitCode {
  let args = Args::parse();
  let debug_mode = args.debug;

  let opts = match OptionSet::from_args(&args) {
    Ok(opts) => opts,
    Err(msg) => {
      print_error(args.json, &msg);
      return ExitCode::from(2);
    }
  };

  if let Err(msg) = validate_path(args.path.as_deref()) {
    print_error(args.json, &msg);
    return ExitCode::from(2);
  }

  let start = if debug_mode {
    Some(Instant::now()) // Start timer here
  } else {
    None
  };

  let mut lines: Option<String> = None;
  let result = if opts.is_async() {
    match start_uuid_file() {
        Ok((pb, file_ref)) => {
            let callback: RowCallback = Box::new(move |row: IndexMap<String, Value>| {
                append_line_to_file(&pb, &json!(row).to_string())
            });
            process_spreadsheet_async(&opts, callback, Some(&file_ref)).await
        },
        Err(msg) => Err(msg)
    }
  } else {
    process_spreadsheet_immediate(&opts).await
  };
  let data_set = match result {
    Err(msg) => {
      print_error(args.json, &describe_error(&msg));
      if debug_mode {
        eprintln!("details: {}", msg);
        for line in opts.to_lines() {
          eprintln!("  {}", line);
        }
      }
      return ExitCode::FAILURE;
    },
    Ok(data_set) => data_set
  };

  // --json only changes how JSON gets formatted (indented, multi-line) -- it does not
  // pick which content is printed. -r/-l/--exclude-cells (or none of them) still decide
  // that, exactly as without --json.
  let rows_only = (args.lines && !args.preview) || args.rows;
  if rows_only {
      if args.lines {
        // JSONL is inherently one compact object per line; --json doesn't apply here.
        lines = Some(data_set.rows().join("\n"));
      } else if args.json {
        lines = Some(to_string_pretty(&data_set.to_vec()).unwrap());
      } else {
        lines = Some(build_indented_json_rows(&data_set.rows()));
      }
  }
  if rows_only {
    if let Some(lines_string) = lines {
      println!("{}", lines_string);
    }
  } else if args.json {
    println!("{}", to_string_pretty(&build_json_result(&data_set, &opts)).unwrap());
  } else {
    let result_lines = if args.exclude_cells {
        opts.to_lines()
    } else {
        data_set.to_output_lines(args.lines)
    };
    for line in result_lines {
      println!("{}", line);
    }
  }
  if debug_mode {
    if let Some(start_timer) = start {
      let duration = start_timer.elapsed(); // Stop timer here
      println!("Total processing time: {:?}", duration);
    }
  }
  ExitCode::SUCCESS
}

/// Catch the common, easy-to-explain failure cases (missing file, wrong extension)
/// before touching the filesystem any further, so no temp file is created and
/// the user gets a plain-English reason instead of an internal error code.
fn validate_path(path_opt: Option<&str>) -> Result<(), String> {
  let Some(path_str) = path_opt else {
    return Err("no spreadsheet file specified. Usage: spreadsheet-to-json-cli [OPTIONS] <PATH>".to_string());
  };
  let path = Path::new(path_str);
  if !path.exists() {
    return Err(format!("file not found: {}", path.display()));
  }
  if path.is_dir() {
    return Err(format!("expected a file but found a directory: {}", path.display()));
  }
  let path_data = PathData::new(path);
  if !path_data.is_valid() {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("(none)");
    return Err(format!(
      "incompatible file format '.{}'. Supported formats: xlsx, xls, xlsb, ods, csv, tsv",
      ext
    ));
  }
  Ok(())
}

/// Prints an error to stderr, as a JSON object when --json is active so
/// scripted callers can rely on a consistent shape either way.
fn print_error(json_mode: bool, msg: &str) {
  if json_mode {
    eprintln!("{}", json!({ "error": msg }));
  } else {
    eprintln!("error: {}", msg);
  }
}

/// Builds the single structured JSON object emitted by --json, covering every
/// query mode (single sheet, multi-sheet preview, CSV/TSV, deferred).
fn build_json_result(result: &ResultSet, opts: &OptionSet) -> Value {
  let is_workbook = result.extension != "csv" && result.extension != "tsv";
  let mut out: IndexMap<String, Value> = IndexMap::new();

  out.insert("extension".to_string(), json!(result.extension));
  if is_workbook {
    out.insert("sheets".to_string(), json!(result.sheets));
    out.insert("column_style".to_string(), json!(opts.field_mode.to_string()));
  }
  if is_workbook {
    if let Some(selected) = &result.selected {
      out.insert("selected_sheet".to_string(), json!(selected.first().cloned().unwrap_or_default()));
    }
  }
  out.insert("row_count".to_string(), json!(result.num_rows));
  out.insert("fields".to_string(), json!(result.keys));
  out.insert("multimode".to_string(), json!(result.multimode()));
  if is_workbook
    && result.selected.is_some() {
      out.insert("sheet_indices".to_string(), json!(opts.indices.first().copied().unwrap_or(0)));
    }
  out.insert("file name".to_string(), json!(result.filename));
  out.insert("max_rows".to_string(), json!(opts.max_rows()));
  out.insert("mode".to_string(), json!(opts.row_mode()));
  out.insert("headers".to_string(), json!(opts.header_mode()));
  out.insert("header_row".to_string(), json!(opts.header_row));
  out.insert("decimal_separator".to_string(), json!(opts.rows.decimal_separator()));
  out.insert("date_mode".to_string(), json!(opts.rows.date_mode()));
  if let Some(out_ref) = &result.out_ref {
    out.insert("output_reference".to_string(), json!(out_ref));
  }

  let data = if result.multimode() {
    let sheets: Vec<Value> = result.data.sheets().iter().map(|sheet| json!({
      "sheet": sheet.name(),
      "row_count": sheet.num_rows,
      "fields": sheet.keys,
      "rows": sheet.rows
    })).collect();
    json!(sheets)
  } else {
    json!(result.to_vec())
  };
  out.insert("data".to_string(), data);

  json!(out)
}

/// Map the library's internal error codes to plain-English messages.
fn describe_error(err: &GenericError) -> String {
  match err.0 {
    "file_unavailable" => "file not found.".to_string(),
    "unsupported_format" => "incompatible file format. Supported formats: xlsx, xls, xlsb, ods, csv, tsv.".to_string(),
    "no_filepath_specified" => "no spreadsheet file specified.".to_string(),
    "workbook_with_no_sheets" => "the workbook has no readable worksheets.".to_string(),
    "cannot_open_workbook" => "could not open the workbook; the file may be corrupt or not a valid spreadsheet.".to_string(),
    "unreadable_csv_file" => "could not read the CSV file.".to_string(),
    "unreadable_tsv_file" => "could not read the TSV file.".to_string(),
    "xlsx_error" => "the Excel file appears to be corrupt or invalid.".to_string(),
    "ods_error" => "the OpenDocument file appears to be corrupt or invalid.".to_string(),
    "file_not_found" => "file not found.".to_string(),
    "permission_denied" => "permission denied while accessing the file.".to_string(),
    "io_error" => "an I/O error occurred while reading the file.".to_string(),
    other => format!("an unexpected error occurred ({}).", other),
  }
}


pub fn build_indented_json_rows(rows: &[String]) -> String {
  format!("[\n\t{}\n]", rows.join(",\n\t"))
}

/// Create a new file with a random UUID and return a result with PathBuf + UUID String
pub fn start_uuid_file() -> Result<(PathBuf, String), GenericError> {
  let file_directory = dotenv::var("EXPORT_FILE_DIRECTORY").unwrap_or_else(|_| "./".to_string());
  let mut dir_path = PathBuf::from(file_directory);

  std::fs::create_dir_all(&dir_path)?;

  let uuid = Uuid::new_v4();
  let filename = format!("{}.jsonl", uuid);
  dir_path.push(&filename);

  if let Ok(mut file) = File::create(&dir_path) {
    file.write_all(b"").map_err(|_| GenericError("write_error"))?;
  }
  Ok((dir_path, filename))
}

/// Called in a closure
fn append_line_to_file(file_path: &PathBuf, line: &str) -> Result<(), GenericError> {
  if let Ok(mut file) = OpenOptions::new().append(true)
  .create(true)
  .open(file_path) {
    file.write_all(format!("{}\n", line).as_bytes())?;
    Ok(())
  } else {
    Err(GenericError("file_error"))
  }
}
