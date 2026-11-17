//! Search over the whole project (M9.2, issue #118): devices by designator,
//! model, catalog data and parameters; wires by number; sheets, text notes
//! and layers by their texts. The window lists what is found and selects it;
//! the search is here so that every front end finds the same things.

use circuit_model::{
    ComponentCatalog, ParamValue, Schematic, StaticNets, Subject, WireNumbers, natural_key,
};

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

    #[test]
    fn an_empty_query_or_limit_finds_nothing_and_the_limit_holds() {
        let doc = project();
        let catalog = ComponentCatalog::builtin();

        assert!(find(&doc, &catalog, "  ", 10).is_empty());
        assert!(find(&doc, &catalog, "k", 0).is_empty());
        assert_eq!(find(&doc, &catalog, "k", 1).len(), 1);
    }
}
