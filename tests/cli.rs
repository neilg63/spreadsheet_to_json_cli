//! Integration tests that run the built binary end-to-end against sample
//! spreadsheets under tests/fixtures/, checking both successful parsing and
//! the error-handling paths (missing file, bad format, malformed arguments).

use std::path::PathBuf;
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_spread-cli"))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("failed to run spread-cli binary")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Parses `-r -l` (rows only, JSON-lines) output into one JSON value per row.
fn parse_jsonl_rows(text: &str) -> Vec<serde_json::Value> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("invalid JSON line {:?}: {}", l, e)))
        .collect()
}

// --- error handling ---

#[test]
fn missing_file_reports_clear_error_and_exit_code() {
    let out = run(&["./does-not-exist.xlsx"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stdout(&out).is_empty(), "stdout should be empty on error");
    assert!(
        stderr(&out).contains("file not found"),
        "expected a 'file not found' message, got: {}",
        stderr(&out)
    );
}

#[test]
fn incompatible_format_reports_clear_error() {
    let path = fixture("not_a_spreadsheet.txt");
    let out = run(&[path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("incompatible file format"),
        "expected an 'incompatible file format' message, got: {}",
        stderr(&out)
    );
}

#[test]
fn corrupt_workbook_reports_clear_error() {
    let path = fixture("corrupt.xlsx");
    let out = run(&[path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("could not open the workbook"),
        "expected a 'could not open the workbook' message, got: {}",
        stderr(&out)
    );
}

#[test]
fn directory_path_reports_clear_error() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let out = run(&[dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("directory"), "got: {}", stderr(&out));
}

#[test]
fn no_path_reports_usage_error() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("no spreadsheet file specified"),
        "got: {}",
        stderr(&out)
    );
}

#[test]
fn bad_keys_integer_default_reports_error_without_panicking() {
    let path = fixture("products.xlsx");
    let out = run(&["-k", "qty|i|notanumber", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        !stderr(&out).contains("panicked"),
        "must not panic on bad --keys input, got: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("not a valid integer default"),
        "got: {}",
        stderr(&out)
    );
}

// --- successful parsing ---

#[test]
fn parses_basic_xlsx_rows() {
    let path = fixture("products.xlsx");
    let out = run(&["-r", "-l", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["sku"], "SKU001");
    assert_eq!(rows[0]["name"], "Widget");
    assert_eq!(rows[0]["price"], 9.99);
    assert_eq!(rows[0]["in_stock"], true);
    assert_eq!(rows[2]["sku"], "SKU003");
    assert_eq!(rows[2]["in_stock"], false);
}

#[test]
fn max_flag_limits_row_count() {
    let path = fixture("products.xlsx");
    let out = run(&["-r", "-l", "-m", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["sku"], "SKU001");
}

#[test]
fn keys_flag_renames_columns_by_source_key() {
    let path = fixture("products.xlsx");
    let out = run(&["-r", "-l", "-k", "sku:product_code,name:product_name", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["product_code"], "SKU001");
    assert_eq!(rows[0]["product_name"], "Widget");
    // untouched columns keep their auto-detected header names
    assert_eq!(rows[0]["price"], 9.99);
}

#[test]
fn keys_flag_overrides_one_field_out_of_many_without_padding() {
    // the whole point of source-key matching: override a single field out of many,
    // without needing empty placeholder entries for the columns ahead of it.
    let path = fixture("products.xlsx");
    let out = run(&["-r", "-l", "-k", "qty:quantity|integer", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows[0]["quantity"], 100);
    assert!(rows[0].get("qty").is_none());
    // everything else is untouched
    assert_eq!(rows[0]["sku"], "SKU001");
    assert_eq!(rows[0]["price"], 9.99);
}

#[test]
fn keys_flag_format_only_override_keeps_natural_name() {
    // omitting ":new_key" before the "|" means "keep the natural name, just change the format"
    let path = fixture("products.csv");
    let out = run(&["-r", "-l", "-k", "qty|boolean", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows[0]["qty"], true); // 100 -> truthy
    assert_eq!(rows[2]["qty"], false); // 0 -> falsy
    assert!(rows[0].get("quantity").is_none());
}

#[test]
fn keys_flag_unmatched_source_key_is_silently_ignored() {
    let path = fixture("products.xlsx");
    let out = run(&["-r", "-l", "-k", "nonexistent_field:renamed", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 3);
    // no error, no warning -- just a no-op
    assert!(stderr(&out).is_empty());
    assert_eq!(rows[0]["sku"], "SKU001");
}

#[test]
fn sheet_selection_by_name() {
    let path = fixture("multi_sheet.xlsx");
    let out = run(&["-r", "-l", "-s", "Details", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["note"], "first");
    assert_eq!(rows[1]["note"], "second");
}

#[test]
fn sheet_selection_by_index() {
    let path = fixture("multi_sheet.xlsx");
    // index 1 is the "Details" sheet (0 is "Summary")
    let out = run(&["-r", "-l", "-i", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["note"], "first");
}

#[test]
fn preview_mode_lists_every_sheet() {
    let path = fixture("multi_sheet.xlsx");
    let out = run(&["-p", path.to_str().unwrap()]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Summary"), "got: {}", text);
    assert!(text.contains("Details"), "got: {}", text);
    assert!(text.contains("multimode: true"), "got: {}", text);
}

// --- real-world sample files (from the spreadsheet_to_json library's own test data) ---

#[test]
fn real_world_xlsx_parses_all_data_rows() {
    let path = fixture("sample-data-1.xlsx");
    let out = run(&["-r", "-l", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    // 401 rows in the sheet including the header, so 400 data rows
    assert_eq!(rows.len(), 400);
    assert_eq!(rows[0]["id"], 1.0);
    assert_eq!(rows[0]["first_name"], "Dulce");
    assert_eq!(rows[0]["country"], "United States");
    assert_eq!(rows[399]["id"], 400.0);
    assert_eq!(rows[399]["last_name"], "Lafollette");
}

#[test]
fn real_world_ods_default_sheet_is_first_sheet() {
    let path = fixture("sample-data-2.ods");
    let out = run(&["-r", "-l", "-m", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 1);
    // without --preview, only the first worksheet ("Rsults-2") is read
    assert_eq!(rows[0]["first"], "Sherron");
    assert_eq!(rows[0]["last"], "Ascencio");
}

#[test]
fn real_world_ods_preview_lists_both_sheets_with_correct_totals() {
    let path = fixture("sample-data-2.ods");
    let out = run(&["-p", path.to_str().unwrap()]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("row count: 118"), "got: {}", text);
    assert!(text.contains("Sheet `Rsults-2` (17)"), "got: {}", text);
    assert!(text.contains("Sheet `results 1` (101)"), "got: {}", text);
}

// --- --json mode: one structured object per invocation, for piping to jq ---

fn parse_json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("invalid JSON output: {}\n---\n{}", e, text))
}

#[test]
fn json_mode_single_sheet_has_expected_shape() {
    let path = fixture("products.xlsx");
    let out = run(&["--json", "-m", "2", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));

    assert_eq!(v["extension"], "xlsx");
    assert_eq!(v["sheets"], serde_json::json!(["Products"]));
    assert_eq!(v["column_style"], "A1 auto");
    assert_eq!(v["selected_sheet"], "Products");
    assert_eq!(v["row_count"], 4); // 3 data rows + header
    assert_eq!(v["fields"], serde_json::json!(["sku", "name", "price", "qty", "in_stock"]));
    assert_eq!(v["multimode"], false);
    assert_eq!(v["sheet_indices"], 0);
    assert_eq!(v["file name"], "products.xlsx");
    assert_eq!(v["max_rows"], 2);
    assert_eq!(v["mode"], "JSON");
    assert_eq!(v["headers"], "capture");
    assert_eq!(v["header_row"], 0);
    assert_eq!(v["decimal_separator"], ".");
    assert_eq!(v["date_mode"], "date/time");

    let data = v["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 2); // capped by -m 2
    assert_eq!(data[0]["sku"], "SKU001");
    assert_eq!(data[0]["price"], 9.99);
    assert_eq!(data[0]["in_stock"], true);
}

#[test]
fn json_mode_omits_sheet_fields_for_csv() {
    let path = fixture("products.csv");
    let out = run(&["--json", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));

    assert_eq!(v["extension"], "csv");
    let obj = v.as_object().unwrap();
    for key in ["sheets", "column_style", "selected_sheet", "sheet_indices"] {
        assert!(!obj.contains_key(key), "'{}' should be absent for CSV, got: {}", key, v);
    }

    let data = v["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 3);
    assert_eq!(data[0]["sku"], "SKU101");
    assert_eq!(data[0]["in_stock"], true);
}

#[test]
fn json_mode_preview_returns_data_per_sheet() {
    let path = fixture("multi_sheet.xlsx");
    let out = run(&["--json", "-p", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));

    assert_eq!(v["multimode"], true);
    assert_eq!(v["sheets"], serde_json::json!(["Summary", "Details"]));
    // no single selected sheet or index when every sheet is being read
    let obj = v.as_object().unwrap();
    assert!(!obj.contains_key("selected_sheet"));
    assert!(!obj.contains_key("sheet_indices"));

    let data = v["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["sheet"], "Summary");
    assert_eq!(data[0]["rows"][0]["region"], "North");
    assert_eq!(data[1]["sheet"], "Details");
    assert_eq!(data[1]["rows"][0]["note"], "first");
}

#[test]
fn json_mode_reports_errors_as_json_on_stderr() {
    let out = run(&["--json", "./does-not-exist.xlsx"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stdout(&out).is_empty());
    let v = parse_json(&stderr(&out));
    assert!(
        v["error"].as_str().unwrap_or("").contains("file not found"),
        "got: {}",
        v
    );
}
