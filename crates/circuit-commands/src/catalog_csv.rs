//! Manufacturer data imported from CSV into the catalog data of devices
//! (ADR-0018).
//!
//! A header row names the columns `reference` (required), `manufacturer`,
//! `part_no`, `description`, `location` and `datasheet`, in any order and
//! case; quoting follows RFC 4180. Each row sets the data of the device with
//! that designator: a filled cell replaces the field, an empty cell leaves
//! it. The core reads text only; whoever calls it reads the file.

use circuit_model::{DeviceInfo, Schematic};

use crate::{CommandError, Transaction};

/// Most rows read, besides the header.
pub const MAX_ROWS: usize = 10_000;
/// Most columns of a row.
const MAX_COLUMNS: usize = 64;

/// What an import did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Imported {
    /// How many devices it changes.
    pub changed: usize,
    /// The designators of rows the project has no device for, in file order.
    pub unknown: Vec<String>,
}

/// Split CSV text into rows of cells, by RFC 4180: cells separated by
/// commas, a cell in double quotes may hold commas, line breaks and doubled
/// quotes. Lines may end in CRLF or LF; a blank line is skipped.
///
/// # Errors
///
/// A message for a quote left open, a quote inside an unquoted cell, or more
/// rows or columns than the limits.
pub fn parse(text: &str) -> Result<Vec<Vec<String>>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let mut chars = text.chars().peekable();
    let mut line: usize = 1;
    let finish_row = |rows: &mut Vec<Vec<String>>, row: &mut Vec<String>| -> Result<(), String> {
        let done = std::mem::take(row);
        if done.len() == 1 && done.first().is_some_and(String::is_empty) {
            return Ok(());
        }
        if rows.len() > MAX_ROWS {
            return Err(format!("more than {MAX_ROWS} rows"));
        }
        rows.push(done);
        Ok(())
    };
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => quoted = false,
                '\n' => {
                    line = line.saturating_add(1);
                    cell.push(c);
                }
                other => cell.push(other),
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() && !was_quoted => {
                quoted = true;
                was_quoted = true;
            }
            '"' => {
                return Err(format!(
                    "line {line}: a quote inside a cell that is not quoted"
                ));
            }
            ',' => {
                if row.len() >= MAX_COLUMNS {
                    return Err(format!("line {line}: more than {MAX_COLUMNS} columns"));
                }
                row.push(std::mem::take(&mut cell));
                was_quoted = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                was_quoted = false;
                finish_row(&mut rows, &mut row)?;
                line = line.saturating_add(1);
            }
            other => {
                if was_quoted {
                    return Err(format!("line {line}: text after a closing quote"));
                }
                cell.push(other);
            }
        }
    }
    if quoted {
        return Err(format!("line {line}: a quote is not closed"));
    }
    if !cell.is_empty() || !row.is_empty() || was_quoted {
        row.push(cell);
        finish_row(&mut rows, &mut row)?;
    }
    Ok(rows)
}

/// The column names the import knows.
const COLUMNS: [&str; 6] = [
    "reference",
    "manufacturer",
    "part_no",
    "description",
    "location",
    "datasheet",
];

