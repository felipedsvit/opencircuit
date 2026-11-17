//! The comparison of two versions of a project (M9.1, issue #118).
//!
//! Elements are matched by their stable IDs, which are never reused
//! (ADR-0001); waivers, which have none, by their code and subject. The
//! counter `next_id` and the schema version are left out: they change with
//! every addition and every migration and say nothing a person decided.
//!
//! Each element type is compared field by field, and the field lists below
//! destructure the types exhaustively: a field added to the model fails to
//! compile here until it is compared, so no change goes unreported.

use std::collections::BTreeMap;
use std::fmt::Display;

use circuit_model::{
    Comment, Device, DeviceView, Group, Junction, Layer, PlcModule, ProjectMeta, Schematic, Shape,
    Sheet, SupplyProfile, SymbolInstance, Table, TextNote, Waiver, Wire,
};

/// The fields of `$a` that differ from those of `$b`. The pattern names
/// every field of the type, so the list cannot fall behind the model.
macro_rules! changed {
    ($a:expr, $b:expr, $ty:ident { $($field:ident),* $(,)? }) => {{
        let $ty { $($field: _),* } = $a;
        let mut fields: Vec<&'static str> = Vec::new();
        $(
            if $a.$field != $b.$field {
                fields.push(stringify!($field));
            }
        )*
        fields
    }};
}

/// How one element differs.
struct Row {
    change: &'static str,
    element: &'static str,
    id: String,
    name: String,
    fields: Vec<&'static str>,
}

/// Compare one collection: elements only in `before` are removed, only in
/// `after` added, in both with different fields changed.
fn compare<T, K: Ord + Display>(
    rows: &mut Vec<Row>,
    element: &'static str,
    before: &[T],
    after: &[T],
    key: impl Fn(&T) -> K,
    name: impl Fn(&T) -> String,
    fields: impl Fn(&T, &T) -> Vec<&'static str>,
) {
    let old: BTreeMap<K, &T> = before.iter().map(|item| (key(item), item)).collect();
    let new: BTreeMap<K, &T> = after.iter().map(|item| (key(item), item)).collect();
    let mut keys: Vec<&K> = old.keys().chain(new.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let row = match (old.get(k), new.get(k)) {
            (Some(a), None) => Some(("removed", name(a), Vec::new())),
            (None, Some(b)) => Some(("added", name(b), Vec::new())),
            (Some(a), Some(b)) => {
                let fields = fields(a, b);
                (!fields.is_empty()).then(|| ("changed", name(b), fields))
            }
            (None, None) => None,
        };
        if let Some((change, name, fields)) = row {
            rows.push(Row {
                change,
                element,
                id: k.to_string(),
                name,
                fields,
            });
        }
    }
}

fn project_fields(a: &ProjectMeta, b: &ProjectMeta) -> Vec<&'static str> {
    changed!(
        a,
        b,
        ProjectMeta {
            name,
            grid_unit,
            author,
            company,
            revision,
            date,
            wire_numbering,
            client,
        }
    )
}

fn sheet_fields(a: &Sheet, b: &Sheet) -> Vec<&'static str> {
    changed!(
        a,
        b,
        Sheet {
            id,
            number,
            title,
            origin,
            size,
            orientation,
            zones,
            kind,
        }
    )
}

fn supply_fields(a: &SupplyProfile, b: &SupplyProfile) -> Vec<&'static str> {
    changed!(
        a,
        b,
        SupplyProfile {
            id,
            kind,
            nominal_voltage_mv,
            phase_voltage_mv,
            frequency_millihz,
            phase_sequence,
        }
    )
}

fn device_fields(a: &Device, b: &Device) -> Vec<&'static str> {
    changed!(
        a,
        b,
        Device {
            id,
            reference,
            model,
            params,
            info,
        }
    )
}

fn symbol_fields(a: &SymbolInstance, b: &SymbolInstance) -> Vec<&'static str> {
    changed!(
        a,
        b,
        SymbolInstance {
            id,
            device_id,
            symbol_key,
            slot,
            position,
            rotation,
            layer,
            mirrored,
        }
    )
}

fn junction_fields(a: &Junction, b: &Junction) -> Vec<&'static str> {
    changed!(a, b, Junction { id, position })
}

fn wire_fields(a: &Wire, b: &Wire) -> Vec<&'static str> {
    changed!(
        a,
        b,
        Wire {
            id,
            from,
            to,
            route,
            number,
            layer,
            spec,
        }
    )
}

fn waiver_fields(a: &Waiver, b: &Waiver) -> Vec<&'static str> {
    changed!(
        a,
        b,
        Waiver {
            code,
            subject,
            reason,
        }
    )
}

