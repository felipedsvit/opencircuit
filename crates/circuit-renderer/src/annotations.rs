//! The documentation of schema version 5 in the scene (ADR-0018): shapes,
//! revision clouds and QR codes, device views on panel layouts and
//! single-line diagrams, and the boxes of PLC modules.
//!
//! Shapes are drawn in the [`StyleKey::Symbol`] style and their words in
//! [`StyleKey::Label`], so every front end and exporter shows them as it
//! shows the drawing; none of it depends on a simulation.

use circuit_model::{
    Category, ComponentCatalog, DeviceClass, DeviceView, GridPoint, LayerStates, PlcModule,
    Schematic, Shape as DocShape, ShapeKind, SheetKind, Subject, sheet_at,
};
use qrcodegen::{QrCode, QrCodeEcc};

use crate::scene::{Anchor, SUBUNITS_PER_GRID, Scene, ScenePoint, SceneRect, Shape, StyleKey};

/// Draw every visible shape, view and placed PLC module of `doc`.
pub(crate) fn draw_annotations(
    scene: &mut Scene,
    doc: &Schematic,
    catalog: &ComponentCatalog,
    layers: &LayerStates,
) {
    let mut shapes: Vec<&DocShape> = doc
        .shapes
        .iter()
        .filter(|shape| layers.is_visible(shape.layer))
        .collect();
    shapes.sort_by_key(|shape| shape.id);
    for shape in shapes {
        draw_shape(scene, shape);
    }
    let mut views: Vec<&DeviceView> = doc
        .views
        .iter()
        .filter(|view| layers.is_visible(view.layer))
        .collect();
    views.sort_by_key(|view| view.id);
    for view in views {
        draw_view(scene, doc, catalog, view);
    }
    let mut plcs: Vec<&PlcModule> = doc.plcs.iter().collect();
    plcs.sort_by_key(|plc| plc.id);
    for plc in plcs {
        draw_plc(scene, doc, plc);
    }
}

fn point((x, y): (i64, i64)) -> ScenePoint {
    ScenePoint::new(x, y)
}

fn grid(point: GridPoint) -> ScenePoint {
    ScenePoint::from_grid(point)
}

/// One shape: its lines, and the revision mark of a cloud or the modules of
/// a QR code.
fn draw_shape(scene: &mut Scene, shape: &DocShape) {
    let subject = Some(Subject::Shape(shape.id));
    // A shape a file made invalid is drawn as far as it can be: what has no
    // place is left out, as validation warns about it.
    match &shape.kind {
        ShapeKind::Rectangle { from, to } => {
            scene.push(
                Shape::Rect(SceneRect::from_corners(grid(*from), grid(*to))),
                StyleKey::Symbol,
                subject,
            );
            return;
        }
        ShapeKind::Circle { center, radius } => {
            scene.push(
                Shape::Circle {
                    center: grid(*center),
                    radius: i64::from(*radius).saturating_mul(SUBUNITS_PER_GRID),
                },
                StyleKey::Symbol,
                subject,
            );
            return;
        }
        ShapeKind::Qr {
            position,
            size,
            text,
        } => {
            draw_qr(scene, shape, *position, *size, text);
            return;
        }
        _ => {}
    }
    for path in shape.kind.paths(SUBUNITS_PER_GRID) {
        if path.len() >= 2 {
            scene.push(
                Shape::Polyline(path.into_iter().map(point).collect()),
                StyleKey::Symbol,
                subject.clone(),
            );
        }
    }
    if let ShapeKind::Cloud {
        from,
        to,
        revision: Some(revision),
    } = &shape.kind
    {
        // A triangle at the top-right corner, with the revision mark in it.
        let corner = grid(GridPoint::new(from.x.max(to.x), from.y.min(to.y)));
        let side = SUBUNITS_PER_GRID.saturating_mul(3);
        let at = |dx: i64, dy: i64| {
            ScenePoint::new(corner.x.saturating_add(dx), corner.y.saturating_add(dy))
        };
        scene.push(
            Shape::Polyline(vec![
                at(0, side.saturating_neg()),
                at(side, side.saturating_neg()),
                at(side / 2, 0),
                at(0, side.saturating_neg()),
            ]),
            StyleKey::Symbol,
            subject.clone(),
        );
        let text: String = revision
            .chars()
            .take(circuit_model::annotations::MAX_REVISION_CHARS)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        scene.push(
            Shape::Text {
                at: at(side / 2, side.saturating_neg().saturating_mul(2) / 3),
                text,
                anchor: Anchor::Center,
            },
            StyleKey::Label,
            subject,
        );
    }
}

