use super::*;
use crate::app::ViewTab;
use std::sync::atomic::{AtomicU64, Ordering};

fn unique_tmp_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("flog_session_test_{}_{}_{}.toml", pid, nanos, n))
}

// ── round-trip: empty / default App ───────────────────────────────

#[test]
fn roundtrip_empty_session() {
    let app = App::new();
    let data = session_data_from_app(&app);
    let s = toml::to_string_pretty(&data).unwrap();
    let back: SessionData = toml::from_str(&s).unwrap();

    assert_eq!(back.min_level, 0); // System
    assert_eq!(back.tag_filter_input, "");
    assert_eq!(back.search_query, "");
    assert_eq!(back.exclude_query, "");
    assert!(back.bookmarks.is_empty());
    assert_eq!(back.active_tab, 0);

    let mut app2 = App::new();
    apply_session_data(&mut app2, back);
    assert_eq!(app2.filter.min_level, LogLevel::System);
    assert!(app2.filter.tag_include.is_empty());
    assert!(app2.filter.tag_exclude.is_empty());
    assert!(app2.filter.search_query.is_empty());
    assert!(app2.filter.exclude_query.is_empty());
    assert!(app2.bookmarks.is_empty());
    assert_eq!(app2.active_tab, ViewTab::Logs);
}

// ── round-trip: active_tab = Network ──────────────────────────────

#[test]
fn roundtrip_active_tab_network() {
    let mut app = App::new();
    app.active_tab = ViewTab::Network;
    let data = session_data_from_app(&app);
    assert_eq!(data.active_tab, 1);

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert_eq!(app2.active_tab, ViewTab::Network);
}

#[test]
fn apply_unknown_active_tab_falls_back_to_logs() {
    let mut app = App::new();
    app.active_tab = ViewTab::Network;
    let data = SessionData {
        active_tab: 99,
        ..SessionData::default()
    };
    apply_session_data(&mut app, data);
    // 99 → Logs (fallback)
    assert_eq!(app.active_tab, ViewTab::Logs);
}

// ── round-trip: level filter ──────────────────────────────────────

#[test]
fn roundtrip_level_filter_warning() {
    let mut app = App::new();
    app.filter.min_level = LogLevel::Warning;
    let data = session_data_from_app(&app);
    assert_eq!(data.min_level, 4);

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert_eq!(app2.filter.min_level, LogLevel::Warning);
}

// ── DOM-021: magic u8 level mapping, every variant ────────────────

#[test]
fn dom_021_every_level_variant_roundtrips() {
    for (variant, expected_u8) in [
        (LogLevel::System, 0u8),
        (LogLevel::Verbose, 1),
        (LogLevel::Debug, 2),
        (LogLevel::Info, 3),
        (LogLevel::Warning, 4),
        (LogLevel::Error, 5),
    ] {
        let mut app = App::new();
        app.filter.min_level = variant;
        let data = session_data_from_app(&app);
        assert_eq!(data.min_level, expected_u8, "save {:?}", variant);

        let mut app2 = App::new();
        apply_session_data(&mut app2, data);
        assert_eq!(app2.filter.min_level, variant, "load {:?}", variant);
    }
}

#[test]
fn dom_021_out_of_range_level_u8_maps_to_system() {
    for bad in [6u8, 7, 50, 255] {
        let mut app = App::new();
        let data = SessionData {
            min_level: bad,
            ..SessionData::default()
        };
        apply_session_data(&mut app, data);
        assert_eq!(
            app.filter.min_level,
            LogLevel::System,
            "out-of-range {} falls back to System",
            bad,
        );
    }
}

// ── DOM-022: fragile tag_include/exclude reconstruction ───────────

#[test]
fn dom_022_tag_include_only() {
    let mut app = App::new();
    app.filter.parse_tag_filter("network");
    let data = session_data_from_app(&app);
    assert_eq!(data.tag_filter_input, "network");

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert_eq!(app2.filter.tag_include, vec!["network".to_string()]);
    assert!(app2.filter.tag_exclude.is_empty());
}