fn text_fields(a: &TextNote, b: &TextNote) -> Vec<&'static str> {
    changed!(
        a,
        b,
        TextNote {
            id,
            position,
            text,
            anchor,
            layer,
        }
    )
}

fn shape_fields(a: &Shape, b: &Shape) -> Vec<&'static str> {
    changed!(a, b, Shape { id, kind, layer })
}

fn comment_fields(a: &Comment, b: &Comment) -> Vec<&'static str> {
    changed!(
        a,
        b,
        Comment {
            id,
            subject,
            text,
            author,
        }
    )
}

fn group_fields(a: &Group, b: &Group) -> Vec<&'static str> {
    changed!(a, b, Group { id, name, members })
}

fn view_fields(a: &DeviceView, b: &DeviceView) -> Vec<&'static str> {
    changed!(
        a,
        b,
        DeviceView {
            id,
            device_id,
            position,
            width,
            height,
            layer,
        }
    )
}

fn plc_fields(a: &PlcModule, b: &PlcModule) -> Vec<&'static str> {
    changed!(
        a,
        b,
        PlcModule {
            id,
            name,
            description,
            position,
            channels,
        }
    )
}

fn layer_fields(a: &Layer, b: &Layer) -> Vec<&'static str> {
    changed!(
        a,
        b,
        Layer {
            id,
            name,
            visible,
            locked,
        }
    )
}

/// What changed from `before` to `after`, one row per element added,
/// removed or changed: the change, the kind of element, its ID, a name a
/// person recognizes (a designator, a sheet title, a layer name) and the
/// fields that differ. No row: the two are the same document.
#[must_use]
pub fn diff(before: &Schematic, after: &Schematic) -> Table {
    let mut rows: Vec<Row> = Vec::new();
    let project = project_fields(&before.project, &after.project);
    if !project.is_empty() {
        rows.push(Row {
            change: "changed",
            element: "project",
            id: String::new(),
            name: after.project.name.clone(),
            fields: project,
        });
    }
    compare(
        &mut rows,
        "sheet",
        &before.sheets,
        &after.sheets,
        |sheet| sheet.id.get(),
        |sheet| format!("{}: {}", sheet.number, sheet.title),
        sheet_fields,
    );
    compare(
        &mut rows,
        "supply",
        &before.supplies,
        &after.supplies,
        |supply| supply.id.get(),
        |_| String::new(),
        supply_fields,
    );
    // A symbol is named by its device in the version it belongs to.
    let reference = |doc: &Schematic, symbol: &SymbolInstance| {
        let device = doc
            .devices
            .iter()
            .find(|device| device.id == symbol.device_id)
            .map(|device| device.reference.0.as_str())
            .unwrap_or("?");
        format!("{device} {}", symbol.symbol_key.0)
    };
    compare(
        &mut rows,
        "device",
        &before.devices,
        &after.devices,
        |device| device.id.get(),
        |device| device.reference.0.clone(),
        device_fields,
    );
    let mut symbols: Vec<Row> = Vec::new();
    compare(
        &mut symbols,
        "symbol",
        &before.symbols,
        &after.symbols,
        |symbol| symbol.id.get(),
        |symbol| {
            if after.symbols.iter().any(|other| other.id == symbol.id) {
                reference(after, symbol)
            } else {
                reference(before, symbol)
            }
        },
        symbol_fields,
    );
    rows.append(&mut symbols);
    compare(
        &mut rows,
        "junction",
        &before.junctions,
        &after.junctions,
        |junction| junction.id.get(),
        |_| String::new(),
        junction_fields,
    );
    compare(
        &mut rows,
        "wire",
        &before.wires,
        &after.wires,
        |wire| wire.id.get(),
        |wire| wire.number.clone().unwrap_or_default(),
        wire_fields,
    );
    compare(
        &mut rows,
        "waiver",
        &before.waivers,
        &after.waivers,
        |waiver| format!("{} {}", waiver.code, waiver.subject),
        |waiver| waiver.code.clone(),
        waiver_fields,
    );
    compare(
        &mut rows,
        "text",
        &before.texts,
        &after.texts,
        |text| text.id.get(),
        |text| text.text.chars().take(40).collect(),
        text_fields,
    );
    compare(
        &mut rows,
        "layer",
        &before.layers,
        &after.layers,
        |layer| layer.id.get(),
        |layer| layer.name.clone(),
        layer_fields,
    );
    compare(
        &mut rows,
        "shape",
        &before.shapes,
        &after.shapes,
        |shape| shape.id.get(),
        |shape| shape.kind.name().to_owned(),
        shape_fields,
    );
    compare(
        &mut rows,
        "comment",
        &before.comments,
        &after.comments,
        |comment| comment.id.get(),
        |comment| comment.text.chars().take(40).collect(),
        comment_fields,
    );
    compare(
        &mut rows,
        "group",
        &before.groups,
        &after.groups,
        |group| group.id.get(),
        |group| group.name.clone(),
        group_fields,
    );
    let viewed = |doc: &Schematic, view: &DeviceView| {
        doc.devices
            .iter()
            .find(|device| device.id == view.device_id)
            .map(|device| device.reference.0.clone())
            .unwrap_or_default()
    };
    compare(
        &mut rows,
        "view",
        &before.views,
        &after.views,
        |view| view.id.get(),
        |view| {
            if after.views.iter().any(|other| other.id == view.id) {
                viewed(after, view)
            } else {
                viewed(before, view)
            }
        },
        view_fields,
    );
    compare(
        &mut rows,
        "plc",
        &before.plcs,
        &after.plcs,
        |plc| plc.id.get(),
        |plc| plc.name.clone(),
        plc_fields,
    );

    let mut table = Table::new("Changes", &["Change", "Element", "ID", "Name", "Fields"]);
    for row in rows {
        table.push(vec![
            row.change.to_owned(),
            row.element.to_owned(),
            row.id,
            row.name,
            row.fields.join(", "),
        ]);
    }
    table
}

