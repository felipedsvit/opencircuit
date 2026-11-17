//! Tab 3: commands and actuation.

use circuit_model::catalog::{Category, ComponentModel, DeviceClass, SymbolKind};

use super::{initial_closed, load_params, model, plain};

/// A contactor with `poles` main poles, a coil and auxiliary contacts.
fn contactor(poles: u8, name: &str, title: &str) -> ComponentModel {
    model(
        name,
        DeviceClass::Contactor,
        Category::Controls,
        title,
        load_params(),
        vec![
            plain("coil", SymbolKind::Coil, 1, 1, false),
            plain("main", SymbolKind::PowerContacts { poles }, 0, 1, false),
            plain("aux_no", SymbolKind::ContactNo, 0, 8, true),
            plain("aux_nc", SymbolKind::ContactNc, 0, 8, true),
        ],
    )
}

pub(super) fn models() -> Vec<ComponentModel> {
    vec![
        contactor(1, "contactor_1p", "Contactor, 1-pole"),
        contactor(2, "contactor_2p", "Contactor, 2-pole"),
        contactor(4, "contactor_4p", "Contactor, 4-pole"),
        model(
            "selector_2pos",
            DeviceClass::LatchingSwitch,
            Category::Controls,
            "Selector switch, two positions (I-II)",
            vec![initial_closed()],
            vec![
                plain("no", SymbolKind::ContactNo, 0, 1, false),
                plain("nc", SymbolKind::ContactNc, 0, 1, false),
            ],
        )
        .with_note("Position II closes the normally open contact and opens the normally closed one. Selectors with a middle position are not simulated."),
        model(
            "phase_monitor_relay",
            DeviceClass::Relay,
            Category::Controls,
            "Phase monitoring relay",
            load_params(),
            vec![
                plain("coil", SymbolKind::Coil, 1, 1, false),
                plain("aux_no", SymbolKind::ContactNo, 0, 8, true),
                plain("aux_nc", SymbolKind::ContactNc, 0, 8, true),
                plain("changeover", SymbolKind::Changeover, 0, 8, true),
            ],
        )
        .with_note("Approximation: it acts as a relay on its own coil. Phase loss, phase sequence and voltage are not monitored."),
    ]
}