#[test]
fn dom_022_tag_exclude_only() {
    let mut app = App::new();
    app.filter.parse_tag_filter("-noise");
    let data = session_data_from_app(&app);
    // saved form always uses leading `-` for excludes
    assert_eq!(data.tag_filter_input, "-noise");

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert!(app2.filter.tag_include.is_empty());
    assert_eq!(app2.filter.tag_exclude, vec!["noise".to_string()]);
}

#[test]
fn dom_022_mixed_include_exclude() {
    let mut app = App::new();
    app.filter.parse_tag_filter("alpha,beta,-drop,-noise");
    let data = session_data_from_app(&app);
    // include first (in order), then exclude with `-` prefix
    assert_eq!(data.tag_filter_input, "alpha,beta,-drop,-noise");

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert_eq!(
        app2.filter.tag_include,
        vec!["alpha".to_string(), "beta".to_string()]
    );
    assert_eq!(
        app2.filter.tag_exclude,
        vec!["drop".to_string(), "noise".to_string()]
    );
}

#[test]
fn dom_022_empty_tag_filter_does_not_trigger_parse() {
    // Empty tag_filter_input is skipped by apply; default-constructed
    // App keeps its default include/exclude lists.
    let mut app = App::new();
    apply_session_data(
        &mut app,
        SessionData {
            tag_filter_input: String::new(),
            ..SessionData::default()
        },
    );
    assert!(app.filter.tag_include.is_empty());
    assert!(app.filter.tag_exclude.is_empty());
}

#[test]
fn dom_022_tag_filter_regex_glob_roundtrips_input_string() {
    // '*' triggers regex mode in parse_tag_filter.
    let mut app = App::new();
    app.filter.parse_tag_filter("net.*,-hb_*");
    assert!(app.filter.tag_regex);
    let data = session_data_from_app(&app);
    // Rebuilt from the literal include/exclude lists, leading `-` for excludes.
    // The saved string may not preserve the exact comma/order/prefix the
    // user typed — but loading it back must yield the same include/exclude
    // sets.
    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert_eq!(app2.filter.tag_include, vec!["net.*".to_string()]);
    assert_eq!(app2.filter.tag_exclude, vec!["hb_*".to_string()]);
    assert!(app2.filter.tag_regex);
}

// ── round-trip: search & exclude queries ──────────────────────────

#[test]
fn roundtrip_search_query() {
    let mut app = App::new();
    app.filter.set_search("timeout|500");
    let data = session_data_from_app(&app);
    assert_eq!(data.search_query, "timeout|500");

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert_eq!(app2.filter.search_query, "timeout|500");
}

#[test]
fn roundtrip_exclude_query() {
    let mut app = App::new();
    app.filter.set_exclude("/^hb_/");
    let data = session_data_from_app(&app);
    assert_eq!(data.exclude_query, "/^hb_/");

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    assert_eq!(app2.filter.exclude_query, "/^hb_/");
    assert!(app2.filter.exclude_regex);
}

// ── round-trip: bookmarks ─────────────────────────────────────────

#[test]
fn roundtrip_bookmarks_preserved_and_sorted() {
    let mut app = App::new();
    app.bookmarks.insert(7);
    app.bookmarks.insert(2);
    app.bookmarks.insert(42);

    let data = session_data_from_app(&app);
    // Bookmarks come from a BTreeSet → collected in sorted order.
    assert_eq!(data.bookmarks, vec![2, 7, 42]);

    let mut app2 = App::new();
    apply_session_data(&mut app2, data);
    let restored: Vec<usize> = app2.bookmarks.iter().copied().collect();
    assert_eq!(restored, vec![2, 7, 42]);
}

