//! Project templates (M9.2, issue #118): example projects built into the
//! program, opened from File > New from template as new projects that have
//! never been saved. They are the generated examples of the repository, so a
//! template always loads and compiles.

/// A template: the name the menu shows (in English, translated by the
/// window) and the text of the project.
#[derive(Clone, Copy, Debug)]
pub struct Template {
    /// The name in the menu.
    pub name: &'static str,
    /// The project.
    pub text: &'static str,
}

/// Every template, in the order of the menu.
pub const TEMPLATES: [Template; 4] = [
    Template {
        name: "Direct-online starter",
        text: include_str!("../../../examples/direct-starter/starter.ocircuit"),
    },
    Template {
        name: "Reversing starter with interlocking",
        text: include_str!("../../../examples/forward-reverse/interlocked.ocircuit"),
    },
    Template {
        name: "Star-delta starter",
        text: include_str!("../../../examples/star-delta/star-delta.ocircuit"),
    },
    Template {
        name: "Starter on two sheets with title block",
        text: include_str!("../../../examples/documentation/documentation.ocircuit"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Session;

    #[test]
    fn every_template_loads_and_compiles_without_errors() {
        for template in TEMPLATES {
            let session = Session::load(template.text, None).unwrap();
            assert!(
                session
                    .diagnostics()
                    .iter()
                    .all(|d| d.severity() != circuit_model::Severity::Error),
                "{}",
                template.name
            );
        }
    }
}
