//! Regenerates the .xlsx fixtures under tests/fixtures/ used by the integration tests.
//! Run with: cargo run --example gen_fixtures

use rust_xlsxwriter::{ExcelDateTime, Format, Workbook};

fn main() -> Result<(), rust_xlsxwriter::XlsxError> {
    gen_products()?;
    gen_multi_sheet()?;
    gen_date_only()?;
    gen_wide_columns();
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
