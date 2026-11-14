//! Schema migrations.
//!
//! A migration is a pure function from the JSON of one schema version to the
//! JSON of the next. Migrations are applied in order until the document
//! reaches the current version. Version 1 is the first schema, so the table
//! is empty; the driver exists so that the first schema change only adds an
//! entry and a pair of fixtures.

use circuit_model::CURRENT_SCHEMA_VERSION;
use serde_json::Value;

use crate::ProjectError;

/// One step: the version it reads and the function that produces the next.
pub(crate) type Migration = (u32, fn(Value) -> Result<Value, String>);

/// Migrations of the shipped schema versions, in ascending order.
const MIGRATIONS: &[Migration] = &[];

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
    fn shipped_migration_table_reaches_the_current_version() {
        assert_eq!(
            migrate(json!({ "schema_version": CURRENT_SCHEMA_VERSION })),
            Ok(json!({ "schema_version": CURRENT_SCHEMA_VERSION }))
        );
    }
}
