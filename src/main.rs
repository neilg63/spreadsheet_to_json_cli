mod args;

use clap::{error::Error, Parser};
use args::*;
use spreadsheet_to_json::{tokio, serde_json::json};
use spreadsheet_to_json::{render_spreadsheet_direct,OptionSet, RowOptionSet};

#[tokio::main]
async fn main() -> Result<(), Error>{
  let args = Args::parse();
  let opts = OptionSet::from_args(&args);

  let mut output_lines = false;
  let mut lines: Option<String> = None;
  let result = render_spreadsheet_direct(&opts).await;
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
