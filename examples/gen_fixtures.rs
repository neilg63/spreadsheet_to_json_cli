//! Regenerates the .xlsx fixtures under tests/fixtures/ used by the integration tests.
//! Run with: cargo run --example gen_fixtures

use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};

fn main() -> Result<(), rust_xlsxwriter::XlsxError> {
    gen_products()?;
    gen_xlsm_from_products();
    gen_multi_sheet()?;
    gen_date_only()?;
    gen_wide_columns();
    gen_header_gap()?;
    Ok(())
}

/// Single-sheet workbook with mixed column types: text, decimal, integer, boolean.
fn gen_products() -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet().set_name("Products")?;

    let headers = ["sku", "name", "price", "qty", "in_stock"];
    for (col, header) in headers.iter().enumerate() {
        sheet.write_string(0, col as u16, *header)?;
    }

    let rows: [(&str, &str, f64, u32, bool); 3] = [
        ("SKU001", "Widget", 9.99, 100, true),
        ("SKU002", "Gadget", 19.5, 50, true),
        ("SKU003", "Gizmo", 5.0, 0, false),
    ];
    for (i, (sku, name, price, qty, in_stock)) in rows.iter().enumerate() {
        let row = (i + 1) as u32;
        sheet.write_string(row, 0, *sku)?;
        sheet.write_string(row, 1, *name)?;
        sheet.write_number(row, 2, *price)?;
        sheet.write_number(row, 3, *qty as f64)?;
        sheet.write_boolean(row, 4, *in_stock)?;
    }

    workbook.save("tests/fixtures/products.xlsx")?;
    Ok(())
}

/// A .xlsm (macro-enabled) fixture -- .xlsm is the exact same OOXML container as .xlsx
/// (calamine reads both through the same Xlsx reader), so a real macro-enabled file isn't
/// needed to exercise the format: reusing products.xlsx's bytes under a .xlsm extension
/// is sufficient to confirm the extension itself isn't rejected before calamine gets it.
fn gen_xlsm_from_products() {
    std::fs::copy("tests/fixtures/products.xlsx", "tests/fixtures/products.xlsm")
        .expect("failed to write tests/fixtures/products.xlsm");
}

/// Two-sheet workbook for testing --preview and --sheet selection.
fn gen_multi_sheet() -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();

    let summary = workbook.add_worksheet().set_name("Summary")?;
    summary.write_string(0, 0, "region")?;
    summary.write_string(0, 1, "total")?;
    summary.write_string(1, 0, "North")?;
    summary.write_number(1, 1, 120.0)?;
    summary.write_string(2, 0, "South")?;
    summary.write_number(2, 1, 80.0)?;

    let details = workbook.add_worksheet().set_name("Details")?;
    details.write_string(0, 0, "id")?;
    details.write_string(0, 1, "note")?;
    details.write_number(1, 0, 1.0)?;
    details.write_string(1, 1, "first")?;
    details.write_number(2, 0, 2.0)?;
    details.write_string(2, 1, "second")?;

    workbook.save("tests/fixtures/multi_sheet.xlsx")?;
    Ok(())
}

/// A cell genuinely formatted as a date-only value in Excel (custom number format
/// "yyyy-mm-dd", no time component shown) -- exercises the real-world case where the
/// underlying serial value has an all-zero time, same as any plain Excel "Date" cell.
fn gen_date_only() -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet().set_name("Dates")?;
    let date_fmt = Format::new().set_num_format("yyyy-mm-dd");

    sheet.write_string(0, 0, "event")?;
    sheet.write_string(0, 1, "occurred_on")?;
    sheet.write_string(1, 0, "Launch")?;
    let dt = ExcelDateTime::from_ymd(2023, 9, 8)?;
    sheet.write_datetime_with_format(1, 1, &dt, &date_fmt)?;

    workbook.save("tests/fixtures/date_only.xlsx")?;
    Ok(())
}

/// A CSV with more than 100 columns -- exercises the c01-style padding width scaling
/// up from 2 digits to 3 (c01..c99 under 100 columns, c001..c999 from 100 up to 1,000).
/// Plain CSV rather than xlsx, since a wide binary workbook is unnecessary overhead
/// just to test column-count-driven padding width.
fn gen_wide_columns() {
    let num_cols = 120;
    let header = (1..=num_cols).map(|i| format!("Col {}", i)).collect::<Vec<_>>().join(",");
    let row = (1..=num_cols).map(|i| format!("v{}", i)).collect::<Vec<_>>().join(",");
    std::fs::write("tests/fixtures/wide_columns.csv", format!("{}\n{}\n", header, row))
        .expect("failed to write tests/fixtures/wide_columns.csv");
}

/// A layout common to real-world spreadsheets (e.g. statistics-agency publications): a
/// title row, a notes row, the real header row, then a blank gap row before the actual
/// data -- for testing --header-row/--data-row. Row 0 "Report Title", row 1 "Generated
/// 2026-01-01", row 2 header ("sku", "qty"), row 3 blank, rows 4-5 data.
fn gen_header_gap() -> Result<(), rust_xlsxwriter::XlsxError> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet().set_name("Sheet1")?;
    sheet.write_string(0, 0, "Report Title")?;
    sheet.write_string(1, 0, "Generated 2026-01-01")?;
    sheet.write_string(2, 0, "sku")?;
    sheet.write_string(2, 1, "qty")?;
    sheet.write_string(4, 0, "SKU001")?;
    sheet.write_number(4, 1, 10.0)?;
    sheet.write_string(5, 0, "SKU002")?;
    sheet.write_number(5, 1, 20.0)?;

    workbook.save("tests/fixtures/header_gap.xlsx")?;
    Ok(())
}
