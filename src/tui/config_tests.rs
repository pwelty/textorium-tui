#[test]
fn config_reload_validates_before_replacing_and_preserves_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.json");
    let mut app = make_app(sample_posts());
    let original = app.posts[0].title.clone();
    for bytes in [
        "{broken",
        r#"{"site_name":"bad","site_path":"/nonexistent-synthetic-site","content_dir":"content","ssg":"hugo"}"#,
    ] {
        std::fs::write(&config_path, bytes).unwrap();
        assert!(app.reload_configuration(&config_path).is_err());
        assert_eq!(app.posts[0].title, original);
    }
    std::fs::create_dir(dir.path().join("content")).unwrap();
    let bytes = serde_json::to_string_pretty(&serde_json::json!({
        "sites": [{"name":"new", "path":dir.path(), "content_dir":"content", "ssg":"hugo", "unknown_site_field":42}],
        "active_site":"new", "unknown_top_field":{"keep":true}
    })).unwrap();
    std::fs::write(&config_path, &bytes).unwrap();
    app.reload_configuration(&config_path).unwrap();
    assert_eq!(app.config.site_name, "new");
    assert!(app.posts.is_empty());
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), bytes);
    assert!(app
        .empty_state_message()
        .unwrap()
        .contains("Valid empty site"));
}

#[test]
fn config_dirty_guard_covers_edit_and_reload() {
    let mut app = make_app(sample_posts());
    app.posts[0].content.push_str("unsaved");
    assert!(app
        .edit_configuration()
        .unwrap_err()
        .to_string()
        .contains("Unsaved"));
    assert!(app
        .reload_configuration(std::path::Path::new("/absent"))
        .unwrap_err()
        .to_string()
        .contains("Unsaved"));
    assert!(app.posts[0].content.ends_with("unsaved"));
}

#[test]
fn config_overlay_actions_and_discovery_fit_narrow_screen() {
    use ratatui::backend::TestBackend;
    for (width, height) in [(120, 40), (40, 12), (24, 8)] {
        let mut app = make_app(vec![]);
        app.ensure_filtered();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains(",:config ?:help"), "{width}x{height}: {text}");
        app.show_config = true;
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>();
        assert!(text.contains("Configuration"));
        assert!(text.contains("e:edit r:reload"));
        assert!(text.contains("Esc:close j/k:scroll"));
    }
}

#[test]
fn config_scaffold_and_path_errors_are_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings/config.json");
    Config::ensure_editable_file(&path).unwrap();
    let mut config = Config::load_from_file(&path).unwrap();
    assert!(config.site_path.is_empty());
    assert!(config
        .validate_paths()
        .unwrap_err()
        .to_string()
        .contains("No site configured"));
    let bytes = std::fs::read(&path).unwrap();
    Config::ensure_editable_file(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    config.site_path = dir.path().join("missing").display().to_string();
    assert!(config
        .validate_paths()
        .unwrap_err()
        .to_string()
        .contains("Site path"));
    config.site_path = dir.path().display().to_string();
    assert!(config
        .validate_paths()
        .unwrap_err()
        .to_string()
        .contains("Content directory"));
    std::fs::create_dir(dir.path().join("content")).unwrap();
    config.validate_paths().unwrap();
}
