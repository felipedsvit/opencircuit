//! Schema migrations.
//!
//! A migration is a pure function from the JSON of one schema version to the
//! JSON of the next. Migrations are applied in order until the document
//! reaches the current version.
//!
//! | From | To | Change |
//! | --- | --- | --- |
//! | 1 | 2 | sheets (ADR-0011) and the title-block fields of `project` |
//! | 2 | 3 | nothing is added: the new fields of ADR-0014 are all optional |
//! | 3 | 4 | nothing is added: `texts` of ADR-0015 is optional |

use circuit_model::CURRENT_SCHEMA_VERSION;
use serde_json::Value;

use crate::ProjectError;

/// One step: the version it reads and the function that produces the next.
pub(crate) type Migration = (u32, fn(Value) -> Result<Value, String>);

/// Migrations of the shipped schema versions, in ascending order.
const MIGRATIONS: &[Migration] = &[(1, v1_to_v2), (2, v2_to_v3), (3, v3_to_v4)];

/// Version 2 adds `sheets` (none: the drawing stays one page sized to its
/// content) and the title-block fields `author`, `company`, `revision` and
/// `date` of `project` (all `null`). A version-1 document that already has
/// one of them is refused rather than overwritten.
fn v1_to_v2(mut value: Value) -> Result<Value, String> {
    let document = value
        .as_object_mut()
        .ok_or("the document is not a JSON object")?;
    if document.contains_key("sheets") {
        return Err("a version 1 document has no `sheets`".to_owned());
    }
    let project = document
        .get_mut("project")
        .and_then(Value::as_object_mut)
        .ok_or("the document has no `project` object")?;
    for field in ["author", "company", "revision", "date"] {
        if project.contains_key(field) {
            return Err(format!("a version 1 `project` has no `{field}`"));
        }
        project.insert(field.to_owned(), Value::Null);
    }
    document.insert("sheets".to_owned(), Value::Array(Vec::new()));
    document.insert("schema_version".to_owned(), Value::from(2_u32));
    Ok(value)
}

/// Version 3 adds `Wire.number`, `Device.info`, `project.wire_numbering` and
/// `waivers` (ADR-0014). Each is optional and omitted when empty, so a
/// version 2 document is already a valid version 3 document and only the
/// version changes. A version 2 document that already has one of them is
/// refused rather than reinterpreted.
fn v2_to_v3(mut value: Value) -> Result<Value, String> {
    let document = value
        .as_object_mut()
        .ok_or("the document is not a JSON object")?;
    if document.contains_key("waivers") {
        return Err("a version 2 document has no `waivers`".to_owned());
    }
    let has = |collection: Option<&Value>, field: &str| {
        collection
            .and_then(Value::as_array)
            .is_some_and(|items| items.iter().any(|item| item.get(field).is_some()))
    };
    if has(document.get("wires"), "number") {
        return Err("a version 2 wire has no `number`".to_owned());
    }
    if has(document.get("devices"), "info") {
        return Err("a version 2 device has no `info`".to_owned());
    }
    if document
        .get("project")
        .is_some_and(|project| project.get("wire_numbering").is_some())
    {
        return Err("a version 2 `project` has no `wire_numbering`".to_owned());
    }
    document.insert("schema_version".to_owned(), Value::from(3_u32));
    Ok(value)
}

/// Version 4 adds `texts` (ADR-0015), which is optional and omitted when
/// empty, so a version 3 document is already a valid version 4 document and
/// only the version changes. A version 3 document that already has `texts` is
/// refused rather than reinterpreted.
fn v3_to_v4(mut value: Value) -> Result<Value, String> {
    let document = value
        .as_object_mut()
        .ok_or("the document is not a JSON object")?;
    if document.contains_key("texts") {
        return Err("a version 3 document has no `texts`".to_owned());
    }
    document.insert("schema_version".to_owned(), Value::from(4_u32));
    Ok(value)
}

/// The `schema_version` of a parsed document. It must be a non-negative
/// integer; a missing field, a string or a floating-point number is refused.
pub(crate) fn schema_version(value: &Value) -> Result<u64, ProjectError> {
    value
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or(ProjectError::MissingSchemaVersion)
}

/// Bring a parsed document to the current schema version.
///
/// # Errors
///
/// Returns [`ProjectError::NewerSchema`] for a document written by a newer
/// build and [`ProjectError::NoMigration`] when no migration path exists.
pub fn migrate(value: Value) -> Result<Value, ProjectError> {
    migrate_with(value, CURRENT_SCHEMA_VERSION, MIGRATIONS)
}

