mod args;

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
use std::time::Instant;
use tokio::fs::File as TokioFile;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Error>{
  let args = Args::parse();
  let opts = OptionSet::from_args(&args);

  let start = Instant::now(); // Start timer here

  let mut output_lines = false;
  let mut lines: Option<String> = None;
  let result = if opts.is_async() {
    match start_uuid_file().await {
      Ok((mut pb, file_ref)) => {
          // Use a closure that returns a Future (the async block)
          let callback = move |row: IndexMap<String, Value>| {
              let line = json!(row).to_string();
              async move {
                  append_line_to_file(&mut pb, &line).await
              }
          };

          // Use Box<dyn FnOnce(...) -> Future<Output = ...> + Send>
          let boxed_callback: Box<dyn Fn(IndexMap<String, Value>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), GenericError>> + Send>> + Send + Sync> = Box::new(move |row| Box::pin(callback(row)));

          render_spreadsheet_core(&opts, Some(boxed_callback), Some(&file_ref)).await
      }
      Err(msg) => Err(GenericError("uuid_error")),
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
  let duration = start.elapsed(); // Stop timer here
  println!("Total processing time: {:?}", duration);
  Ok(())
}


/// Create a new file with a random UUID and return a result with PathBuf + UUID String
async fn start_uuid_file() -> Result<(TokioFile, String), GenericError> {
  let file_directory = dotenv::var("EXPORT_FILE_DIRECTORY").unwrap_or_else(|_| "./".to_string());
  let mut file_path = PathBuf::from(file_directory);

  std::fs::create_dir_all(&file_path)?;

  let uuid = Uuid::new_v4();
  let filename = format!("{}.jsonl", uuid);
  file_path.push(&filename);

 /*  if let Ok(mut file) = TokioFile::create(&dir_path).await {
    file.write_all(b"").;
  } */
  let file = TokioFile::create(file_path).await?; // Create the file asynchronously
  Ok((file, filename))
}

/// Called in a closure
async fn append_line_to_file(file: &mut TokioFile, line: &str) -> Result<(), GenericError> {
  file.write_all(format!("{}\n", line).as_bytes()).await?;
  Ok(())
}