/// A QR code: one filled block for each dark module, scaled so that the
/// code with its quiet zone of four modules fills `size` grid units. A text
/// too long to encode leaves the border only.
fn draw_qr(scene: &mut Scene, shape: &DocShape, position: GridPoint, size: u32, text: &str) {
    let subject = Some(Subject::Shape(shape.id));
    let (low, high) = circuit_model::annotations::QR_SIDE;
    let side = i64::from(size.clamp(low, high)).saturating_mul(SUBUNITS_PER_GRID);
    let origin = grid(position);
    let border = SceneRect::from_corners(
        origin,
        ScenePoint::new(origin.x.saturating_add(side), origin.y.saturating_add(side)),
    );
    let Ok(code) = QrCode::encode_text(text, QrCodeEcc::Medium) else {
        scene.push(Shape::Rect(border), StyleKey::Symbol, subject);
        return;
    };
    let modules = i64::from(code.size());
    let span = modules.saturating_add(8);
    // Positions in hundredths of a subunit, so that the modules tile the
    // square exactly; each block is rounded to whole subunits.
    let edge = |index: i64| {
        side.saturating_mul(index.saturating_add(4))
            .checked_div(span)
            .unwrap_or(0)
    };
    for y in 0..code.size() {
        for x in 0..code.size() {
            if !code.get_module(x, y) {
                continue;
            }
            let (x, y) = (i64::from(x), i64::from(y));
            let min = ScenePoint::new(
                origin.x.saturating_add(edge(x)),
                origin.y.saturating_add(edge(y)),
            );
            let max = ScenePoint::new(
                origin.x.saturating_add(edge(x.saturating_add(1))),
                origin.y.saturating_add(edge(y.saturating_add(1))),
            );
            scene.push(
                Shape::Block(SceneRect::from_corners(min, max)),
                StyleKey::Symbol,
                subject.clone(),
            );
        }
    }
}

/// Where the first symbol of a device is on the schematic, as `/2/4B`;
/// empty when it has none on a sheet.
fn schematic_place(doc: &Schematic, device: circuit_model::DeviceId) -> String {
    let mut symbols: Vec<&circuit_model::SymbolInstance> = doc
        .symbols
        .iter()
        .filter(|symbol| symbol.device_id == device)
        .collect();
    symbols.sort_by_key(|symbol| symbol.id);
    symbols
        .into_iter()
        .find_map(|symbol| {
            let sheet = sheet_at(doc, symbol.position)?;
            let zone = sheet.zone_of(symbol.position)?;
            (!sheet.kind.shows_views()).then(|| format!("/{}/{zone}", sheet.number))
        })
        .unwrap_or_default()
}

/// A device view: a rectangle with the designator and the place of the
/// device on the schematic; on a single-line sheet, a glyph by the category
/// of the device's model instead of the rectangle.
fn draw_view(scene: &mut Scene, doc: &Schematic, catalog: &ComponentCatalog, view: &DeviceView) {
    let subject = Some(Subject::View(view.id));
    let Some(area) = view.bounds() else {
        return;
    };
    let Some(device) = doc.devices.iter().find(|d| d.id == view.device_id) else {
        return;
    };
    let rect = SceneRect::from_corners(grid(area.min), grid(area.max));
    let single_line =
        sheet_at(doc, view.position).is_some_and(|sheet| sheet.kind == SheetKind::SingleLine);
    if single_line {
        let model = catalog.model(&device.model);
        draw_glyph(
            scene,
            rect,
            model.map(circuit_model::ComponentModel::category),
            model.map(circuit_model::ComponentModel::class),
            &subject,
        );
    } else {
        scene.push(Shape::Rect(rect), StyleKey::Symbol, subject.clone());
    }
    let center_x = rect.min.x.saturating_add(rect.max.x) / 2;
    let middle_y = rect.min.y.saturating_add(rect.max.y) / 2;
    let (label_y, place_y) = (middle_y, middle_y.saturating_add(SUBUNITS_PER_GRID));
    // Beside a single-line glyph, to its right; inside a footprint.
    let (label_x, anchor) = if single_line {
        (
            rect.max.x.saturating_add(SUBUNITS_PER_GRID / 2),
            Anchor::Left,
        )
    } else {
        (center_x, Anchor::Center)
    };
    scene.push(
        Shape::Text {
            at: ScenePoint::new(label_x, label_y),
            text: device.reference.0.clone(),
            anchor,
        },
        StyleKey::Label,
        subject.clone(),
    );
    let place = schematic_place(doc, device.id);
    if !place.is_empty() {
        scene.push(
            Shape::Text {
                at: ScenePoint::new(label_x, place_y),
                text: place,
                anchor,
            },
            StyleKey::Label,
            subject,
        );
    }
}

