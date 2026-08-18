use despatch_domain::SubgroupId;
use serde_json::{Map, Value, json};
use thiserror::Error;

/// The subgroup a v1 group's launchers are collected into, since v1 had no categories.
pub const DEFAULT_SUBGROUP_NAME: &str = "General";

/// The extension appended to the recoverable copy kept before a schema migration.
pub const BACKUP_EXTENSION: &str = "v1.bak";

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MigrationError {
    #[error("a schema {found} config is not a JSON object")]
    NotAnObject { found: u32 },
    #[error("a schema 1 config must list its groups as an array")]
    GroupsAreNotAnArray,
    #[error("a schema 1 group must be an object with an 'items' array")]
    MalformedGroup,
}

/// Rewrites a schema 1 document into the schema 2 shape.
///
/// Schema 1 hung launchers directly off a group. Schema 2 keeps every launcher in a subgroup, so
/// each group's `items` move into one subgroup named [`DEFAULT_SUBGROUP_NAME`]. Identifiers,
/// ordering, and launcher payloads are preserved untouched.
pub fn migrate_v1_to_v2(document: &mut Value) -> Result<(), MigrationError> {
    let root = document
        .as_object_mut()
        .ok_or(MigrationError::NotAnObject { found: 1 })?;
    let groups = root
        .get_mut("groups")
        .and_then(Value::as_array_mut)
        .ok_or(MigrationError::GroupsAreNotAnArray)?;

    for group in groups.iter_mut() {
        let group = group
            .as_object_mut()
            .ok_or(MigrationError::MalformedGroup)?;
        let items = group.remove("items").unwrap_or_else(|| json!([]));
        let items = items
            .as_array()
            .ok_or(MigrationError::MalformedGroup)?
            .clone();
        group.insert("subgroups".to_owned(), subgroups_for(items));
    }

    root.insert("schema_version".to_owned(), json!(2));
    Ok(())
}

/// An empty group stays empty; anything else keeps its launchers in one default subgroup.
fn subgroups_for(items: Vec<Value>) -> Value {
    if items.is_empty() {
        return json!([]);
    }
    let mut subgroup = Map::new();
    subgroup.insert(
        "id".to_owned(),
        serde_json::to_value(SubgroupId::new()).unwrap_or(Value::Null),
    );
    subgroup.insert("name".to_owned(), json!(DEFAULT_SUBGROUP_NAME));
    subgroup.insert("items".to_owned(), Value::Array(items));
    Value::Array(vec![Value::Object(subgroup)])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DEFAULT_SUBGROUP_NAME, MigrationError, migrate_v1_to_v2};

    #[test]
    fn every_launcher_of_a_v1_group_moves_into_one_default_subgroup() {
        let mut document = json!({
            "schema_version": 1,
            "revision": 4,
            "groups": [{
                "id": "018f46f2-bde0-7cc8-a875-6cbe1a29a227",
                "name": "Production",
                "section": "servers",
                "items": [
                    { "type": "ssh", "id": "018f46f4-1a77-7df2-a70a-85ad1c7cb842", "name": "API",
                      "host": "api.example.com", "user": "deploy", "port": 22, "extra_args": [] }
                ]
            }]
        });

        migrate_v1_to_v2(&mut document).expect("migrate");

        assert_eq!(document["schema_version"], 2);
        assert_eq!(document["revision"], 4, "a migration is not an edit");
        let group = &document["groups"][0];
        assert!(group.get("items").is_none(), "v1 items must not linger");
        let subgroups = group["subgroups"].as_array().expect("subgroups array");
        assert_eq!(subgroups.len(), 1);
        assert_eq!(subgroups[0]["name"], DEFAULT_SUBGROUP_NAME);
        assert_eq!(subgroups[0]["items"][0]["name"], "API");
        assert_eq!(
            subgroups[0]["items"][0]["id"], "018f46f4-1a77-7df2-a70a-85ad1c7cb842",
            "launcher identifiers survive the migration"
        );
        assert!(subgroups[0]["id"].is_string(), "the subgroup gets an id");
    }

    #[test]
    fn an_empty_v1_group_migrates_to_no_subgroups_at_all() {
        let mut document = json!({
            "schema_version": 1,
            "revision": 0,
            "groups": [{
                "id": "018f46f2-bde0-7cc8-a875-6cbe1a29a227",
                "name": "Production",
                "section": "servers",
                "items": []
            }]
        });

        migrate_v1_to_v2(&mut document).expect("migrate");

        assert_eq!(document["groups"][0]["subgroups"].as_array(), Some(&vec![]));
    }

    #[test]
    fn two_migrated_groups_never_share_a_subgroup_identifier() {
        let group = |name: &str| {
            json!({
                "id": "018f46f2-bde0-7cc8-a875-6cbe1a29a227",
                "name": name,
                "section": "servers",
                "items": [{ "type": "ssh", "id": "018f46f4-1a77-7df2-a70a-85ad1c7cb842",
                            "name": "API", "host": "api.example.com", "user": "deploy",
                            "port": 22, "extra_args": [] }]
            })
        };
        let mut document = json!({
            "schema_version": 1,
            "revision": 0,
            "groups": [group("Production"), group("Staging")],
        });

        migrate_v1_to_v2(&mut document).expect("migrate");

        assert_ne!(
            document["groups"][0]["subgroups"][0]["id"],
            document["groups"][1]["subgroups"][0]["id"]
        );
    }

    #[test]
    fn a_malformed_v1_document_is_reported_rather_than_guessed_at() {
        let mut not_an_object = json!([]);
        assert_eq!(
            migrate_v1_to_v2(&mut not_an_object),
            Err(MigrationError::NotAnObject { found: 1 })
        );

        let mut groups_not_an_array = json!({ "schema_version": 1, "groups": {} });
        assert_eq!(
            migrate_v1_to_v2(&mut groups_not_an_array),
            Err(MigrationError::GroupsAreNotAnArray)
        );

        let mut group_not_an_object = json!({ "schema_version": 1, "groups": ["Production"] });
        assert_eq!(
            migrate_v1_to_v2(&mut group_not_an_object),
            Err(MigrationError::MalformedGroup)
        );
    }
}