#[test]
fn roundtrip_bookmarks_dedupe_via_btreeset() {
    let mut app = App::new();
    apply_session_data(
        &mut app,
        SessionData {
            bookmarks: vec![1, 1, 2, 2, 3],
            ..SessionData::default()
        },
    );
    let restored: Vec<usize> = app.bookmarks.iter().copied().collect();
    // BTreeSet dedupes.
    assert_eq!(restored, vec![1, 2, 3]);
}

// ── file I/O round-trip through save_session_to_path ──────────────

#[test]
fn file_roundtrip_preserves_all_fields() {
    let path = unique_tmp_path();

    let mut src = App::new();
    src.filter.min_level = LogLevel::Info;
    src.filter.parse_tag_filter("keep,-drop");
    src.filter.set_search("alpha");
    src.filter.set_exclude("heartbeat");
    src.bookmarks.insert(3);
    src.bookmarks.insert(5);
    src.active_tab = ViewTab::Network;

    save_session_to_path(&src, &path).expect("save ok");

    let mut dst = App::new();
    load_session_from_path(&mut dst, &path).expect("load ok");

    assert_eq!(dst.filter.min_level, LogLevel::Info);
    assert_eq!(dst.filter.tag_include, vec!["keep".to_string()]);
    assert_eq!(dst.filter.tag_exclude, vec!["drop".to_string()]);
    assert_eq!(dst.filter.search_query, "alpha");
    assert_eq!(dst.filter.exclude_query, "heartbeat");
    let bm: Vec<usize> = dst.bookmarks.iter().copied().collect();
    assert_eq!(bm, vec![3, 5]);
    assert_eq!(dst.active_tab, ViewTab::Network);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_roundtrip_empty_session() {
    let path = unique_tmp_path();
    let src = App::new();
    save_session_to_path(&src, &path).expect("save ok");

    let mut dst = App::new();
    dst.filter.min_level = LogLevel::Error; // dirty state we expect to overwrite
    load_session_from_path(&mut dst, &path).expect("load ok");
    assert_eq!(dst.filter.min_level, LogLevel::System);

    let _ = std::fs::remove_file(&path);
}

// ── error paths ───────────────────────────────────────────────────

#[test]
fn load_from_missing_file_returns_io_err() {
    let path = unique_tmp_path(); // not written
    let mut app = App::new();
    let err = load_session_from_path(&mut app, &path).unwrap_err();
    match err {
        SessionLoadError::Io(_) => {}
        SessionLoadError::Parse(_) => panic!("expected Io err for missing file"),
    }
}

#[test]
fn load_from_corrupted_file_returns_parse_err() {
    let path = unique_tmp_path();
    std::fs::write(&path, "!!! not valid toml !!!\n[[[").unwrap();

    let mut app = App::new();
    let err = load_session_from_path(&mut app, &path).unwrap_err();
    match err {
        SessionLoadError::Parse(_) => {}
        SessionLoadError::Io(_) => panic!("expected Parse err for corrupted file"),
    }

    let _ = std::fs::remove_file(&path);
}

#[test]
fn load_error_does_not_mutate_app() {
    // App state is untouched when the file is bad.
    let path = unique_tmp_path();
    std::fs::write(&path, "garbage").unwrap();

    let mut app = App::new();
    app.filter.min_level = LogLevel::Warning;
    app.bookmarks.insert(99);
    let _ = load_session_from_path(&mut app, &path);
    assert_eq!(app.filter.min_level, LogLevel::Warning);
    assert!(app.bookmarks.contains(&99));

    let _ = std::fs::remove_file(&path);
}

// ── legacy `load_session`/`save_session` smoke — session_path based
//    — do not actually mutate user's config dir; just confirm they
//    exist and don't panic on a pristine App. The filesystem write
//    target is whatever dirs::config_dir() resolves to; if the file
//    is missing, load() returns silently.

#[test]
fn load_session_does_not_panic_on_missing_config() {
    // On CI the config dir may or may not exist; either way this must
    // not panic. It silently returns on error.
    let mut app = App::new();
    load_session(&mut app);
    // No assertion on fields — we only pin "does not panic".
}
