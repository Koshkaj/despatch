use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use despatch_application::{
    ConfigRepository, RepositorySaveExpectation as ApplicationSaveExpectation,
};
use despatch_config::{BACKUP_EXTENSION, DEFAULT_SUBGROUP_NAME};
use despatch_config::{
    ConfigError, JsonConfigRepository, SaveExpectation, config_expansion_context,
};
use despatch_domain::{AppConfig, CURRENT_SCHEMA_VERSION, PathTemplate, ValidationError};
use tempfile::tempdir;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

#[test]
fn the_current_fixture_round_trips_byte_stably_after_canonical_formatting() {
    let path = fixture("config-v2.json");
    let original = fs::read(&path).expect("read fixture");
    let loaded = JsonConfigRepository::load(&path).expect("load fixture");
    let encoded = JsonConfigRepository::encode(&loaded.config).expect("encode fixture");

    assert!(!loaded.migrated, "the current schema needs no migration");
    assert_eq!(encoded, original);
    let decoded_again: AppConfig = serde_json::from_slice(&encoded).expect("decode encoded config");
    assert_eq!(decoded_again, loaded.config);
}

#[test]
fn fingerprinting_a_config_reads_it_without_decoding_or_backing_it_up() {
    let directory = tempdir().expect("temp dir");
    let path = directory.path().join("config.json");
    fs::write(
        &path,
        fs::read(fixture("config-v1.json")).expect("read fixture"),
    )
    .expect("seed a legacy config");

    let repository = JsonConfigRepository;
    let fingerprint = ConfigRepository::fingerprint(&repository, &path).expect("fingerprint");

    assert!(
        !directory
            .path()
            .join(format!("config.json.{BACKUP_EXTENSION}"))
            .exists(),
        "a check every few seconds must not touch the backup"
    );
    assert_eq!(
        fingerprint,
        ConfigRepository::load(&repository, &path)
            .expect("load")
            .fingerprint,
        "the check and a load agree on what the file is"
    );
}

#[test]
fn a_v1_config_is_migrated_into_one_subgroup_per_group_and_backed_up() {
    let directory = tempdir().expect("temp dir");
    let path = directory.path().join("config.json");
    let original = fs::read(fixture("config-v1.json")).expect("read fixture");
    fs::write(&path, &original).expect("seed a legacy config");

    let loaded = JsonConfigRepository::load(&path).expect("load legacy config");

    assert!(loaded.migrated);
    assert_eq!(loaded.config.schema_version, CURRENT_SCHEMA_VERSION);
    assert_eq!(loaded.config.revision, 4, "a migration is not an edit");
    let production = &loaded.config.groups[0];
    assert_eq!(production.name, "Production");
    assert_eq!(production.subgroups.len(), 1);
    assert_eq!(production.subgroups[0].name, DEFAULT_SUBGROUP_NAME);
    assert_eq!(production.subgroups[0].items[0].name(), "API server");

    let backup = directory
        .path()
        .join(format!("config.json.{BACKUP_EXTENSION}"));
    assert_eq!(
        fs::read(&backup).expect("read backup"),
        original,
        "the pre-migration file stays recoverable"
    );
    assert_eq!(
        fs::read(&path).expect("read source"),
        original,
        "loading never rewrites the source file"
    );
}

#[test]
fn an_existing_migration_backup_is_never_overwritten() {
    let directory = tempdir().expect("temp dir");
    let path = directory.path().join("config.json");
    let backup = directory
        .path()
        .join(format!("config.json.{BACKUP_EXTENSION}"));
    fs::write(
        &path,
        fs::read(fixture("config-v1.json")).expect("read fixture"),
    )
    .expect("seed a legacy config");
    fs::write(&backup, b"the original hand-written config").expect("seed an older backup");

    JsonConfigRepository::load(&path).expect("load legacy config");

    assert_eq!(
        fs::read(&backup).expect("read backup"),
        b"the original hand-written config"
    );
}

#[test]
fn a_future_schema_version_is_refused_without_touching_the_file() {
    let directory = tempdir().expect("temp dir");
    let path = directory.path().join("config.json");
    let contents = br#"{ "schema_version": 99, "revision": 1, "groups": [] }"#;
    fs::write(&path, contents).expect("seed a future config");

    let error = JsonConfigRepository::load(&path).expect_err("a future schema must be refused");

    assert!(matches!(error, ConfigError::FutureSchemaVersion { .. }));
    assert_eq!(fs::read(&path).expect("read source"), contents);
}

#[test]
fn pretty_output_is_deterministic() {
    let loaded = JsonConfigRepository::load(fixture("config-v2.json")).expect("load fixture");
    let first = JsonConfigRepository::encode(&loaded.config).expect("first encode");
    let second = JsonConfigRepository::encode(&loaded.config).expect("second encode");

    assert_eq!(first, second);
    assert!(first.ends_with(b"\n"));
}