pub(crate) fn migrate_with(
    mut value: Value,
    target: u32,
    migrations: &[Migration],
) -> Result<Value, ProjectError> {
    let mut version = schema_version(&value)?;
    if version > u64::from(target) {
        return Err(ProjectError::NewerSchema {
            found: version,
            supported: target,
        });
    }
    while version < u64::from(target) {
        let (_, step) = migrations
            .iter()
            .find(|(from, _)| u64::from(*from) == version)
            .ok_or(ProjectError::NoMigration { found: version })?;
        value = step(value).map_err(|message| ProjectError::Migration {
            from: version,
            message,
        })?;
        let migrated = schema_version(&value)?;
        if Some(migrated) != version.checked_add(1) {
            return Err(ProjectError::Migration {
                from: version,
                message: format!("the migration left the document at version {migrated}"),
            });
        }
        version = migrated;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn rename_title(mut value: Value) -> Result<Value, String> {
        let object = value.as_object_mut().ok_or("document is not an object")?;
        let title = object.remove("title").ok_or("version 1 has no title")?;
        object.insert("name".to_owned(), title);
        object.insert("schema_version".to_owned(), json!(2));
        Ok(value)
    }

    fn add_grid(mut value: Value) -> Result<Value, String> {
        let object = value.as_object_mut().ok_or("document is not an object")?;
        object.insert("grid_unit".to_owned(), json!(10));
        object.insert("schema_version".to_owned(), json!(3));
        Ok(value)
    }

    fn forgets_to_bump(value: Value) -> Result<Value, String> {
        Ok(value)
    }

    const STEPS: &[Migration] = &[(1, rename_title), (2, add_grid)];

    #[test]
    fn migrations_are_applied_in_order_up_to_the_target() {
        let migrated = migrate_with(json!({ "schema_version": 1, "title": "demo" }), 3, STEPS);

        assert_eq!(
            migrated,
            Ok(json!({ "schema_version": 3, "name": "demo", "grid_unit": 10 }))
        );
    }

    #[test]
    fn document_at_the_target_version_is_returned_unchanged() {
        let current = json!({ "schema_version": 3, "name": "demo", "grid_unit": 10 });

        assert_eq!(migrate_with(current.clone(), 3, STEPS), Ok(current));
    }

    #[test]
    fn migrating_twice_gives_the_same_document() {
        let once = migrate_with(json!({ "schema_version": 1, "title": "demo" }), 3, STEPS);
        let twice = once.clone().and_then(|value| migrate_with(value, 3, STEPS));

        assert_eq!(twice, once);
    }

    #[test]
    fn newer_schema_is_refused_without_reading_further() {
        assert_eq!(
            migrate_with(json!({ "schema_version": 4, "anything": [] }), 3, STEPS),
            Err(ProjectError::NewerSchema {
                found: 4,
                supported: 3
            })
        );
    }

    #[test]
    fn version_without_a_migration_path_is_refused() {
        assert_eq!(
            migrate_with(json!({ "schema_version": 0 }), 3, STEPS),
            Err(ProjectError::NoMigration { found: 0 })
        );
    }

    #[test]
    fn failing_migration_reports_its_source_version() {
        assert_eq!(
            migrate_with(json!({ "schema_version": 1 }), 3, STEPS),
            Err(ProjectError::Migration {
                from: 1,
                message: "version 1 has no title".to_owned()
            })
        );
    }

    #[test]
    fn migration_that_does_not_advance_the_version_is_an_error_not_a_loop() {
        assert_eq!(
            migrate_with(json!({ "schema_version": 1 }), 2, &[(1, forgets_to_bump)]),
            Err(ProjectError::Migration {
                from: 1,
                message: "the migration left the document at version 1".to_owned()
            })
        );
    }

    #[test]
    fn schema_version_must_be_a_non_negative_integer() {
        for value in [
            json!({}),
            json!({ "schema_version": "1" }),
            json!({ "schema_version": 1.0 }),
            json!({ "schema_version": -1 }),
            json!([1]),
        ] {
            assert_eq!(
                schema_version(&value),
                Err(ProjectError::MissingSchemaVersion),
                "{value}"
            );
        }
    }

    #[test]
    fn version_2_only_changes_its_version_to_become_the_current_one() {
        let v2 = json!({
            "schema_version": 2,
            "project": { "name": "demo" },
            "devices": [{ "id": 1 }],
            "wires": [{ "id": 2 }]
        });
        let mut expected = v2.clone();
        expected["schema_version"] = json!(4);

        assert_eq!(migrate(v2), Ok(expected));
    }

    #[test]
    fn version_2_with_a_version_3_field_is_refused() {
        for document in [
            json!({ "schema_version": 2, "waivers": [] }),
            json!({ "schema_version": 2, "wires": [{ "id": 1, "number": "5" }] }),
            json!({ "schema_version": 2, "devices": [{ "id": 1, "info": {} }] }),
            json!({ "schema_version": 2, "project": { "wire_numbering": null } }),
        ] {
            assert!(
                matches!(
                    migrate(document.clone()),
                    Err(ProjectError::Migration { from: 2, .. })
                ),
                "{document}"
            );
        }
    }

    #[test]
    fn version_3_only_changes_its_version_to_become_version_4() {
        let v3 = json!({ "schema_version": 3, "project": { "name": "demo" }, "wires": [] });
        let mut expected = v3.clone();
        expected["schema_version"] = json!(4);

        assert_eq!(migrate(v3), Ok(expected));
    }

    #[test]
    fn version_3_with_texts_is_refused() {
        assert!(matches!(
            migrate(json!({ "schema_version": 3, "texts": [] })),
            Err(ProjectError::Migration { from: 3, .. })
        ));
    }

    #[test]
    fn shipped_migration_table_reaches_the_current_version() {
        assert_eq!(
            migrate(json!({ "schema_version": CURRENT_SCHEMA_VERSION })),
            Ok(json!({ "schema_version": CURRENT_SCHEMA_VERSION }))
        );
    }
}