/// A single-line glyph in `rect`: a circle for a motor, two circles for a
/// transformer, a square with a cross for protection, a square with a
/// diagonal for a switching device, and a plain square for anything else.
fn draw_glyph(
    scene: &mut Scene,
    rect: SceneRect,
    category: Option<Category>,
    class: Option<DeviceClass>,
    subject: &Option<Subject>,
) {
    let width = rect.max.x.saturating_sub(rect.min.x);
    let height = rect.max.y.saturating_sub(rect.min.y);
    let center = ScenePoint::new(
        rect.min.x.saturating_add(width / 2),
        rect.min.y.saturating_add(height / 2),
    );

    let radius = width.min(height) / 2;
    let push = |scene: &mut Scene, shape: Shape| {
        scene.push(shape, StyleKey::Symbol, subject.clone());
    };
    match (category, class) {
        (_, Some(DeviceClass::Transformer)) => {
            let small = radius.saturating_mul(3) / 5;
            for dy in [small.saturating_neg() / 2, small / 2] {
                push(
                    scene,
                    Shape::Circle {
                        center: ScenePoint::new(center.x, center.y.saturating_add(dy)),
                        radius: small,
                    },
                );
            }
        }
        (Some(Category::Motors), _) => {
            push(scene, Shape::Circle { center, radius });
            scene.push(
                Shape::Text {
                    at: center,
                    text: "M".to_owned(),
                    anchor: Anchor::Center,
                },
                StyleKey::Label,
                subject.clone(),
            );
        }
        (Some(Category::Protection), _) => {
            push(scene, Shape::Rect(rect));
            push(
                scene,
                Shape::Line {
                    from: rect.min,
                    to: rect.max,
                },
            );
            push(
                scene,
                Shape::Line {
                    from: ScenePoint::new(rect.max.x, rect.min.y),
                    to: ScenePoint::new(rect.min.x, rect.max.y),
                },
            );
        }
        (_, Some(DeviceClass::Contactor | DeviceClass::Relay)) => {
            push(scene, Shape::Rect(rect));
            push(
                scene,
                Shape::Line {
                    from: ScenePoint::new(rect.min.x, rect.max.y),
                    to: ScenePoint::new(rect.max.x, rect.min.y),
                },
            );
        }
        _ => push(scene, Shape::Rect(rect)),
    }
}

/// The box of a PLC module placed on the drawing: its name and description,
/// then one row per channel with the address, the direction, the device and
/// the label.
fn draw_plc(scene: &mut Scene, doc: &Schematic, plc: &PlcModule) {
    let Some(position) = plc.position else {
        return;
    };
    let subject = Some(Subject::Plc(plc.id));
    let mut rows: Vec<String> = Vec::with_capacity(plc.channels.len().saturating_add(1));
    let title = match &plc.description {
        Some(description) => format!("{} {description}", plc.name),
        None => plc.name.clone(),
    };
    rows.push(title);
    for channel in plc
        .channels
        .iter()
        .take(circuit_model::annotations::MAX_PLC_CHANNELS)
    {
        let device = channel
            .device
            .and_then(|id| doc.devices.iter().find(|d| d.id == id))
            .map(|device| device.reference.0.as_str())
            .unwrap_or("");
        let arrow = match channel.direction {
            circuit_model::PlcDirection::Input => "<",
            circuit_model::PlcDirection::Output => ">",
        };
        let label = channel.label.as_deref().unwrap_or("");
        rows.push(
            format!("{} {arrow} {device} {label}", channel.address)
                .trim_end()
                .to_owned(),
        );
    }
    let clean = |text: &str| -> String {
        text.chars()
            .take(48)
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    };
    let widest = rows
        .iter()
        .map(|row| row.chars().count().min(48))
        .max()
        .unwrap_or(1);
    let row_height = SUBUNITS_PER_GRID.saturating_mul(2);
    // About half a grid unit for each character, as the text notes take.
    let width = i64::try_from(widest)
        .unwrap_or(48)
        .saturating_mul(SUBUNITS_PER_GRID / 2)
        .saturating_add(SUBUNITS_PER_GRID.saturating_mul(2));
    let height = i64::try_from(rows.len())
        .unwrap_or(1)
        .saturating_mul(row_height);
    let origin = grid(position);
    let corner = ScenePoint::new(
        origin.x.saturating_add(width),
        origin.y.saturating_add(height),
    );
    scene.push(
        Shape::Rect(SceneRect::from_corners(origin, corner)),
        StyleKey::Symbol,
        subject.clone(),
    );
    for (index, row) in rows.iter().enumerate() {
        let index = i64::try_from(index).unwrap_or(0);
        let top = origin.y.saturating_add(row_height.saturating_mul(index));
        if index > 0 {
            scene.push(
                Shape::Line {
                    from: ScenePoint::new(origin.x, top),
                    to: ScenePoint::new(corner.x, top),
                },
                StyleKey::Symbol,
                subject.clone(),
            );
        }
        scene.push(
            Shape::Text {
                at: ScenePoint::new(
                    origin.x.saturating_add(SUBUNITS_PER_GRID),
                    top.saturating_add(row_height / 2),
                ),
                text: clean(row),
                anchor: Anchor::Left,
            },
            StyleKey::Label,
            subject.clone(),
        );
    }
}

/// The area of a documentation subject on the sheet, for an outline: a
/// shape, a view, or the box of a placed PLC module.
pub(crate) fn area_of(doc: &Schematic, subject: &Subject) -> Option<SceneRect> {
    let rect =
        |area: circuit_model::GridRect| SceneRect::from_corners(grid(area.min), grid(area.max));
    match subject {
        Subject::Shape(id) => doc
            .shapes
            .iter()
            .find(|shape| shape.id == *id)
            .and_then(|shape| shape.kind.bounds())
            .map(rect),
        Subject::View(id) => doc
            .views
            .iter()
            .find(|view| view.id == *id)
            .and_then(DeviceView::bounds)
            .map(rect),
        _ => None,
    }
}
