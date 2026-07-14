mod args;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(unix)]
use std::process::{Command, Stdio};

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

  // --deferred: on Unix, hand the actual export off to a detached background process and
  // return immediately, rather than blocking here until it finishes (see
  // launch_background_export for why this is Unix-only). We still validate the path and
  // options synchronously above first, so obvious mistakes (bad file, bad --keys) are
  // still reported directly in the foreground rather than silently failing in the
  // background. args.background_worker is only ever set by that spawn below -- it marks
  // "this invocation IS the worker", so it falls through to do the real work instead of
  // spawning yet another one. On non-Unix (or for the worker itself), this falls through
  // to the same in-process, streamed-but-blocking handling --deferred has always had.
  if opts.is_async() && !args.background_worker {
    if let Some(launch_result) = try_launch_background_export(&args) {
      return match launch_result {
        Ok(export_path) => {
          let log_path = format!("{}.log", export_path);
          if args.json {
            println!("{}", to_string_pretty(&json!({
              "output_reference": export_path,
              "log_file": log_path,
              "background": true
            })).unwrap());
          } else {
            println!("exporting to {} in the background (see {} for progress and errors)", export_path, log_path);
          }
          ExitCode::SUCCESS
        },
        Err(msg) => {
          print_error(args.json, &describe_error(&msg));
          ExitCode::FAILURE
        }
      };
    }
  }

  let start = if debug_mode {
    Some(Instant::now()) // Start timer here
  } else {
    None
  };

  let mut lines: Option<String> = None;
  let result = if opts.is_async() {
    match resolve_export_file(args.output.as_deref()) {
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
  if rows_only && !opts.is_async() {
      if opts.multimode() {
        // data_set.rows()/to_vec() only ever return the *first* sheet -- with multiple
        // sheets (--preview) that would silently drop every sheet after it, so rows-only
        // mode needs the same per-sheet blocks the full --json object already uses.
        let blocks = multimode_sheet_blocks(&data_set);
        if args.lines {
          lines = Some(blocks.iter().map(|b| b.to_string()).collect::<Vec<_>>().join("\n"));
        } else if args.json {
          lines = Some(to_string_pretty(&blocks).unwrap());
        } else {
          let compact: Vec<String> = blocks.iter().map(|b| b.to_string()).collect();
          lines = Some(build_indented_json_rows(&compact));
        }
      } else if args.lines {
        // JSONL is inherently one compact object per line; --json doesn't apply here.
        lines = Some(data_set.rows().join("\n"));
      } else if args.json {
        lines = Some(to_string_pretty(&data_set.to_vec()).unwrap());
      } else {
        lines = Some(build_indented_json_rows(&data_set.rows()));
      }
  }
  if rows_only {
    if opts.is_async() {
      // --deferred streams rows straight to a file rather than capturing them, so
      // there's no row data to show here -- tell the user where it went instead of
      // printing an empty line (or, with --json, an empty array).
      if let Some(out_ref) = data_set.out_ref.clone() {
        if args.json {
          println!("{}", to_string_pretty(&json!({ "output_reference": out_ref })).unwrap());
        } else {
          println!("exporting to {}", out_ref);
        }
      }
    } else if let Some(lines_string) = lines {
      println!("{}", lines_string);
    }
    // Rows-only output is a bare array (or JSONL stream) -- there's no clean place to
    // embed debug metadata inline without breaking that contract, so timing always goes
    // to stderr here, never stdout (keeping stdout safe to pipe into jq/NDJSON tools).
    print_debug_timing(debug_mode, start, true);
  } else if args.json {
    let mut json_result = build_json_result(&data_set, &opts);
    if debug_mode {
      if let Some(start_timer) = start {
        json_result["processing_time_ms"] = json!(start_timer.elapsed().as_secs_f64() * 1000.0);
      }
    }
    println!("{}", to_string_pretty(&json_result).unwrap());
  } else {
    let result_lines = if args.exclude_cells {
        opts.to_lines()
    } else {
        data_set.to_output_lines(args.lines)
    };
    for line in result_lines {
      println!("{}", line);
    }
    print_debug_timing(debug_mode, start, false);
  }
  ExitCode::SUCCESS
}

/// Prints the --debug processing-time line, if timing was started. JSON-producing modes
/// (rows-only, or the plain-text fallback) always send it to stderr so it can never
/// corrupt stdout for a jq/NDJSON consumer -- the full --json object mode embeds the
/// timing as a real field instead (see the `processing_time_ms` insert above) rather
/// than calling this at all.
fn print_debug_timing(debug_mode: bool, start: Option<Instant>, to_stderr: bool) {
  if !debug_mode {
    return;
  }
  let Some(start_timer) = start else {
    return;
  };
  let duration = start_timer.elapsed();
  if to_stderr {
    eprintln!("Total processing time: {:?}", duration);
  } else {
    println!("Total processing time: {:?}", duration);
  }
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

/// One JSON block per sheet: {"sheet", "row_count", "fields", "rows"}. Used both by the
/// full --json object's "data" field and directly by the rows-only (-r/-l) path when
/// --preview is active -- multimode results have no single flat row list to hand back
/// (data_set.rows()/to_vec() only ever return the *first* sheet's rows), so rows-only
/// mode needs this same per-sheet shape rather than silently dropping every other sheet.
fn multimode_sheet_blocks(result: &ResultSet) -> Vec<Value> {
  result.data.sheets().iter().map(|sheet| json!({
    "sheet": sheet.name(),
    "row_count": sheet.num_rows,
    "fields": sheet.keys,
    "rows": sheet.rows
  })).collect()
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
    json!(multimode_sheet_blocks(result))
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
    "write_error" => "could not create the export file. Check the path and permissions.".to_string(),
    "exec_path_error" => "could not determine this program's own executable path, needed to launch the background export worker.".to_string(),
    "spawn_error" => "could not launch the background export worker process.".to_string(),
    other => format!("an unexpected error occurred ({}).", other),
  }
}


pub fn build_indented_json_rows(rows: &[String]) -> String {
  format!("[\n\t{}\n]", rows.join(",\n\t"))
}

/// Resolves the file a --deferred export should be written to, and creates it (empty)
/// ready for streaming writes. If `explicit_path` (--output) is given, it's used exactly
/// as provided; otherwise falls back to a random-UUID .jsonl filename under
/// EXPORT_FILE_DIRECTORY (default "./"), as before. Returns the path used both as a
/// PathBuf (for writing) and as a displayable string (for the "exporting to ..." message
/// and the `output_reference`/`output reference` metadata).
pub fn resolve_export_file(explicit_path: Option<&str>) -> Result<(PathBuf, String), GenericError> {
  let dir_path = if let Some(explicit) = explicit_path {
    PathBuf::from(explicit)
  } else {
    let file_directory = dotenv::var("EXPORT_FILE_DIRECTORY").unwrap_or_else(|_| "./".to_string());
    let mut dir_path = PathBuf::from(file_directory);
    dir_path.push(format!("{}.jsonl", Uuid::new_v4()));
    dir_path
  };

  if let Some(parent) = dir_path.parent() {
    if !parent.as_os_str().is_empty() {
      std::fs::create_dir_all(parent)?;
    }
  }

  if let Ok(mut file) = File::create(&dir_path) {
    file.write_all(b"").map_err(|_| GenericError("write_error"))?;
  } else {
    return Err(GenericError("write_error"));
  }

  let display_path = dir_path.to_string_lossy().to_string();
  Ok((dir_path, display_path))
}

/// Tries to launch a detached background export (see launch_background_export);
/// `None` means the caller should fall back to the ordinary in-process, streamed-but-
/// blocking --deferred handling this crate has always had.
///
/// Unix-only by design, not just by omission: this is primarily a server-side tool
/// (Linux/Mac deployment), true background detachment is the thing actually worth
/// having there for million-row imports, and Windows' equivalent (CREATE_NEW_PROCESS_GROUP
/// | DETACHED_PROCESS) is a meaningfully different, untested code path not worth carrying
/// for a platform this tool isn't really aimed at. Falling back to the existing blocking
/// (but still memory-streamed) behavior on Windows is strictly safe -- it's exactly what
/// --deferred already did everywhere before background detachment existed.
#[cfg(unix)]
fn try_launch_background_export(args: &Args) -> Option<Result<String, GenericError>> {
  Some(launch_background_export(args))
}

#[cfg(not(unix))]
fn try_launch_background_export(_args: &Args) -> Option<Result<String, GenericError>> {
  None
}

/// Spawns a detached copy of this same binary to perform the actual --deferred export,
/// and returns immediately without waiting for it -- the caller can hand control back to
/// the shell right away while the export continues after this process exits.
///
/// The export file is resolved and created *here*, in the foreground process, both so the
/// path can be reported back immediately and so the worker doesn't independently generate
/// a different random UUID filename than the one just announced. The worker is re-invoked
/// with every original argument plus an explicit `--output <resolved path>` (pinning it to
/// that exact file) and the internal `--background-worker` flag.
///
/// The worker's stdout/stderr are redirected to a `<export path>.log` file, since once
/// detached there's no terminal left to report progress or errors to directly -- that log
/// is the only way to find out afterward whether it actually succeeded.
///
/// Detachment via a new process group (stable since Rust 1.64 via
/// `std::os::unix::process::CommandExt::process_group`) keeps the worker from being tied
/// to this process's console/job, so it keeps running after this process exits and won't
/// receive a Ctrl+C sent to this one.
#[cfg(unix)]
fn launch_background_export(args: &Args) -> Result<String, GenericError> {
  use std::os::unix::process::CommandExt;

  let (_, export_path) = resolve_export_file(args.output.as_deref())?;

  let exe = std::env::current_exe().map_err(|_| GenericError("exec_path_error"))?;
  let mut cmd = Command::new(exe);
  // Forward the original invocation's own arguments, but strip out any pre-existing
  // --output/-o (and its value) first -- clap rejects the same non-repeatable argument
  // being passed twice, and we're about to append our own resolved --output below.
  for arg in args_without_output_flag() {
    cmd.arg(arg);
  }
  cmd.arg("--output").arg(&export_path);
  cmd.arg("--background-worker");

  let log_path = format!("{}.log", export_path);
  let log_out = File::create(&log_path).map_err(|_| GenericError("write_error"))?;
  let log_err = log_out.try_clone().map_err(|_| GenericError("write_error"))?;
  cmd.stdin(Stdio::null());
  cmd.stdout(Stdio::from(log_out));
  cmd.stderr(Stdio::from(log_err));
  cmd.process_group(0);

  cmd.spawn().map_err(|_| GenericError("spawn_error"))?;
  Ok(export_path)
}

/// The current invocation's own arguments (skipping argv[0]), with any --output/-o and its
/// value removed. Doesn't attempt to handle -f bundled into a multi-short-flag group
/// (e.g. "-ro value") -- pass --output/-o as its own token.
#[cfg(unix)]
fn args_without_output_flag() -> Vec<String> {
  let mut out = Vec::new();
  let mut skip_next = false;
  for arg in std::env::args().skip(1) {
    if skip_next {
      skip_next = false;
      continue;
    }
    if arg == "--output" || arg == "-o" {
      skip_next = true;
      continue;
    }
    if arg.starts_with("--output=") || arg.starts_with("-o=") {
      continue;
    }
    out.push(arg);
  }
  out
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
