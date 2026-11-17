//! A table of text: what a report is made of (ADR-0014).
//!
//! The reports of a project (bill of materials, wires, terminals, ...) are
//! derived into tables, and a table is written as text, CSV, JSON or PDF
//! pages by whoever needs it. The table itself carries no layout.

use serde::Serialize;

/// A titled table with named columns.
///
/// Every row has one cell per column; the constructors of reports keep to
/// that, and [`Table::push`] pads or cuts a row that does not.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Table {
    /// What the table is, such as `Bill of materials`.
    pub title: String,
    /// The column headings.
    pub columns: Vec<String>,
    /// The rows, each with one cell per column.
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// An empty table with these column headings.
    #[must_use]
    pub fn new(title: &str, columns: &[&str]) -> Self {
        Self {
            title: title.to_owned(),
            columns: columns.iter().map(|column| (*column).to_owned()).collect(),
            rows: Vec::new(),
        }
    }

    /// Add a row; a missing cell is empty and an extra cell is dropped.
    pub fn push(&mut self, cells: Vec<String>) {
        let mut cells = cells;
        cells.resize(self.columns.len(), String::new());
        self.rows.push(cells);
    }

    /// The table as comma-separated values: a header line, then one line per
    /// row, each ending in a line feed.
    ///
    /// A cell with a comma, a quote or a line break is quoted. A cell that a
    /// spreadsheet would read as a formula (it starts with `=`, `+`, `-`,
    /// `@`, a tab or a carriage return) gets a leading apostrophe, so opening
    /// a report never runs anything that a designator or a description says.
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut out = String::new();
        for cells in std::iter::once(&self.columns).chain(self.rows.iter()) {
            let line: Vec<String> = cells.iter().map(|cell| csv_cell(cell)).collect();
            out.push_str(&line.join(","));
            out.push('\n');
        }
        out
    }

    /// The table as aligned text: the title, the headings, a rule and the
    /// rows, with two spaces between columns. Widths count characters.
    #[must_use]
    pub fn to_text(&self) -> String {
        let widths: Vec<usize> = (0..self.columns.len())
            .map(|index| {
                std::iter::once(&self.columns)
                    .chain(self.rows.iter())
                    .filter_map(|cells| cells.get(index))
                    .map(|cell| cell.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let line = |cells: &[String]| -> String {
            let padded: Vec<String> = cells
                .iter()
                .zip(&widths)
                .map(|(cell, width)| {
                    let fill = width.saturating_sub(cell.chars().count());
                    format!("{cell}{}", " ".repeat(fill))
                })
                .collect();
            padded.join("  ").trim_end().to_owned()
        };
        let mut out = format!("{}\n", self.title);
        out.push_str(&line(&self.columns));
        out.push('\n');
        let rule: usize = widths
            .iter()
            .sum::<usize>()
            .saturating_add(widths.len().saturating_sub(1).saturating_mul(2));
        out.push_str(&"-".repeat(rule));
        out.push('\n');
        for row in &self.rows {
            out.push_str(&line(row));
            out.push('\n');
        }
        out
    }
}

fn csv_cell(cell: &str) -> String {
    let guarded = if cell.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{cell}")
    } else {
        cell.to_owned()
    };
    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let mut table = Table::new("Parts", &["Ref", "Note"]);
        table.push(vec!["K1".to_owned(), "coil, 24 V".to_owned()]);
        table.push(vec!["S1".to_owned()]);
        table
    }

    #[test]
    fn a_short_row_is_padded_and_a_long_row_is_cut() {
        let mut table = table();
        table.push(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);

        assert_eq!(table.rows[1], ["S1", ""]);
        assert_eq!(table.rows[2], ["a", "b"]);
    }

    #[test]
    fn csv_quotes_what_needs_it_and_ends_every_line_with_a_line_feed() {
        let mut table = table();
        table.push(vec!["say \"hi\"".to_owned(), "two\nlines".to_owned()]);

        assert_eq!(
            table.to_csv(),
            "Ref,Note\nK1,\"coil, 24 V\"\nS1,\n\"say \"\"hi\"\"\",\"two\nlines\"\n"
        );
    }

    #[test]
    fn csv_does_not_let_a_cell_become_a_spreadsheet_formula() {
        let mut table = Table::new("t", &["a"]);
        for cell in ["=1+1", "+1", "-1", "@SUM(A1)", "\tx", "ok"] {
            table.push(vec![cell.to_owned()]);
        }

        assert_eq!(table.to_csv(), "a\n'=1+1\n'+1\n'-1\n'@SUM(A1)\n'\tx\nok\n");
    }

    #[test]
    fn text_aligns_the_columns_under_their_headings() {
        assert_eq!(
            table().to_text(),
            "Parts\nRef  Note\n---------------\nK1   coil, 24 V\nS1\n"
        );
    }

    #[test]
    fn an_empty_table_has_only_its_headings() {
        let table = Table::new("Empty", &["A", "B"]);

        assert_eq!(table.to_csv(), "A,B\n");
        assert_eq!(table.to_text(), "Empty\nA  B\n----\n");
    }
}
