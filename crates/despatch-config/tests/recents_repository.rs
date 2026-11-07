use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use despatch_application::{RecentLaunch, RecentLaunches};
use despatch_config::JsonRecentLaunches;
use despatch_domain::LauncherId;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

#[test]
fn the_fixture_reads_back_newest_first() {
    let store = JsonRecentLaunches::new(fixture("recents.json"));

    let recents = store.load().expect("readable fixture");

    assert_eq!(recents.len(), 2);
    assert_eq!(recents[0].used_at, at(1_762_520_000));
    assert!(recents[0].used_at > recents[1].used_at);
}

#[test]
fn a_history_round_trips_through_its_file() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let store = JsonRecentLaunches::beside(&directory.path().join("config.json"));
    let recents = vec![
        RecentLaunch {
            launcher_id: LauncherId::new(),
            used_at: at(2_000),
        },
        RecentLaunch {
            launcher_id: LauncherId::new(),
            used_at: at(1_000),
        },
    ];

    store.save(&recents).expect("writable directory");

    assert_eq!(store.path(), directory.path().join("recents.json"));
    assert_eq!(store.load().expect("readable history"), recents);
}

#[test]
fn a_missing_history_is_empty() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let store = JsonRecentLaunches::new(directory.path().join("recents.json"));

    assert!(store.load().expect("missing is not an error").is_empty());
}

#[test]
fn a_damaged_history_is_empty_and_never_touches_the_config_beside_it() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let config = directory.path().join("config.json");
    std::fs::write(&config, b"{\"untouched\": true}").expect("write config");
    std::fs::write(directory.path().join("recents.json"), b"{ not json").expect("write");
    let store = JsonRecentLaunches::beside(&config);

    assert!(store.load().expect("damage is not fatal").is_empty());
    assert_eq!(
        std::fs::read(&config).expect("config still readable"),
        b"{\"untouched\": true}"
    );
}

#[test]
fn a_history_from_a_future_format_is_ignored_rather_than_misread() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let path = directory.path().join("recents.json");
    std::fs::write(&path, b"{\"version\": 9, \"recent\": []}").expect("write");

    assert!(
        JsonRecentLaunches::new(path)
            .load()
            .expect("not fatal")
            .is_empty()
    );
}
