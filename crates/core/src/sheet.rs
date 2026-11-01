//! Reads the first sheet of a CSV or Excel file into a header row and data rows.

use std::collections::BTreeMap;
use std::io::Cursor;

use calamine::{Data, Reader, Xlsb, Xlsx};

use crate::error::{Result, bad_request};

pub const MAX_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_ROWS: usize = 1000;
const MAX_COLUMNS: usize = 60;
const MAX_CELL: usize = 500;
const MAX_UNPACKED: u64 = 60 * 1024 * 1024;
const MAX_CELLS: usize = (MAX_ROWS + 2) * (MAX_COLUMNS + 1);
const OTHER_SPREADSHEET: &str = "Save the spreadsheet as .xlsx or .csv, then upload it again";

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
    let table = if bytes.starts_with(b"PK\x03\x04") {
        spreadsheet(bytes)?
    } else if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err(bad_request(OTHER_SPREADSHEET));
    } else if lower.ends_with(".csv") || lower.ends_with(".txt") || lower.ends_with(".tsv") {
        csv_rows(&text(bytes))?
    } else {
        return Err(bad_request("Upload a CSV or an Excel file (.csv or .xlsx)"));
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
    let unreadable = || bad_request("The spreadsheet could not be read");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| unreadable())?;
    let mut unpacked = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index_raw(i).map_err(|_| unreadable())?;
        unpacked = unpacked.saturating_add(entry.size());
    }
    if unpacked > MAX_UNPACKED {
        return Err(bad_request("The spreadsheet is too large once unpacked"));
    }
    let binary = archive.file_names().any(|n| n.is_ok_and(|n| n == "xl/workbook.bin"));
    if !binary && !archive.file_names().any(|n| n.is_ok_and(|n| n == "xl/workbook.xml")) {
        return Err(bad_request(OTHER_SPREADSHEET));
    }
    // Cell by cell: a whole-sheet range allocates rows times columns however few cells there are,
    // and an allocation that fails aborts the process. Older formats build that range when opened,
    // which is why they are refused above.
    let mut cells = Cells::default();
    let cursor = Cursor::new(bytes);
    if binary {
        let mut book: Xlsb<_> = Xlsb::new(cursor).map_err(|_| unreadable())?;
        let first = book.sheet_names().first().cloned().ok_or_else(|| bad_request("The spreadsheet has no sheets"))?;
        let mut reader = book.worksheet_cells_reader(&first).map_err(|_| unreadable())?;
        while let Some(cell) = reader.next_cell().map_err(|_| unreadable())? {
            if !cells.add(cell.get_position(), Data::from(cell.get_value().clone())) {
                break;
            }
        }
    } else {
        let mut book: Xlsx<_> = Xlsx::new(cursor).map_err(|_| unreadable())?;
        let first = book.sheet_names().first().cloned().ok_or_else(|| bad_request("The spreadsheet has no sheets"))?;
        let mut reader = book.worksheet_cells_reader(&first).map_err(|_| unreadable())?;
        while let Some(cell) = reader.next_cell().map_err(|_| unreadable())? {
            if !cells.add(cell.get_position(), Data::from(cell.get_value().clone())) {
                break;
            }
        }
    }
    Ok(cells.into_rows())
}

#[derive(Default)]
struct Cells {
    by_row: BTreeMap<u32, BTreeMap<u32, String>>,
    count: usize,
}

impl Cells {
    /// False once there is more than a sheet we accept could hold, which `tidy` then refuses.
    fn add(&mut self, (row, column): (u32, u32), value: Data) -> bool {
        if !matches!(value, Data::Empty) {
            self.by_row.entry(row).or_default().insert(column, cell(&value));
            self.count += 1;
        }
        self.count < MAX_CELLS && self.by_row.len() <= MAX_ROWS + 2
    }

    fn into_rows(self) -> Vec<Vec<String>> {
        let first_column = self.by_row.values().filter_map(|row| row.keys().next().copied()).min().unwrap_or(0);
        self.by_row
            .into_values()
            .map(|row| {
                let last = row.keys().next_back().map_or(0, |c| c - first_column);
                let mut out = vec![String::new(); last.min(MAX_COLUMNS as u32) as usize + 1];
                for (column, value) in row {
                    if let Some(slot) = out.get_mut((column - first_column) as usize) {
                        *slot = value;
                    }
                }
                out
            })
            .collect()
    }
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
        return Err(bad_request("Use at most {max} columns").params([("max", &MAX_COLUMNS)]));
    }
    if rows.len() > MAX_ROWS {
        return Err(bad_request("Send to at most {max} rows at a time").params([("max", &MAX_ROWS)]));
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

    fn xlsx(cells: &[(u32, u16, &str)]) -> Vec<u8> {
        let mut book = rust_xlsxwriter::Workbook::new();
        let sheet = book.add_worksheet();
        for (row, column, value) in cells {
            sheet.write_string(*row, *column, *value).unwrap();
        }
        book.save_to_buffer().unwrap()
    }

    #[test]
    fn reads_excel() {
        let bytes = xlsx(&[(0, 1, "Email"), (0, 2, "Name"), (2, 1, "a@example.test"), (2, 2, "Bo")]);
        let sheet = read("a.xlsx", &bytes).unwrap();
        assert_eq!(sheet.columns, ["Email", "Name"]);
        assert_eq!(sheet.rows, vec![vec!["a@example.test".to_string(), "Bo".into()]]);
    }

    #[test]
    fn far_apart_cells_do_not_allocate_the_whole_sheet() {
        let bytes = xlsx(&[(0, 0, "Email"), (1_048_575, 16_383, "x")]);
        let sheet = read("a.xlsx", &bytes).unwrap();
        assert_eq!(sheet.columns, ["Email"]);
        assert!(sheet.rows.is_empty());
    }

    #[test]
    fn too_wide_and_too_long_sheets_are_refused() {
        let wide: Vec<(u32, u16, String)> = (0..70).map(|c| (0, c, format!("C{c}"))).collect();
        let wide: Vec<(u32, u16, &str)> = wide.iter().map(|(r, c, v)| (*r, *c, v.as_str())).collect();
        assert!(read("a.xlsx", &xlsx(&wide)).unwrap_err().message.contains("columns"));
        let long: Vec<(u32, u16, &str)> = (0..1100).map(|r| (r, 0, "x")).collect();
        assert!(read("a.xlsx", &xlsx(&long)).unwrap_err().message.contains("rows"));
    }

    #[test]
    fn formats_that_cannot_be_read_safely_are_refused() {
        assert!(read("a.xls", &[0xD0, 0xCF, 0x11, 0xE0, 0, 0]).unwrap_err().message.contains(".xlsx"));
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("content.xml", zip::write::SimpleFileOptions::default()).unwrap();
        let ods = zip.finish().unwrap().into_inner();
        assert!(read("a.ods", &ods).unwrap_err().message.contains(".xlsx"));
    }
}