#[cfg(test)]
mod tests {
    use circuit_model::builder::pin;
    use circuit_model::{GridPoint, JunctionId, ReferenceDesignator, SchematicBuilder};

    use super::*;

    /// S1 feeds the coil of K1 through a junction.
    fn sample() -> Schematic {
        let mut b = SchematicBuilder::new("Starter");
        let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
        let k1 = b.device("K1", "contactor.generic", &[]).unwrap();
        let contact = b.symbol(s1, "contact", None, GridPoint::new(0, 0)).unwrap();
        let coil = b.symbol(k1, "coil", None, GridPoint::new(10, 0)).unwrap();
        b.junction(GridPoint::new(5, 5)).unwrap();
        b.wire(pin(contact, "p2"), pin(coil, "a1")).unwrap();
        b.wire(pin(coil, "a2"), pin(contact, "p1")).unwrap();
        b.finish()
    }

    fn cells(table: &Table) -> Vec<Vec<&str>> {
        table
            .rows
            .iter()
            .map(|row| row.iter().map(String::as_str).collect())
            .collect()
    }

    #[test]
    fn a_document_has_no_difference_with_itself() {
        let doc = sample();

        assert!(diff(&doc, &doc).rows.is_empty());
    }

    #[test]
    fn a_renamed_device_is_one_changed_row_naming_the_field() {
        let before = sample();
        let mut after = before.clone();
        let device = after.devices.first_mut().unwrap();
        let id = device.id;
        device.reference = ReferenceDesignator("K9".to_owned());

        let table = diff(&before, &after);

        assert_eq!(
            cells(&table),
            [[
                "changed",
                "device",
                id.get().to_string().as_str(),
                "K9",
                "reference"
            ]]
        );
    }

    #[test]
    fn added_and_removed_elements_are_listed_and_the_counter_is_not() {
        let before = sample();
        let mut after = before.clone();
        let removed = after.wires.remove(0);
        after.next_id += 10;
        let mut added = after.junctions.first().cloned().unwrap();
        added.id = JunctionId(after.next_id - 1);
        after.junctions.push(added);

        let table = diff(&before, &after);
        let changes: Vec<(&str, &str)> = table
            .rows
            .iter()
            .map(|row| (row[0].as_str(), row[1].as_str()))
            .collect();

        assert_eq!(changes, [("added", "junction"), ("removed", "wire")]);
        assert_eq!(table.rows[1][2], removed.id.get().to_string());
    }

    #[test]
    fn a_moved_symbol_is_named_by_its_device() {
        let before = sample();
        let mut after = before.clone();
        let symbol = after
            .symbols
            .iter_mut()
            .find(|symbol| symbol.symbol_key.0 == "coil")
            .unwrap();
        symbol.position.x += 1;

        let table = diff(&before, &after);

        assert_eq!(
            cells(&table)
                .into_iter()
                .map(|row| (row[1], row[3], row[4]))
                .collect::<Vec<_>>(),
            [("symbol", "K1 coil", "position")]
        );
    }

    #[test]
    fn the_order_of_the_collections_does_not_matter() {
        let before = sample();
        let mut after = before.clone();
        after.devices.reverse();
        after.symbols.reverse();
        after.wires.reverse();

        assert!(diff(&before, &after).rows.is_empty());
    }

    #[test]
    fn project_data_changes_are_one_row() {
        let before = sample();
        let mut after = before.clone();
        after.project.author = Some("Ana".to_owned());
        after.project.revision = Some("B".to_owned());

        assert_eq!(
            cells(&diff(&before, &after)),
            [["changed", "project", "", "Starter", "author, revision"]]
        );
    }
}