#[test]
fn future_schema_is_rejected_without_touching_the_file() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("config.json");
    let contents = b"{\"schema_version\":99,\"revision\":1,\"groups\":[]}";
    fs::write(&path, contents).expect("write config");

    assert!(matches!(
        JsonConfigRepository::load(&path),
        Err(ConfigError::FutureSchemaVersion { found: 99, .. })
    ));
    assert_eq!(fs::read(path).expect("read unchanged file"), contents);
}

#[test]
fn invalid_json_is_rejected_without_touching_the_file() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("config.json");
    let contents = b"{ definitely not JSON";
    fs::write(&path, contents).expect("write config");

    assert!(matches!(
        JsonConfigRepository::load(&path),
        Err(ConfigError::Parse { .. })
    ));
    assert_eq!(fs::read(path).expect("read unchanged file"), contents);
}

#[test]
fn semantic_validation_identifies_the_offending_rule() {
    let error = JsonConfigRepository::load(fixture("config-invalid.json"))
        .expect_err("fixture must fail validation");

    assert!(matches!(
        error,
        ConfigError::Validation {
            source: ValidationError::DuplicateGroupName { .. },
            ..
        }
    ));
}

#[test]
fn atomic_save_creates_a_private_file_and_loads_it_back() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("nested/config.json");
    let config = AppConfig::default();

    let fingerprint = JsonConfigRepository::save(&path, &config, SaveExpectation::NewFile)
        .expect("save new config");
    let loaded = JsonConfigRepository::load(&path).expect("load saved config");

    assert_eq!(loaded.config, config);
    assert_eq!(loaded.fingerprint, fingerprint);
    assert_eq!(
        fs::metadata(path)
            .expect("config metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn existing_file_cannot_be_replaced_as_new() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("config.json");
    fs::write(&path, b"keep me").expect("write existing file");

    assert!(matches!(
        JsonConfigRepository::save(&path, &AppConfig::default(), SaveExpectation::NewFile),
        Err(ConfigError::AlreadyExists { .. })
    ));
    assert_eq!(fs::read(path).expect("read existing file"), b"keep me");
}

#[test]
fn stale_fingerprint_refuses_to_overwrite_external_edit() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("config.json");
    JsonConfigRepository::save(&path, &AppConfig::default(), SaveExpectation::NewFile)
        .expect("save initial config");
    let loaded = JsonConfigRepository::load(&path).expect("load initial config");

    let mut external = loaded.config.clone();
    external.revision = 1;
    fs::write(
        &path,
        JsonConfigRepository::encode(&external).expect("encode external edit"),
    )
    .expect("write external edit");

    let mut local = loaded.config;
    local.revision = 1;
    assert!(matches!(
        JsonConfigRepository::save(&path, &local, SaveExpectation::Matches(loaded.fingerprint)),
        Err(ConfigError::Conflict { .. })
    ));
    assert_eq!(
        JsonConfigRepository::load(path)
            .expect("load external edit")
            .config,
        external
    );
}

#[test]
fn replacement_requires_an_advanced_revision() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("config.json");
    JsonConfigRepository::save(&path, &AppConfig::default(), SaveExpectation::NewFile)
        .expect("save initial config");
    let loaded = JsonConfigRepository::load(&path).expect("load initial config");

    assert!(matches!(
        JsonConfigRepository::save(
            &path,
            &loaded.config,
            SaveExpectation::Matches(loaded.fingerprint)
        ),
        Err(ConfigError::RevisionNotAdvanced {
            current_revision: 0,
            attempted_revision: 0
        })
    ));
}

#[test]
fn relative_paths_use_the_selected_configs_directory() {
    let config_path = Path::new("/shared/despatch/config.json");
    let context = config_expansion_context(config_path).with_variable("HOME", "/Users/alice");

    assert_eq!(
        PathTemplate::new("../project")
            .expect("valid path")
            .expand(&context)
            .expect("expand relative path"),
        PathBuf::from("/shared/despatch/../project")
    );
    assert_eq!(
        PathTemplate::new("$HOME/.ssh/id_ed25519")
            .expect("valid path")
            .expand(&context)
            .expect("expand absolute path"),
        PathBuf::from("/Users/alice/.ssh/id_ed25519")
    );
}

#[test]
fn application_port_round_trips_and_conflict_checks_the_concrete_repository() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("config.json");
    let repository = JsonConfigRepository;
    let config = AppConfig::default();
    let first_fingerprint = ConfigRepository::save(
        &repository,
        &path,
        &config,
        ApplicationSaveExpectation::NewFile,
    )
    .expect("save through application port");
    let loaded = ConfigRepository::load(&repository, &path).expect("load through application port");
    assert_eq!(loaded.config, config);
    assert_eq!(loaded.fingerprint, first_fingerprint);

    let mut edited = loaded.config;
    edited.revision = 1;
    let second_fingerprint = ConfigRepository::save(
        &repository,
        &path,
        &edited,
        ApplicationSaveExpectation::Matches(loaded.fingerprint),
    )
    .expect("replace through application port");
    assert_ne!(second_fingerprint, first_fingerprint);
}
