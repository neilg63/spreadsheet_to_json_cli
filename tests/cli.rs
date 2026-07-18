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
fn no_path_shows_help() {
    // Bare invocation, or flags without a target file, isn't an error -- there's nothing
    // useful to run, so show help instead of a terse usage error.
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("Usage: spread-cli"), "got: {}", stdout(&out));

    // same for flags with no path
    let out = run(&["-p", "-j"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("Usage: spread-cli"), "got: {}", stdout(&out));
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
    let out = run(&["-l", path.to_str().unwrap()]);
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
fn parses_xlsm_same_as_xlsx() {
    // Regression: .xlsm (macro-enabled) files were rejected by our own extension check
    // before ever reaching calamine, even though calamine reads .xlsm through the exact
    // same Xlsx reader as .xlsx and has always supported it.
    let path = fixture("products.xlsm");
    let out = run(&["-l", path.to_str().unwrap()]);
    assert!(out.status.success(), "got: {}", stderr(&out));
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["sku"], "SKU001");
}

#[test]
fn max_flag_limits_row_count() {
    let path = fixture("products.xlsx");
    let out = run(&["-l", "-m", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["sku"], "SKU001");
}

#[test]
fn keys_flag_renames_columns_by_source_key() {
    let path = fixture("products.xlsx");
    let out = run(&["-l", "-k", "sku:product_code,name:product_name", path.to_str().unwrap()]);
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
    let out = run(&["-l", "-k", "qty:quantity|integer", path.to_str().unwrap()]);
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
    let out = run(&["-l", "-k", "qty|boolean", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows[0]["qty"], true); // 100 -> truthy
    assert_eq!(rows[2]["qty"], false); // 0 -> falsy
    assert!(rows[0].get("quantity").is_none());
}

#[test]
fn keys_flag_unmatched_source_key_is_silently_ignored() {
    let path = fixture("products.xlsx");
    let out = run(&["-l", "-k", "nonexistent_field:renamed", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 3);
    // no error, no warning -- just a no-op
    assert!(stderr(&out).is_empty());
    assert_eq!(rows[0]["sku"], "SKU001");
}

#[test]
fn keys_flag_compound_entries_mix_rename_and_format_only() {
    // A single --keys value can mix "source_key:new_key|format" entries with plain
    // "source_key:new_key" (rename only, no format) entries in the same comma list.
    let path = fixture("products.xlsx");
    let out = run(&["-l", "-k", "qty:quantity|integer,sku:code", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows[0]["code"], "SKU001");
    assert_eq!(rows[0]["quantity"], 100);
    assert!(rows[0].get("sku").is_none());
    assert!(rows[0].get("qty").is_none());
    // untouched column keeps its natural name
    assert_eq!(rows[0]["price"], 9.99);
}

#[test]
fn colstyle_flag_bare_value_applies_to_every_field() {
    // "-c c01" (no ":all" suffix) should behave the same as "-c c01:all" -- it used to be
    // a silent no-op, since matching required both a key AND an explicit mode after ":".
    let path = fixture("products.xlsx");
    let bare = run(&["--json", "-m", "1", "-c", "c01", path.to_str().unwrap()]);
    assert!(bare.status.success());
    let bare_json = parse_json(&stdout(&bare));

    let explicit = run(&["--json", "-m", "1", "-c", "c01:all", path.to_str().unwrap()]);
    assert!(explicit.status.success());
    let explicit_json = parse_json(&stdout(&explicit));

    assert_eq!(bare_json["fields"], explicit_json["fields"]);
    assert_eq!(bare_json["column_style"], "C01 override");
    assert_eq!(bare_json["fields"], serde_json::json!(["c01", "c02", "c03", "c04", "c05"]));
}

#[test]
fn colstyle_flag_bare_a1_applies_to_every_field() {
    let path = fixture("products.xlsx");
    let out = run(&["--json", "-m", "1", "-c", "a1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert_eq!(v["column_style"], "A1 override");
    assert_eq!(v["fields"], serde_json::json!(["a", "b", "c", "d", "e"]));
}

#[test]
fn colstyle_flag_accepts_r1_and_r1c1_as_aliases_for_c01() {
    // r1/R1C1-style is a common spreadsheet-programming convention for the same
    // zero-padded column numbering as c01/c02/... -- but bare "r" (no digit) is NOT
    // an alias; it isn't a recognised style at all and falls through to the default.
    let path = fixture("products.xlsx");
    let expected = serde_json::json!(["c01", "c02", "c03", "c04", "c05"]);

    for style in ["r1", "R1", "r1c1", "R1C1"] {
        let out = run(&["--json", "-m", "1", "-c", style, path.to_str().unwrap()]);
        assert!(out.status.success());
        let v = parse_json(&stdout(&out));
        assert_eq!(v["column_style"], "C01 override", "style '{}' should map to C01", style);
        assert_eq!(v["fields"], expected, "style '{}' should map to C01", style);
    }

    // bare "r" is not a recognised style -- falls through to the default (no-op)
    let out = run(&["--json", "-m", "1", "-c", "r", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert_eq!(v["column_style"], "A1 auto");
}

#[test]
fn colstyle_padding_width_scales_with_column_count() {
    // Under 100 columns, c01-style padding is 2 digits (products.xlsx has 5 columns).
    let narrow = fixture("products.xlsx");
    let out = run(&["--json", "-m", "1", "-c", "c1", narrow.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert_eq!(v["fields"], serde_json::json!(["c01", "c02", "c03", "c04", "c05"]));

    // 100+ columns bumps the padding to 3 digits: c001..c120, not c01..c120.
    let wide = fixture("wide_columns.csv");
    let out = run(&["--json", "-m", "1", "-c", "c1", wide.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    let fields = v["fields"].as_array().expect("fields should be an array");
    assert_eq!(fields.len(), 120);
    assert_eq!(fields[0], "c001");
    assert_eq!(fields[8], "c009");
    assert_eq!(fields[99], "c100");
    assert_eq!(fields[119], "c120");
}

#[test]
fn sheet_selection_by_name() {
    let path = fixture("multi_sheet.xlsx");
    let out = run(&["-l", "-s", "Details", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["note"], "first");
    assert_eq!(rows[1]["note"], "second");
}

#[test]
fn sheet_selection_by_name_is_fuzzy() {
    // Case-insensitive, ignoring spaces/punctuation -- see the library's own
    // match_sheet_name_and_index tests for the underlying matching logic; this confirms
    // it end-to-end through the actual --sheet flag.
    let path = fixture("multi_sheet.xlsx");
    // case, surrounding whitespace, and surrounding punctuation are all ignored
    for variant in ["DETAILS", "details", "  Details  ", "[Details]", "Details!"] {
        let out = run(&["-l", "-s", variant, path.to_str().unwrap()]);
        assert!(out.status.success(), "variant '{}' should succeed", variant);
        let rows = parse_jsonl_rows(&stdout(&out));
        assert_eq!(rows.len(), 2, "variant '{}' got: {:?}", variant, rows);
        assert_eq!(rows[0]["note"], "first", "variant '{}'", variant);
    }

    // punctuation *within* the name introduces a new word boundary, so it's a different
    // snake_case string, not a fuzzy match -- "de-tails" is not the same as "Details"
    let out = run(&["-l", "-s", "de-tails", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows[0]["region"], "North", "unmatched --sheet falls back to the first sheet");
}

#[test]
fn sheet_selection_by_index() {
    let path = fixture("multi_sheet.xlsx");
    // index 1 is the "Details" sheet (0 is "Summary")
    let out = run(&["-l", "-i", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["note"], "first");
}

#[test]
fn sheet_selection_by_number_is_index_plus_one() {
    // --number/-n is 1-based, --index/-i is 0-based -- -n 2 and -i 1 should be
    // identical (both select the "Details" sheet).
    let path = fixture("multi_sheet.xlsx");
    let out_number = run(&["-l", "-n", "2", path.to_str().unwrap()]);
    let out_index = run(&["-l", "-i", "1", path.to_str().unwrap()]);
    assert!(out_number.status.success());
    assert!(out_index.status.success());
    assert_eq!(stdout(&out_number), stdout(&out_index));

    // -n 1 selects the first sheet, same as the default (no -n/-i at all)
    let out_number_1 = run(&["-l", "-n", "1", path.to_str().unwrap()]);
    let out_default = run(&["-l", path.to_str().unwrap()]);
    assert!(out_number_1.status.success());
    assert_eq!(stdout(&out_number_1), stdout(&out_default));
}

#[test]
fn number_flag_rejects_zero_and_conflicts_with_index() {
    let path = fixture("multi_sheet.xlsx");

    // sheets are numbered starting at 1 -- 0 is invalid, not "the first sheet"
    let out = run(&["-n", "0", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("sheets are numbered starting at 1"), "got: {}", stderr(&out));

    // --number and --index are two ways of saying the same thing -- passing both is
    // ambiguous, so clap rejects it outright rather than silently picking one
    let out = run(&["-n", "1", "-i", "0", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("cannot be used with"), "got: {}", stderr(&out));
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

#[test]
fn preview_with_rows_only_does_not_drop_any_sheet() {
    // Regression: -p -r (and -p -r --json) used to silently return only the *first*
    // sheet's rows -- data_set.rows()/to_vec() only ever look at the first sheet, which
    // is correct for single-sheet results but silently dropped every other sheet in a
    // --preview (multimode) result.
    let path = fixture("multi_sheet.xlsx");

    let out = run(&["-p", "-r", "--json", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    let blocks = v.as_array().expect("top-level value should be an array of per-sheet blocks");
    assert_eq!(blocks.len(), 2, "both sheets should be present, got: {}", v);
    assert_eq!(blocks[0]["sheet"], "summary");
    assert_eq!(blocks[0]["rows"][0]["region"], "North");
    assert_eq!(blocks[1]["sheet"], "details");
    assert_eq!(blocks[1]["rows"][0]["note"], "first");

    // same fix applies without --json (plain -p -r)
    let out = run(&["-p", "-r", path.to_str().unwrap()]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("\"sheet\":\"summary\""), "got: {}", text);
    assert!(text.contains("\"sheet\":\"details\""), "got: {}", text);
}

// --- real-world sample files (from the spreadsheet_to_json library's own test data) ---

#[test]
fn real_world_xlsx_parses_all_data_rows() {
    let path = fixture("sample-data-1.xlsx");
    let out = run(&["-l", path.to_str().unwrap()]);
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
fn keys_flag_casts_native_datetime_column_to_date_only() {
    // Regression: a per-column Format::Date override used to be silently ignored for
    // real (non-string) datetime cells -- only the row-wide --date-only flag was ever
    // consulted -- so casting a single datetime column to date-only had no effect at all.
    let path = fixture("sample-data-1.xlsx");
    let out = run(&["-l", "-k", "start_time|date", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    // exactly 400 data rows -- no leaked header row (see next test) and no dropped rows
    assert_eq!(rows.len(), 400);
    assert_eq!(rows[0]["start_time"], "2023-06-15");
    // other columns are untouched
    assert_eq!(rows[0]["first_name"], "Dulce");
}

#[test]
fn keys_flag_with_format_override_does_not_leak_header_row() {
    // Regression: applying any non-Auto Format via --keys used to corrupt the header-row
    // de-duplication check on xlsx/ods files, because it compared the header row's own
    // text *after* running it through that format (e.g. a date parse turns "start_time"
    // into null), which no longer matched the literal header text -- so the header row
    // was wrongly kept as a bogus extra data row.
    let path = fixture("sample-data-1.xlsx");
    let out = run(&["-l", "-k", "start_time|date", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 400);
    // the bogus leaked row looked like {"id": "id", "first_name": "First name", ...}
    assert_ne!(rows[0]["id"], "id");
    assert_eq!(rows[0]["id"], 1.0);
}

#[test]
fn body_start_skips_gap_between_header_row_and_real_data() {
    // header_gap.xlsx: row 0 title, row 1 notes, row 2 header ("sku", "qty"), row 3
    // blank, rows 4-5 real data.
    let path = fixture("header_gap.xlsx");

    // baseline: -t alone (no --body-start) captures the blank gap row as data
    let out = run(&["-l", "-t", "3", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 3, "gap row plus SKU001/SKU002");
    assert_eq!(rows[0]["sku"], serde_json::Value::Null);

    // --body-start skips the gap row entirely
    let out = run(&["-l", "-t", "3", "-b", "5", path.to_str().unwrap()]);
    assert!(out.status.success());
    let rows = parse_jsonl_rows(&stdout(&out));
    assert_eq!(rows.len(), 2, "just SKU001 and SKU002");
    assert_eq!(rows[0]["sku"], "SKU001");
    assert_eq!(rows[0]["qty"], 10.0);
    assert_eq!(rows[1]["sku"], "SKU002");
}

#[test]
fn header_index_and_body_index_are_zero_based_equivalents() {
    // --header-index/--body-index are the 0-based direct-passthrough forms of
    // --top/--body-start (1-based) -- -t 3 -b 5 and --header-index 2 --body-index 4
    // should produce identical output.
    let path = fixture("header_gap.xlsx");
    let out_one_based = run(&["-l", "-t", "3", "-b", "5", path.to_str().unwrap()]);
    let out_zero_based = run(&["-l", "--header-index", "2", "--body-index", "4", path.to_str().unwrap()]);
    assert!(out_one_based.status.success());
    assert!(out_zero_based.status.success());
    assert_eq!(stdout(&out_one_based), stdout(&out_zero_based));
}

#[test]
fn top_and_body_start_reject_zero_and_conflict_with_index_variants() {
    let path = fixture("header_gap.xlsx");

    // rows are numbered starting at 1 -- 0 is invalid
    let out = run(&["-t", "0", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("rows are numbered starting at 1"), "got: {}", stderr(&out));

    let out = run(&["-b", "0", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("rows are numbered starting at 1"), "got: {}", stderr(&out));

    // --top/--header-index and --body-start/--body-index are each two ways of saying the
    // same thing -- passing both sides of a pair is rejected rather than silently picking one
    let out = run(&["-t", "1", "--header-index", "0", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("cannot be used with"), "got: {}", stderr(&out));

    let out = run(&["-b", "1", "--body-index", "0", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("cannot be used with"), "got: {}", stderr(&out));
}

#[test]
fn keys_flag_casts_genuine_excel_date_cell_to_date_only() {
    // A cell Excel itself formats as a plain "Date" (custom number format "yyyy-mm-dd",
    // no time shown) is still stored internally as the same datetime serial value as any
    // full datetime cell, with an all-zero time component -- Excel/calamine don't expose
    // a way to tell "this cell's format is date-only" apart from "this cell just happens
    // to be exactly midnight", so by default it renders with the full ISO datetime
    // (including the meaningless T00:00:00.000Z). --keys "field|date" is the fix.
    let path = fixture("date_only.xlsx");
    let default_out = run(&["-l", path.to_str().unwrap()]);
    assert!(default_out.status.success());
    let default_rows = parse_jsonl_rows(&stdout(&default_out));
    assert_eq!(default_rows[0]["occurred_on"], "2023-09-08T00:00:00.000Z");

    let cast_out = run(&["-l", "-k", "occurred_on|date", path.to_str().unwrap()]);
    assert!(cast_out.status.success());
    let cast_rows = parse_jsonl_rows(&stdout(&cast_out));
    assert_eq!(cast_rows[0]["occurred_on"], "2023-09-08");
}

#[test]
fn real_world_ods_default_sheet_is_first_sheet() {
    let path = fixture("sample-data-2.ods");
    let out = run(&["-l", "-m", "1", path.to_str().unwrap()]);
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
    assert_eq!(v["sheets"], serde_json::json!(["products"]));
    assert_eq!(v["column_style"], "A1 auto");
    assert_eq!(v["selected_sheet"], "products");
    assert_eq!(v["row_count"], 4); // 3 data rows + header
    assert_eq!(v["fields"], serde_json::json!(["sku", "name", "price", "qty", "in_stock"]));
    assert_eq!(v["multimode"], false);
    assert_eq!(v["sheet_indices"], 0);
    assert_eq!(v["file name"], "products.xlsx");
    assert_eq!(v["max_rows"], 2);
    assert_eq!(v["mode"], "JSON");
    assert_eq!(v["headers"], "capture");
    // null (not 0) by default -- the header row is auto-detected, not assumed to be row 0
    assert_eq!(v["header_row"], serde_json::Value::Null);
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
    assert_eq!(v["sheets"], serde_json::json!(["summary", "details"]));
    // no single selected sheet or index when every sheet is being read
    let obj = v.as_object().unwrap();
    assert!(!obj.contains_key("selected_sheet"));
    assert!(!obj.contains_key("sheet_indices"));

    let data = v["data"].as_array().expect("data should be an array");
    assert_eq!(data.len(), 2);
    assert_eq!(data[0]["sheet"], "summary");
    assert_eq!(data[0]["rows"][0]["region"], "North");
    assert_eq!(data[1]["sheet"], "details");
    assert_eq!(data[1]["rows"][0]["note"], "first");
}

#[test]
fn json_mode_preview_has_top_level_columns_map_not_per_sheet_fields() {
    // Field names for a multi-sheet result live once each in the top-level "columns"
    // map ({sheet_key: [field_names]}), not repeated as "fields" inside every "data"
    // sheet block -- keeps a single source of truth instead of two copies of the same
    // names living in different places.
    let path = fixture("multi_sheet.xlsx");
    let out = run(&["--json", "-p", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert_eq!(v["columns"], serde_json::json!({
        "summary": ["region", "total"],
        "details": ["id", "note"]
    }));

    let data = v["data"].as_array().expect("data should be an array");
    for block in data {
        assert!(block.get("fields").is_none(), "got: {}", v);
    }
}

#[test]
fn json_mode_single_sheet_has_no_columns_map() {
    let path = fixture("products.xlsx");
    let out = run(&["--json", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert!(v.as_object().unwrap().get("columns").is_none(), "got: {}", v);
    assert_eq!(v["fields"], serde_json::json!(["sku", "name", "price", "qty", "in_stock"]));
}

#[test]
fn exclude_cells_flag_drops_row_values_but_keeps_columns_in_json_mode() {
    // Regression: --exclude-cells only ever affected the plain-text output path --
    // combined with --json it was silently ignored and full row data was still printed.
    let path = fixture("multi_sheet.xlsx");

    // multi-sheet: top-level "columns" survives cells being excluded; "data" is replaced
    // entirely by a {sheet_key: row_count} "row_counts" map -- no rows, no fields (both
    // already covered elsewhere), just the one number per sheet that "data" would
    // otherwise carry inside a needless {sheet, row_count} wrapper.
    let out = run(&["--json", "-p", "-x", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert_eq!(v["columns"], serde_json::json!({
        "summary": ["region", "total"],
        "details": ["id", "note"]
    }));
    assert_eq!(v["row_counts"], serde_json::json!({
        "summary": 3,
        "details": 3
    }));
    assert!(v.as_object().unwrap().get("data").is_none(), "got: {}", v);

    // single-sheet: "data" would always be an empty array here, so it's omitted entirely
    let path = fixture("products.xlsx");
    let out = run(&["--json", "-x", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert!(v.as_object().unwrap().get("data").is_none(), "got: {}", v);
    assert_eq!(v["fields"], serde_json::json!(["sku", "name", "price", "qty", "in_stock"]));

    // plain text mode (no --json) is unaffected by this fix -- still the pre-existing
    // options dump, not a data-derived overview
    let out = run(&["-x", path.to_str().unwrap()]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("column style:"), "got: {}", stdout(&out));
}

#[test]
fn json_mode_combined_with_rows_prints_only_rows_pretty_printed() {
    // --json must not override -r's "rows only" output mode -- it should only change
    // how those rows get formatted (indented, multi-line) instead of switching to the
    // full metadata-wrapped object.
    let path = fixture("products.xlsx");
    let out = run(&["-r", "--json", path.to_str().unwrap()]);
    assert!(out.status.success());
    let text = stdout(&out);
    let v = parse_json(&text);

    // just the rows array, no metadata wrapper
    let rows = v.as_array().expect("top-level value should be an array of rows");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["sku"], "SKU001");
    assert!(v.get("extension").is_none(), "should not include metadata fields, got: {}", v);
    assert!(v.get("data").is_none(), "should not be wrapped in a 'data' field, got: {}", v);

    // and it should actually be indented/multi-line, unlike plain -r
    assert!(text.contains("\n  "), "expected indented multi-line JSON, got: {}", text);
}

#[test]
fn debug_flag_does_not_corrupt_json_output() {
    // Regression: --debug used to always print "Total processing time: ..." as a plain
    // text line on stdout, which broke any JSON output it was combined with -- a jq
    // consumer would choke on the trailing non-JSON line. --debug must never write to
    // stdout when the output is JSON.
    let path = fixture("products.xlsx");

    // full --json object: stdout must still parse as one JSON value, with timing
    // embedded as a real, queryable field rather than appended as text.
    let out = run(&["--debug", "--json", "-m", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert!(v["processing_time_ms"].is_number(), "got: {}", v);
    assert!(stderr(&out).is_empty(), "timing should not also go to stderr here, got: {}", stderr(&out));

    // -r --json: stdout must still parse as a bare array; timing goes to stderr instead.
    let out = run(&["--debug", "-r", "--json", "-m", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert!(v.is_array(), "got: {}", v);
    assert!(stderr(&out).contains("Total processing time"), "got: {}", stderr(&out));

    // -l (JSONL): every stdout line must still parse as JSON; timing goes to stderr.
    let out = run(&["--debug", "-l", "-m", "1", path.to_str().unwrap()]);
    assert!(out.status.success());
    for line in stdout(&out).lines() {
        parse_json(line);
    }
    assert!(stderr(&out).contains("Total processing time"), "got: {}", stderr(&out));
}

/// --deferred now hands off to a detached background worker and returns immediately (see
/// launch_background_export in main.rs), so the export file may still be empty/partial
/// right when the command returns. Polls briefly for the expected content to land.
fn wait_for_file_lines(path: &std::path::Path, expected_lines: usize) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            if content.lines().count() >= expected_lines {
                return content;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {} to have {} lines",
            path.display(),
            expected_lines
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn deferred_flag_returns_immediately_and_streams_in_the_background() {
    let path = fixture("products.csv");
    let export_path = std::env::temp_dir().join("spread_cli_test_deferred_custom.jsonl");
    let log_path = std::env::temp_dir().join("spread_cli_test_deferred_custom.jsonl.log");
    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&log_path);

    let out = run(&[
        "-r", "-l", "-d",
        "--output", export_path.to_str().unwrap(),
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success());

    // no row data on stdout in deferred mode -- just a message pointing at the file,
    // returned by the foreground process before the background worker has necessarily
    // finished (or even started) writing.
    assert!(
        stdout(&out).contains(&format!("exporting to {}", export_path.display())),
        "got: {}",
        stdout(&out)
    );

    let written = wait_for_file_lines(&export_path, 3);
    let rows: Vec<serde_json::Value> = written.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0]["sku"], "SKU101");

    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&log_path);
}

#[test]
fn deferred_flag_creates_parent_directories_for_custom_file_path() {
    let path = fixture("products.csv");
    let export_dir = std::env::temp_dir().join("spread_cli_test_nested_export_dir");
    let export_path = export_dir.join("sub").join("out.jsonl");
    let _ = std::fs::remove_dir_all(&export_dir);

    let out = run(&["-r", "-l", "-d", "--output", export_path.to_str().unwrap(), path.to_str().unwrap()]);
    assert!(out.status.success());
    // the export file is created synchronously in the foreground before the background
    // worker is even spawned, so this is guaranteed to exist immediately -- no need to wait.
    assert!(export_path.exists(), "expected {} to have been created", export_path.display());

    let _ = std::fs::remove_dir_all(&export_dir);
}

#[test]
fn deferred_flag_with_json_reports_backgrounded_export_as_valid_json() {
    let path = fixture("products.csv");
    let export_path = std::env::temp_dir().join("spread_cli_test_deferred_json.jsonl");
    let log_path = std::env::temp_dir().join("spread_cli_test_deferred_json.jsonl.log");
    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&log_path);

    // full --json object: reports the background hand-off itself, not the export's
    // eventual row data (which isn't necessarily written yet).
    let out = run(&["--json", "-d", "--output", export_path.to_str().unwrap(), path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert_eq!(v["output_reference"], export_path.to_str().unwrap());
    assert_eq!(v["log_file"], format!("{}.log", export_path.display()));
    assert_eq!(v["background"], true);

    wait_for_file_lines(&export_path, 3);
    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&log_path);

    // -rj (bundled -r --json) + deferred: stdout must still be a single valid JSON value
    let out = run(&["-rj", "-d", "--output", export_path.to_str().unwrap(), path.to_str().unwrap()]);
    assert!(out.status.success());
    let v = parse_json(&stdout(&out));
    assert_eq!(v["output_reference"], export_path.to_str().unwrap());

    wait_for_file_lines(&export_path, 3);
    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&log_path);
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
