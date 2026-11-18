//! Search over the whole project (M9.2, issue #118): devices by designator,
//! model, catalog data and parameters; wires by number; sheets, text notes
//! and layers by their texts. The window lists what is found and selects it;
//! the search is here so that every front end finds the same things.

use circuit_model::{
    ComponentCatalog, ParamValue, ReferenceDesignator, Schematic, StaticNets, Subject, WireNumbers,
    natural_key,
};

use crate::{CommandError, Edit, Transaction, layers, ops};

/// One element found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Found {
    /// The element.
    pub subject: Subject,
    /// What it is: `device`, `wire`, `sheet`, `text` or `layer`.
    pub kind: &'static str,
    /// How a person names it: a designator, a wire number, a sheet title.
    pub label: String,
    /// The text that matched, when it is not the label.
    pub detail: String,
}

/// Whether `text` contains `needle`, ignoring case. `needle` is lower case.
fn matches(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(needle)
}

/// The elements whose texts contain `query`, ignoring case, at most `limit`:
/// devices in natural order of designators, then wires in numbering order,
/// sheets by number, text notes and layers by ID. An empty query finds
/// nothing.
#[must_use]
pub fn find(doc: &Schematic, catalog: &ComponentCatalog, query: &str, limit: usize) -> Vec<Found> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut found = Vec::new();

    let mut devices: Vec<_> = doc.devices.iter().collect();
    devices.sort_by_key(|device| (natural_key(&device.reference.0), device.id));
    for device in devices {
        let info = device.info.clone().unwrap_or_default();
        let params = device.params.iter().map(|(name, value)| {
            let value = match value {
                ParamValue::Bool(flag) => flag.to_string(),
                ParamValue::Int(number) => number.to_string(),
                ParamValue::Text(text) => text.clone(),
            };
            format!("{name}={value}")
        });
        let texts: Vec<String> = [
            Some(device.reference.0.clone()),
            Some(device.model.id.clone()),
            catalog
                .model(&device.model)
                .map(|model| model.title().to_owned()),
            info.manufacturer,
            info.part_no,
            info.description,
            info.location,
        ]
        .into_iter()
        .flatten()
        .chain(params)
        .collect();
        if let Some(hit) = texts.iter().find(|text| matches(text, &needle)) {
            found.push(Found {
                subject: Subject::Device(device.id),
                kind: "device",
                label: device.reference.0.clone(),
                detail: if hit == &device.reference.0 {
                    device.model.id.clone()
                } else {
                    hit.clone()
                },
            });
        }
    }

    let nets = StaticNets::build(doc, catalog);
    let numbers = WireNumbers::build(doc, catalog, &nets);
    let mut wires: Vec<(String, _)> = doc
        .wires
        .iter()
        .filter_map(|wire| {
            let number = numbers.of_wire(wire.id)?.to_owned();
            matches(&number, &needle).then_some((number, wire.id))
        })
        .collect();
    wires.sort_by_key(|(number, id)| (natural_key(number), *id));
    found.extend(wires.into_iter().map(|(number, id)| Found {
        subject: Subject::Wire(id),
        kind: "wire",
        label: number,
        detail: String::new(),
    }));

    let mut sheets: Vec<_> = doc.sheets.iter().collect();
    sheets.sort_by_key(|sheet| (sheet.number, sheet.id));
    for sheet in sheets {
        let label = format!("{}: {}", sheet.number, sheet.title);
        if matches(&label, &needle) {
            found.push(Found {
                subject: Subject::Sheet(sheet.id),
                kind: "sheet",
                label,
                detail: String::new(),
            });
        }
    }

    let mut texts: Vec<_> = doc.texts.iter().collect();
    texts.sort_by_key(|text| text.id);
    found.extend(
        texts
            .into_iter()
            .filter(|text| matches(&text.text, &needle))
            .map(|text| Found {
                subject: Subject::Text(text.id),
                kind: "text",
                label: text.text.clone(),
                detail: String::new(),
            }),
    );

    let mut layers: Vec<_> = doc.layers.iter().collect();
    layers.sort_by_key(|layer| layer.id);
    found.extend(
        layers
            .into_iter()
            .filter(|layer| matches(&layer.name, &needle))
            .map(|layer| Found {
                subject: Subject::Layer(layer.id),
                kind: "layer",
                label: layer.name.clone(),
                detail: String::new(),
            }),
    );

    found.truncate(limit);
    found
}

