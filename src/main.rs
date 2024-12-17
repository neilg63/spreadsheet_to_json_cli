mod args;

use std::fs::File;
use std::path::PathBuf;

use clap::Parser;
use args::*;
use spreadsheet_to_json::error::GenericError;
use spreadsheet_to_json::indexmap::IndexMap;
use spreadsheet_to_json::serde_json::Value;
use std::io::{Error, Write};
use uuid::Uuid;
use spreadsheet_to_json::{tokio, serde_json::json};
use spreadsheet_to_json::{render_spreadsheet_core, render_spreadsheet_direct, OptionSet, RowOptionSet};
use std::fs::OpenOptions;

#[tokio::main]
async fn main() -> Result<(), Error>{
  let args = Args::parse();
  let opts = OptionSet::from_args(&args);

  let mut output_lines = false;
  let mut lines: Option<String> = None;
  let result = if opts.is_async() {
    match start_uuid_file() {
        Ok((pb, file_ref)) => {
            let callback: Box<dyn Fn(IndexMap<String, Value>) -> Result<(), GenericError> + Send + Sync> = Box::new(move |row: IndexMap<String, Value>| {
                append_line_to_file(&pb, &json!(row).to_string())
            });
            render_spreadsheet_core(&opts, Some(callback), Some(&file_ref)).await
        },
        Err(msg) => Err(GenericError("uuid_error"))
    }
  } else {
    render_spreadsheet_direct(&opts).await
  };
  
  let json_value = match result {
    Err(msg) => json!{ { "error": true, "message": msg.to_string(), "options": opts.to_json() } },
    Ok(data_set) => {
      output_lines = args.jsonl;
      if output_lines {
          lines = Some(data_set.rows().join("\n"));
      }
      if args.exclude_cells {
          json!({
              "options": opts.to_json() 
          })
      } else {
          data_set.to_json()
      }
    }
  };
  if output_lines {
    if let Some(lines_string) = lines {
        println!("{}", lines_string);
    }
  } else {
      println!("{}", json_value);
  }
  Ok(())
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