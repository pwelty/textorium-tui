use super::*;

#[test]
fn safety_shared_generator_fixture_coverage() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../../tests/fixtures/discovery.json")).unwrap();
    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = tempfile::tempdir().unwrap();
        if let Some(marker) = case["marker"].as_str() {
            fs::write(dir.path().join(marker), "").unwrap();
        }
        if let Some(package) = case.get("package") {
            fs::write(dir.path().join("package.json"), package.to_string()).unwrap();
        }
        for field in ["posts", "excluded"] {
            if let Some(paths) = case[field].as_array() {
                for path in paths {
                    let path = dir.path().join(path.as_str().unwrap());
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, "---\ntitle: Fixture\n---\nbody\n").unwrap();
                }
            }
        }
        let root = dir.path().join(case["root"].as_str().unwrap());
        for ignored in [
            ".git",
            "node_modules",
            "vendor",
            "target",
            "dist",
            "public",
            "_site",
            "_output",
            ".next",
            ".astro",
            ".textorium",
        ] {
            fs::create_dir_all(root.join(ignored)).unwrap();
            fs::write(root.join(ignored).join("excluded.md"), "excluded").unwrap();
        }
        configure_site_to(dir.path().to_str().unwrap(), config_dir.path()).unwrap();
        let config = Config::load_from(config_dir.path()).unwrap();
        assert_eq!(
            serde_json::to_value(&config.ssg).unwrap(),
            case["ssg"],
            "{}",
            case["name"]
        );
        assert_eq!(
            config.content_dir,
            case["root"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        let result = crate::core::posts::scan_posts(&config).unwrap();
        assert!(result.errors.is_empty(), "{}", case["name"]);
        let mut actual: Vec<String> = result
            .posts
            .iter()
            .map(|post| {
                post.path
                    .strip_prefix(fs::canonicalize(dir.path()).unwrap())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        let mut expected: Vec<String> = case["posts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "{}", case["name"]);
    }
}

#[test]
fn safety_leaf_preview_and_configured_server_base_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = Config {
        site_path: "/site".into(),
        server_url: Some("http://localhost:9123/custom/".into()),
        ..Config::default()
    };
    assert_eq!(
        config
            .preview_url(Path::new("/site/content/posts/leaf/index.md"))
            .unwrap(),
        "http://localhost:9123/custom/posts/leaf/"
    );
    assert_eq!(
        config
            .preview_url(Path::new("/site/content/index.md"))
            .unwrap(),
        "http://localhost:9123/custom/"
    );
    config.save_to(dir.path()).unwrap();
    let loaded = Config::load_from(dir.path()).unwrap();
    assert_eq!(loaded.server_url, config.server_url);
    config.server_url = None; // reconfiguration with defaults does not erase a custom base
    config.save_to(dir.path()).unwrap();
    assert_eq!(
        Config::load_from(dir.path()).unwrap().server_url,
        loaded.server_url
    );
    config.server_url = loaded.server_url;
    config.ssg = SsgType::Jekyll;
    config.content_dir = "_posts".into();
    assert_eq!(
        config
            .preview_url(Path::new("/site/_drafts/draft.md"))
            .unwrap(),
        "http://localhost:9123/custom/draft/"
    );
}
