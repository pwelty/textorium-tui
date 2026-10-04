// Included in app::tests so the regression exercises the same handlers as the event loop.
#[test]
fn safety_all_batch_operations_stage_and_noop_preserves_last_undo() {
    for operation in [
        BatchOp::AddField { key: "author".into(), value: "New".into() },
        BatchOp::SetValue { key: "title".into(), value: "Updated".into() },
        BatchOp::RemoveField { key: "tags".into() },
        BatchOp::ToggleDraft,
    ] {
        let source = "---\ntitle: Original\ndraft: false\ntags: [one]\n---\nbody\n";
        let f = create_temp_markdown(source);
        let mut app = make_app(vec![read_post(f.path()).unwrap()]);
        let before = app.posts[0].frontmatter.clone();
        app.selected_posts.insert(f.path().into());
        app.apply_batch_op(&operation);
        assert_ne!(app.posts[0].frontmatter, before);
        assert_eq!(std::fs::read_to_string(f.path()).unwrap(), source);
        app.selected_posts.insert(f.path().into());
        app.apply_batch_op(&BatchOp::RemoveField { key: "title".into() });
        assert!(app.status_message.contains("no changes"));
        app.revert_batch();
        assert_eq!(app.posts[0].frontmatter, before);
        assert_eq!(std::fs::read_to_string(f.path()).unwrap(), source);
    }
}

#[test]
fn safety_external_conflict_is_painted_and_preserves_dirty_body() {
    let f = create_temp_markdown("---\ntitle: Original\n---\nbody\n");
    let mut app = make_app(vec![read_post(f.path()).unwrap()]);
    app.posts[0].content = "unsaved body".into();
    std::fs::write(f.path(), "external bytes\n").unwrap();
    app.save_all();
    assert_eq!(app.posts[0].content, "unsaved body");
    assert_eq!(app.dirty_count(), 1);
    let backend = ratatui::backend::TestBackend::new(180, 42);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| ui(frame, &mut app)).unwrap();
    let text: String = terminal.backend().buffer().content.iter().map(|cell| cell.symbol()).collect();
    assert!(text.contains("File changed externally; edits retained"));
    assert!(text.contains("1 unsaved change"));
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), "external bytes\n");
}

#[test]
fn safety_external_editor_refuses_dirty_selected_post_before_launch() {
    let f = create_temp_markdown("---\ntitle: Original\n---\nbody\n");
    let mut app = make_app(vec![read_post(f.path()).unwrap()]);
    app.ensure_filtered();
    app.posts[0].content.push_str(" unsaved");
    app.config.editor = Some("must-not-run-editor".into());
    assert!(app.open_in_editor().unwrap_err().to_string().contains("Unsaved edits retained"));
}
#[test]
fn safety_refresh_refuses_metadata_and_body_edits_then_clean_refreshes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("post.md");
    std::fs::write(&path, "---\ntitle: Original\n---\nbody\n").unwrap();
    for body_edit in [false, true] {
        let mut app = make_app(vec![read_post(&path).unwrap()]);
        app.config.site_path = dir.path().to_string_lossy().into_owned();
        app.config.content_dir = ".".into();
        if body_edit { app.posts[0].content.push_str(" unsaved"); }
        else { app.posts[0].frontmatter.insert("title".into(), serde_json::json!("Unsaved")); }
        app.refresh_posts();
        assert_eq!(app.dirty_count(), 1);
        assert!(app.status_message.contains("Refresh refused"));
        app.refresh_posts(); // Repeated refresh is never an implicit discard confirmation.
        assert_eq!(app.dirty_count(), 1);
        app.ensure_filtered();
        app.revert_selected(); // Explicit cancellation/revert never writes disk.
        assert_eq!(app.dirty_count(), 0);
        std::fs::write(&path, "---\ntitle: External\n---\nbody\n").unwrap();
        app.refresh_posts();
        assert_eq!(app.posts[0].title, "External");
        assert!(app.status_message.contains("Refreshed"));
        std::fs::write(&path, "---\ntitle: Original\n---\nbody\n").unwrap();
    }
}

#[test]
fn safety_batch_and_undo_never_write_before_save_or_discard_dirty_body() {
    let source = "---\ntitle: Original\ndraft: false\n---\n\nbody\n";
    let f = create_temp_markdown(source);
    let mut post = read_post(f.path()).unwrap();
    post.content = "preexisting unsaved body".into();
    post.frontmatter.insert("author".into(), serde_json::json!("Preexisting"));
    let before = post.frontmatter.clone();
    let mut app = make_app(vec![post]);
    app.selected_posts.insert(f.path().into());
    app.apply_batch_op(&BatchOp::ToggleDraft);
    assert!(app.posts[0].draft);
    assert!(app.status_message.contains("staged"));
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), source);
    app.revert_batch();
    assert_eq!(app.posts[0].frontmatter, before);
    assert_eq!(app.posts[0].content, "preexisting unsaved body");
    assert_eq!(app.dirty_count(), 1);
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), source);
}

#[test]
fn safety_undo_after_save_stages_inverse_and_retains_later_field_body_edits() {
    let f = create_temp_markdown("---\ntitle: Original\ndraft: false\n---\nbody\n");
    let mut app = make_app(vec![read_post(f.path()).unwrap()]);
    app.selected_posts.insert(f.path().into());
    app.apply_batch_op(&BatchOp::ToggleDraft);
    app.save_all();
    assert!(read_post(f.path()).unwrap().draft);
    assert_eq!(app.dirty_count(), 0);
    let saved = std::fs::read(f.path()).unwrap();
    app.posts[0].content = "later body".into();
    app.posts[0].frontmatter.insert("author".into(), serde_json::json!("Later"));
    app.revert_batch();
    assert_eq!(std::fs::read(f.path()).unwrap(), saved);
    assert!(!app.posts[0].draft);
    assert_eq!(app.posts[0].content, "later body");
    assert_eq!(app.posts[0].frontmatter["author"], "Later");
    app.save_all();
    assert!(!read_post(f.path()).unwrap().draft);
    assert_eq!(read_post(f.path()).unwrap().content, "later body");
}

