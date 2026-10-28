//! Reads the first sheet of a CSV, Excel or OpenDocument file into a header row and data rows.

use std::io::Cursor;

use calamine::{Data, Reader, open_workbook_auto_from_rs};

use crate::error::{Result, bad_request};

pub const MAX_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_ROWS: usize = 1000;
const MAX_COLUMNS: usize = 60;
const MAX_CELL: usize = 500;
const MAX_UNPACKED: u64 = 60 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn read(name: &str, bytes: &[u8]) -> Result<Sheet> {
    if bytes.len() > MAX_BYTES {
        return Err(bad_request("The file is larger than 5 MB"));
    }
    let lower = name.to_lowercase();
    let table = if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        spreadsheet(bytes)?
    } else if lower.ends_with(".csv") || lower.ends_with(".txt") || lower.ends_with(".tsv") {
        csv_rows(&text(bytes))?
    } else {
        return Err(bad_request("Upload a CSV or an Excel file (.csv, .xlsx, .xls or .ods)"));
    };
    tidy(table)
}

/// Excel on Windows still saves CSV as Windows-1252, so anything that is not UTF-8 is read as that.
fn text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1252.decode(bytes).0.into_owned(),
    }
}

fn csv_rows(text: &str) -> Result<Vec<Vec<String>>> {
    let first = text.lines().next().unwrap_or_default();
    let delimiter = b",;\t".iter().copied().max_by_key(|d| first.matches(*d as char).count()).unwrap_or(b',');
    let mut reader = csv::ReaderBuilder::new().delimiter(delimiter).has_headers(false).flexible(true).from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|_| bad_request("The CSV file could not be read"))?;
        rows.push(record.iter().map(str::to_string).collect());
        if rows.len() > MAX_ROWS + 1 {
            break;
        }
    }
    Ok(rows)
}

fn spreadsheet(bytes: &[u8]) -> Result<Vec<Vec<String>>> {
    if bytes.starts_with(b"PK") {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| bad_request("The spreadsheet could not be read"))?;
        let mut unpacked = 0u64;
        for i in 0..archive.len() {
            let entry = archive.by_index_raw(i).map_err(|_| bad_request("The spreadsheet could not be read"))?;
            unpacked = unpacked.saturating_add(entry.size());
        }
        if unpacked > MAX_UNPACKED {
            return Err(bad_request("The spreadsheet is too large once unpacked"));
        }
    }
    let mut workbook =
        open_workbook_auto_from_rs(Cursor::new(bytes.to_vec())).map_err(|_| bad_request("The spreadsheet could not be read"))?;
    let range = workbook
        .worksheet_range_at(0)
        .ok_or_else(|| bad_request("The spreadsheet has no sheets"))?
        .map_err(|_| bad_request("The first sheet could not be read"))?;
    Ok(range.rows().take(MAX_ROWS + 2).map(|row| row.iter().map(cell).collect()).collect())
}

fn cell(data: &Data) -> String {
    match data {
        Data::Empty | Data::Error(_) => String::new(),
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{f:.0}"),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(d) => match d.as_datetime() {
            Some(t) if t.time() == chrono::NaiveTime::MIN => t.date().to_string(),
            Some(t) => t.format("%Y-%m-%d %H:%M").to_string(),
            None => d.as_f64().to_string(),
        },
    }
}

fn tidy(mut rows: Vec<Vec<String>>) -> Result<Sheet> {
    for row in &mut rows {
        for value in row.iter_mut() {
            let clean: String = value.chars().filter(|c| !c.is_control() || *c == '\n').collect();
            *value = clean.trim().chars().take(MAX_CELL).collect();
        }
    }
    rows.retain(|r| r.iter().any(|v| !v.is_empty()));
    if rows.is_empty() {
        return Err(bad_request("The file is empty"));
    }
    let header = rows.remove(0);
    let width = header.iter().rposition(|h| !h.is_empty()).map_or(0, |i| i + 1);
    if width == 0 {
        return Err(bad_request("The first row must name the columns"));
    }
    if width > MAX_COLUMNS {
        return Err(bad_request(format!("Use at most {MAX_COLUMNS} columns")));
    }
    if rows.len() > MAX_ROWS {
        return Err(bad_request(format!("Send to at most {MAX_ROWS} rows at a time")));
    }
    let mut columns: Vec<String> = Vec::with_capacity(width);
    for (i, h) in header.into_iter().take(width).enumerate() {
        let mut name = if h.is_empty() { format!("Column {}", i + 1) } else { h.replace('\n', " ") };
        if columns.contains(&name) {
            name = format!("{name} ({})", i + 1);
        }
        columns.push(name);
    }
    let rows = rows
        .into_iter()
        .map(|mut r| {
            r.resize(width, String::new());
            r
        })
        .collect();
    Ok(Sheet { columns, rows })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reads_semicolon_csv_in_windows_1252() {
        let bytes = b"Namn;E-post;Belopp\nG\xf6ran \xc5berg;goran@example.test;1 200\n;;\n";
        let sheet = read("kunder.csv", bytes).unwrap();
        assert_eq!(sheet.columns, ["Namn", "E-post", "Belopp"]);
        assert_eq!(sheet.rows, vec![vec!["Göran Åberg".to_string(), "goran@example.test".into(), "1 200".into()]]);
    }

    #[test]
    fn names_blank_and_repeated_headers() {
        let sheet = read("a.csv", "Email,,Email\nx,y,z\n".as_bytes()).unwrap();
        assert_eq!(sheet.columns, ["Email", "Column 2", "Email (3)"]);
    }

    #[test]
    fn refuses_other_files() {
        assert!(read("a.pdf", b"%PDF-1.7").is_err());
        assert!(read("a.xlsx", b"PK\x03\x04garbage").is_err());
    }
}