/// Set the catalog data of the devices of `doc` from CSV `text`, in one
/// transaction.
///
/// # Errors
///
/// [`CommandError::Invalid`] when the text is not CSV, has no `reference`
/// column, names a column twice or one it does not know, or a value breaks
/// a bound of the catalog data; [`CommandError::Locked`] for a device on a
/// locked layer. The document is then unchanged.
pub fn import(doc: &Schematic, text: &str) -> Result<(Transaction, Imported), CommandError> {
    let rows = parse(text).map_err(|message| CommandError::Invalid(format!("CSV: {message}")))?;
    let mut rows = rows.into_iter();
    let header = rows
        .next()
        .ok_or_else(|| CommandError::Invalid("CSV: the file is empty".to_owned()))?;
    let mut columns: Vec<usize> = Vec::with_capacity(header.len());
    for name in &header {
        let name = name.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        let index = COLUMNS
            .iter()
            .position(|known| *known == name)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "CSV: unknown column `{name}`; use reference, manufacturer, part_no, description, location or datasheet"
                ))
            })?;
        if columns.contains(&index) {
            return Err(CommandError::Invalid(format!(
                "CSV: the column `{name}` is given twice"
            )));
        }
        columns.push(index);
    }
    if !columns.contains(&0) {
        return Err(CommandError::Invalid(
            "CSV: there is no `reference` column".to_owned(),
        ));
    }
    let mut infos: std::collections::BTreeMap<circuit_model::DeviceId, DeviceInfo> =
        std::collections::BTreeMap::new();
    let mut imported = Imported::default();
    for row in rows {
        let cell = |index: usize| -> Option<String> {
            let at = columns.iter().position(|column| *column == index)?;
            row.get(at)
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        };
        let Some(reference) = cell(0) else {
            continue;
        };
        let Some(device) = doc
            .devices
            .iter()
            .find(|device| device.reference.0 == reference)
        else {
            imported.unknown.push(reference);
            continue;
        };
        let info = infos
            .entry(device.id)
            .or_insert_with(|| device.info.clone().unwrap_or_default());
        for (index, field) in [
            (1, &mut info.manufacturer),
            (2, &mut info.part_no),
            (3, &mut info.description),
            (4, &mut info.location),
            (5, &mut info.datasheet),
        ] {
            if let Some(value) = cell(index) {
                *field = Some(value);
            }
        }
    }
    let mut tx = Transaction::new("Import catalog data", doc.next_id);
    for (device, info) in infos {
        let edits = crate::documentation::set_device_info(doc, device, info)?.edits;
        if !edits.is_empty() {
            imported.changed = imported.changed.saturating_add(1);
        }
        tx.edits.extend(edits);
    }
    Ok((tx, imported))
}

#[cfg(test)]
mod tests {
    use circuit_model::SchematicBuilder;

    use super::*;
    use crate::History;

    fn doc() -> Schematic {
        let mut b = SchematicBuilder::new("csv");
        b.device("K1", "contactor.generic", &[]).unwrap();
        b.device("S1", "pushbutton_no", &[]).unwrap();
        let mut doc = b.finish();
        doc.devices[0].info = Some(DeviceInfo {
            location: Some("Panel 1".to_owned()),
            ..DeviceInfo::default()
        });
        doc
    }

    #[test]
    fn rfc_4180_quotes_hold_commas_quotes_and_line_breaks() {
        let rows = parse("a,\"b, c\",\"say \"\"hi\"\"\"\r\n\"two\nlines\",,x\n\n").unwrap();
        assert_eq!(
            rows,
            [vec!["a", "b, c", "say \"hi\""], vec!["two\nlines", "", "x"]]
        );
        assert!(parse("\"open").is_err());
        assert!(parse("a\"b").is_err());
        assert!(parse("\"a\"b").is_err());
        assert_eq!(parse("\u{feff}x").unwrap(), [vec!["x"]]);
    }

    #[test]
    fn rows_fill_the_devices_by_designator_and_empty_cells_keep_a_field() {
        let mut doc = doc();
        let text = "Reference,Manufacturer,Part No,Datasheet,Location\n\
                    K1,Acme,CTR-9,https://example.com/ctr9.pdf,\n\
                    S1,Acme,PB-G,,Door\n\
                    X9,Other,1,,\n";
        let (tx, imported) = import(&doc, text).unwrap();
        assert_eq!(imported.changed, 2);
        assert_eq!(imported.unknown, ["X9"]);
        let mut history = History::new();
        history.execute(&mut doc, tx).unwrap();
        let k1 = doc.devices[0].info.clone().unwrap();
        assert_eq!(k1.manufacturer.as_deref(), Some("Acme"));
        assert_eq!(k1.location.as_deref(), Some("Panel 1"));
        assert_eq!(
            k1.datasheet.as_deref(),
            Some("https://example.com/ctr9.pdf")
        );
        assert_eq!(
            doc.devices[1].info.clone().unwrap().location.as_deref(),
            Some("Door")
        );
        // One step undoes all of it.
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.devices[0].info.clone().unwrap().manufacturer, None);
    }

    #[test]
    fn a_bad_header_or_value_refuses_the_whole_file() {
        let doc = doc();
        for text in [
            "",
            "manufacturer\nAcme\n",
            "reference,colour\nK1,red\n",
            "reference,reference\nK1,K1\n",
            "reference,datasheet\nK1,javascript:alert(1)\n",
        ] {
            assert!(
                matches!(import(&doc, text), Err(CommandError::Invalid(_))),
                "{text:?}"
            );
        }
    }
}