#[test]
fn safety_batch_partial_save_conflict_and_undo_keep_external_bytes() {
    let a = create_temp_markdown("---\ntitle: A\ndraft: false\n---\nbody A\n");
    let b = create_temp_markdown("---\ntitle: B\ndraft: false\n---\nbody B\n");
    let mut app = make_app(vec![read_post(a.path()).unwrap(), read_post(b.path()).unwrap()]);
    app.selected_posts.extend([a.path().into(), b.path().into()]);
    app.apply_batch_op(&BatchOp::ToggleDraft);
    std::fs::write(b.path(), "external B\n").unwrap();
    app.save_all();
    assert!(app.status_message.contains("Saved 1"));
    assert!(app.status_message.contains("1 error"));
    assert!(App::is_dirty(&app.posts[1]));
    assert!(app.posts[1].draft);
    assert_eq!(std::fs::read_to_string(b.path()).unwrap(), "external B\n");
    app.revert_batch();
    assert_eq!(std::fs::read_to_string(b.path()).unwrap(), "external B\n");
    assert!(read_post(a.path()).unwrap().draft);
    app.save_all();
    assert!(!read_post(a.path()).unwrap().draft);
    assert_eq!(std::fs::read_to_string(b.path()).unwrap(), "external B\n");
}

#[test]
fn safety_undo_preserves_subsequent_change_to_same_batch_field() {
    let f = create_temp_markdown("---\ntitle: Original\n---\nbody\n");
    let mut app = make_app(vec![read_post(f.path()).unwrap()]);
    app.selected_posts.insert(f.path().into());
    app.apply_batch_op(&BatchOp::SetValue { key: "title".into(), value: "Batch".into() });
    app.posts[0].frontmatter.insert("title".into(), serde_json::json!("Later"));
    app.revert_batch();
    assert_eq!(app.posts[0].title, "Later");
    assert!(app.status_message.contains("0 field(s) restored"));
    assert!(app.status_message.contains("1 later edit(s) retained"));
    assert_eq!(read_post(f.path()).unwrap().title, "Original");
}

// Read -> actual App transform -> explicit save -> reload, not helper-only coverage.
fn assert_smartquotes_save(source: &str, expected: &str) {
    let f = create_temp_markdown(source);
    let mut app = make_app(vec![read_post(f.path()).unwrap()]);
    app.ensure_filtered();
    app.focused_pane = 2;
    app.apply_smartquotes();
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), source, "must stage edits");
    app.save_all();
    assert_eq!(std::fs::read_to_string(f.path()).unwrap(), expected, "source: {source:?}");
    assert!(app.status_message.contains("Saved"));
    assert_eq!(app.dirty_count(), 0);
    let reloaded = read_post(f.path()).unwrap();
    assert_eq!(app.posts[0].content, reloaded.content);
    app.apply_smartquotes();
    assert_eq!(app.dirty_count(), 0, "repeat transform must be idempotent");
}

#[test]
fn safety_first_indented_code_survives_smartquotes_and_save() {
    for header in ["---\ntitle: Original\n---\n", "+++\ntitle = \"Original\"\n+++\n", ""] {
        for indent in ["    ", "\t", " \t", "   \t"] {
            for newline in ["\n", "\r\n"] {
                for ending in ["", newline] {
                    let body = format!("{newline}{indent}\"raw\" -- ...{newline}{indent}'second' -- ...{newline}{newline}\"prose\" -- ...  {ending}");
                    let source = format!("{}{body}", header.replace('\n', newline));
                    let expected = source.replace("\"prose\" -- ...", "“prose” — …");
                    assert_smartquotes_save(&source, &expected);
                }
            }
        }
    }
}

#[test]
fn safety_tab_space_blockquote_fences_survive_smartquotes_and_save() {
    for prefix in [">\t", "> \t", " >\t", "  >\t", "   >\t", ">\t ", ">\t>\t", "> >\t"] {
        for fence in ["~~~", "~~~~", "```", "````"] {
            let source = format!("---\ntitle: Original\n---\n\n{prefix}{fence}rust\n{prefix}\"raw\" -- ... 'code'\n{prefix}{fence}\n\n\"prose\" -- ...\n");
            assert_smartquotes_save(&source, &source.replace("\"prose\" -- ...", "“prose” — …"));
        }
    }
}

#[test]
fn safety_existing_top_level_fence_save_control() {
    let source = "---\ntitle: Original\n---\n\n~~~\n\"raw\" -- ...\n~~~\n\n\"prose\"\n";
    assert_smartquotes_save(source, &source.replace("\"prose\"", "“prose”"));
}

#[test]
fn safety_blockquote_indentation_does_not_turn_code_into_a_fence() {
    // Five spaces after > leave four code columns; tabs leave virtual columns.
    // These are indented code containing fence-looking text, NOT fence openers.
    for prefix in [">     ", ">\t  ", "> \t  ", " >\t   ", "  >\t    ", "   >\t ", ">\t\t"] {
        let source = format!("---\ntitle: Original\n---\n\n{prefix}~~~\n\n> \"quoted prose\" -- ...\n\n{prefix}\"raw\" -- ...\n\n\"prose\"\n");
        let expected = source.replace("\"quoted prose\" -- ...", "“quoted prose” — …").replace("\"prose\"", "“prose”");
        assert_smartquotes_save(&source, &expected);
    }
}
