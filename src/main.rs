mod args;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use args::*;
use spreadsheet_to_json::error::GenericError;
use spreadsheet_to_json::indexmap::IndexMap;
use spreadsheet_to_json::serde_json::Value;
use spreadsheet_to_json::tokio::time::Instant;
use std::io::Write;
use uuid::Uuid;
use spreadsheet_to_json::{tokio, serde_json::json};
use spreadsheet_to_json::{process_spreadsheet_async, process_spreadsheet_immediate, OptionSet, RowOptionSet, PathData};
use std::fs::OpenOptions;

#[tokio::main]
async fn main() -> ExitCode {
  let args = Args::parse();
  let debug_mode = args.debug;

  let opts = match OptionSet::from_args(&args) {
    Ok(opts) => opts,
    Err(msg) => {
      eprintln!("error: {}", msg);
      return ExitCode::from(2);
    }
  };

  if let Err(msg) = validate_path(args.path.as_deref()) {
    eprintln!("error: {}", msg);
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
            let callback: Box<dyn Fn(IndexMap<String, Value>) -> Result<(), GenericError> + Send + Sync> = Box::new(move |row: IndexMap<String, Value>| {
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
      eprintln!("error: {}", describe_error(&msg));
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

  let rows_only = (args.lines && !args.preview) || args.rows;
  if rows_only {
      if args.lines {
        lines = Some(data_set.rows().join("\n"));
      } else {
        lines = Some(build_indented_json_rows(&data_set.rows()));
      }
  }
  let result_lines = if args.exclude_cells {
      opts.to_lines()
  } else {
      data_set.to_output_lines(args.lines)
  };
  if rows_only {
    if let Some(lines_string) = lines {
      println!("{}", lines_string);
    }
  } else {
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
  format!("[\n\t{}\n]", &rows.join(",\n\t"))
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