/// What a replacement changes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Fields {
    /// The designators of the devices.
    pub designators: bool,
    /// The text notes.
    pub notes: bool,
    /// The text parameters of the devices.
    pub parameters: bool,
}

impl Fields {
    /// Designators, notes and parameters.
    pub const ALL: Self = Self {
        designators: true,
        notes: true,
        parameters: true,
    };
}

/// Replace every occurrence of `find` (matched exactly, with its case) by
/// `with` in the chosen fields, in one transaction: undone in one step. The
/// result is refused as a whole when one new designator is not valid or
/// would be taken twice, when a new note or parameter value is not valid,
/// or when an element to change is on a locked layer. An empty `find`
/// changes nothing.
///
/// # Errors
///
/// Returns [`CommandError::InvalidReference`],
/// [`CommandError::DuplicateReference`], [`CommandError::InvalidParameter`]
/// or [`CommandError::Locked`] as above.
pub fn replace(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    fields: Fields,
    find: &str,
    with: &str,
) -> Result<Transaction, CommandError> {
    let mut tx = Transaction::new(
        format!("Replace \u{201c}{find}\u{201d} with \u{201c}{with}\u{201d}"),
        doc.next_id,
    );
    if find.is_empty() {
        return Ok(tx);
    }
    let mut devices: Vec<_> = doc.devices.iter().collect();
    devices.sort_by_key(|device| device.id);

    if fields.designators {
        let renamed: Vec<_> = devices
            .iter()
            .filter(|device| device.reference.0.contains(find))
            .map(|device| {
                (
                    device.id,
                    ReferenceDesignator(device.reference.0.replace(find, with)),
                )
            })
            .collect();
        for (id, reference) in &renamed {
            if !reference.is_valid() {
                return Err(CommandError::InvalidReference(reference.0.clone()));
            }
            layers::ensure_unlocked(doc, [&Subject::Device(*id)])?;
        }
        // The designators after the replacement, all of them, must differ.
        let mut after: Vec<&str> = devices
            .iter()
            .map(|device| {
                renamed
                    .iter()
                    .find(|(id, _)| *id == device.id)
                    .map_or(device.reference.0.as_str(), |(_, new)| new.0.as_str())
            })
            .collect();
        after.sort_unstable();
        if let Some(pair) = after.windows(2).find(|pair| pair.first() == pair.get(1)) {
            let taken = pair.first().copied().unwrap_or_default();
            return Err(CommandError::DuplicateReference(taken.to_owned()));
        }
        tx.edits.extend(
            renamed
                .into_iter()
                .map(|(device, reference)| Edit::SetReference { device, reference }),
        );
    }

    if fields.parameters {
        for device in &devices {
            for (key, value) in &device.params {
                let ParamValue::Text(text) = value else {
                    continue;
                };
                if !text.contains(find) {
                    continue;
                }
                let new = ParamValue::Text(text.replace(find, with));
                let one = ops::set_param(doc, catalog, device.id, key, Some(new))?;
                tx.edits.extend(one.edits);
            }
        }
    }

    if fields.notes {
        let mut notes: Vec<_> = doc.texts.iter().collect();
        notes.sort_by_key(|note| note.id);
        for note in notes.into_iter().filter(|note| note.text.contains(find)) {
            let one = ops::set_text(doc, note.id, &note.text.replace(find, with))?;
            tx.edits.extend(one.edits);
        }
    }
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use circuit_model::builder::pin;
    use circuit_model::{DeviceInfo, GridPoint, SchematicBuilder};

    use super::*;

    fn project() -> Schematic {
        let mut b = SchematicBuilder::new("find");
        let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
        let k10 = b.device("K10", "contactor.generic", &[]).unwrap();
        let k2 = b.device("K2", "contactor.generic", &[]).unwrap();
        let contact = b.symbol(s1, "contact", None, GridPoint::new(0, 0)).unwrap();
        let coil = b.symbol(k2, "coil", None, GridPoint::new(10, 0)).unwrap();
        b.symbol(k10, "coil", None, GridPoint::new(20, 0)).unwrap();
        b.wire(pin(contact, "p2"), pin(coil, "a1")).unwrap();
        let mut doc = b.finish();
        if let Some(device) = doc.devices.iter_mut().find(|d| d.reference.0 == "S1") {
            device.info = Some(DeviceInfo {
                description: Some("Start pump".to_owned()),
                ..DeviceInfo::default()
            });
        }
        doc
    }

    fn labels(found: &[Found]) -> Vec<(&str, &str)> {
        found.iter().map(|f| (f.kind, f.label.as_str())).collect()
    }

    #[test]
    fn devices_are_found_by_designator_in_natural_order_ignoring_case() {
        let doc = project();
        let found = find(&doc, &ComponentCatalog::builtin(), "k", 10);

        assert_eq!(labels(&found), [("device", "K2"), ("device", "K10")]);
        assert_eq!(found[0].detail, "contactor.generic");
    }

    #[test]
    fn catalog_data_says_what_matched() {
        let doc = project();
        let found = find(&doc, &ComponentCatalog::builtin(), "PUMP", 10);

        assert_eq!(labels(&found), [("device", "S1")]);
        assert_eq!(found[0].detail, "Start pump");
    }

    #[test]
    fn wires_are_found_by_their_number() {
        let mut doc = project();
        doc.wires[0].number = Some("W17".to_owned());
        let catalog = ComponentCatalog::builtin();

        let found = find(&doc, &catalog, "w1", 10);

        assert_eq!(labels(&found), [("wire", "W17")]);
        assert_eq!(found[0].subject, Subject::Wire(doc.wires[0].id));
    }

    fn apply(doc: &Schematic, tx: &Transaction) -> Schematic {
        tx.apply_to(doc).unwrap().0
    }

    fn references(doc: &Schematic) -> Vec<&str> {
        let mut names: Vec<&str> = doc.devices.iter().map(|d| d.reference.0.as_str()).collect();
        names.sort_unstable();
        names
    }

    #[test]
    fn designators_are_replaced_in_one_step() {
        let doc = project();
        let tx = replace(
            &doc,
            &ComponentCatalog::builtin(),
            Fields {
                designators: true,
                ..Fields::default()
            },
            "K",
            "KA",
        )
        .unwrap();

        assert_eq!(tx.edits.len(), 2);
        assert_eq!(references(&apply(&doc, &tx)), ["KA10", "KA2", "S1"]);
    }

    #[test]
    fn a_swap_is_allowed_and_a_collision_is_refused() {
        let doc = project();
        let catalog = ComponentCatalog::builtin();
        let designators = Fields {
            designators: true,
            ..Fields::default()
        };

        // K10 becomes K20, K2 stays: no clash.
        assert!(replace(&doc, &catalog, designators, "10", "20").is_ok());
        // K10 would become K2, which K2 is.
        assert_eq!(
            replace(&doc, &catalog, designators, "10", "2"),
            Err(CommandError::DuplicateReference("K2".to_owned()))
        );
        assert!(matches!(
            replace(&doc, &catalog, designators, "K", " "),
            Err(CommandError::InvalidReference(_))
        ));
    }

    #[test]
    fn notes_change_and_fields_not_chosen_stay() {
        let mut doc = project();
        let tx = crate::ops::add_text(
            &doc,
            GridPoint::new(0, 20),
            "Pump K2 start",
            circuit_model::TextAnchor::default(),
        )
        .unwrap();
        doc = apply(&doc, &tx);
        let notes = Fields {
            notes: true,
            ..Fields::default()
        };

        let tx = replace(&doc, &ComponentCatalog::builtin(), notes, "K2", "KM2").unwrap();
        let after = apply(&doc, &tx);

        assert_eq!(after.texts[0].text, "Pump KM2 start");
        assert_eq!(references(&after), references(&doc));
        assert!(
            replace(&doc, &ComponentCatalog::builtin(), Fields::ALL, "", "x")
                .unwrap()
                .edits
                .is_empty()
        );
    }

    #[test]
    fn an_empty_query_or_limit_finds_nothing_and_the_limit_holds() {
        let doc = project();
        let catalog = ComponentCatalog::builtin();

        assert!(find(&doc, &catalog, "  ", 10).is_empty());
        assert!(find(&doc, &catalog, "k", 0).is_empty());
        assert_eq!(find(&doc, &catalog, "k", 1).len(), 1);
    }
}
