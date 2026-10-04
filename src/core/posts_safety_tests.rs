use super::*;

#[test]
fn safety_body_projection_preserves_significant_whitespace_and_recomposes() {
    for (body, expected) in [
        ("\n\n    \"raw\"  \n\n", "    \"raw\"  "),
        (
            "\r\n \t\r\n\tcode\r\n \"prose\"  \r\n\t\r\n",
            "\tcode\r\n \"prose\"  ",
        ),
        ("\n   prose  ", "   prose  "),
        ("\n \t\r\n", ""),
        ("", ""),
    ] {
        let (leading, projected, trailing) = body_projection(body);
        assert_eq!(projected, expected);
        assert_eq!(format!("{leading}{projected}{trailing}"), body);
        for header in [
            "---\ntitle: Original\n---",
            "+++\ntitle = \"Original\"\n+++",
        ] {
            let source = format!("{header}{body}");
            let (_dir, path, mut post) = fixture(&source);
            assert_eq!(post.content, expected);
            post.frontmatter
                .insert("title".into(), serde_json::json!("Updated"));
            save_post(&post).unwrap();
            assert!(fs::read_to_string(&path).unwrap().ends_with(body));
            post = read_post(&path).unwrap();
            post.content = smartquotes(&post.content);
            save_post(&post).unwrap();
            assert!(fs::read_to_string(&path)
                .unwrap()
                .ends_with(&format!("{leading}{}{trailing}", smartquotes(expected))));
        }
    }
}

#[test]
fn safety_adding_body_after_header_eof_keeps_closing_delimiter_valid() {
    for source in [
        "---\ntitle: Header\n---",
        "+++\r\ntitle = \"Header\"\r\n+++",
    ] {
        let (_dir, path, mut post) = fixture(source);
        post.content = "new body".into();
        save_post(&post).unwrap();
        assert_eq!(read_post(&path).unwrap().content, "new body");
        assert_eq!(read_post(&path).unwrap().title, "Header");
    }
}

#[test]
fn safety_metadata_preserves_empty_fields_alias_and_literal_delimiters() {
    for source in [
        "---\r\ntitle: Original\r\ncategory: dev\r\ntags: []\r\nempty: null\r\n---\r\n\r\nbody\r\n",
        "+++\r\ntitle = \"Original\"\r\ncategory = \"dev\"\r\ntags = []\r\n+++\r\n\r\nbody\r\n",
        "---\ntitle: \"literal---delimiter\"\n---\nbody",
        "+++\ntitle = \"literal+++delimiter\"\n+++\nbody",
    ] {
        let (_dir, path, mut post) = fixture(source);
        post.frontmatter
            .insert("draft".into(), serde_json::json!(true));
        save_post(&post).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        if source.contains("category") {
            assert!(saved.contains("category"));
        }
        if source.contains("tags") {
            assert!(saved.contains("tags"));
        }
        if source.contains("empty") {
            assert!(saved.contains("empty: null"));
        }
        assert_eq!(read_post(&path).unwrap().title, post.title);
        assert!(read_post(&path).unwrap().draft);
        if source.contains("\r\n") {
            assert!(!saved.replace("\r\n", "").contains('\n'));
        }
    }
}

#[test]
#[cfg(unix)]
fn safety_symlink_replacement_is_refused_without_touching_target() {
    let (dir, path, mut post) = fixture("---\ntitle: Original\n---\nbody\n");
    let target = dir.path().join("external.md");
    fs::write(&target, fs::read(&path).unwrap()).unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    post.content.push_str(" unsaved");
    let external = fs::read(&target).unwrap();
    assert!(save_post(&post).is_err());
    assert_eq!(fs::read(&target).unwrap(), external);
    assert!(fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
}

fn fixture(source: &str) -> (tempfile::TempDir, PathBuf, Post) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("post.md");
    fs::write(&path, source).unwrap();
    let post = read_post(&path).unwrap();
    (dir, path, post)
}

#[test]
fn safety_metadata_save_preserves_exact_body_bytes() {
    for header in [
        "---\ntitle: Original\n---",
        "+++\ntitle = \"Original\"\n+++",
    ] {
        for body in [
            "\n\n\n```rust\nlet s = \"raw\";\n```\n\n\n",
            "\r\n\r\ncode\r\n\r\n",
            "\n\nbody",
            "\n",
            "",
            "\n   body  \n",
        ] {
            let source = format!("{header}{body}");
            let (_dir, path, mut post) = fixture(&source);
            post.frontmatter
                .insert("title".into(), serde_json::json!("Updated"));
            save_post(&post).unwrap();
            let saved = fs::read_to_string(&path).unwrap();
            let delimiter = if header.starts_with("+++") {
                "+++"
            } else {
                "---"
            };
            assert_eq!(
                saved.rsplit_once(delimiter).unwrap().1.as_bytes(),
                body.as_bytes()
            );
            assert_eq!(read_post(&path).unwrap().title, "Updated");
        }
    }
}

#[test]
fn safety_noop_preserves_bytes_identity_and_absent_fields() {
    for source in [
        "---\ndraft: true\n---\r\n\r\nbody\r\n\r\n",
        "+++\ndraft = true\n+++\nbody",
        "plain\r\n\n",
        "---\ntitle: \"a---b\"\n---\nbody",
    ] {
        let (_dir, path, post) = fixture(source);
        let before = fs::metadata(&path).unwrap();
        save_post(&post).unwrap();
        let after = fs::metadata(&path).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(before.ino(), after.ino());
        }
        if !source.contains("title:") {
            assert!(!post.frontmatter.contains_key("title"));
        }
    }
}

#[test]
fn safety_plain_markdown_body_edits_stay_plain_metadata_adds_yaml() {
    let (_dir, path, mut post) = fixture("\nplain\r\n\n");
    post.content.push_str("next\n");
    save_post(&post).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "\nplain\r\n\nnext\n");
    post = read_post(&path).unwrap();
    post.frontmatter
        .insert("title".into(), serde_json::json!("New"));
    save_post(&post).unwrap();
    assert!(fs::read_to_string(&path)
        .unwrap()
        .starts_with("---\ntitle: New\n---\n"));
    assert_eq!(read_post(&path).unwrap().content, "plain\r\n\nnext");
}

#[test]
fn safety_changed_deleted_replaced_files_refuse_writes() {
    for mode in ["changed", "deleted", "replaced"] {
        let (_dir, path, mut post) = fixture("---\ntitle: Original\n---\nbody\n");
        post.frontmatter
            .insert("title".into(), serde_json::json!("Unsaved"));
        match mode {
            "changed" => fs::write(&path, "external bytes\n").unwrap(),
            "deleted" => fs::remove_file(&path).unwrap(),
            _ => {
                let replacement = path.with_extension("replacement");
                fs::write(&replacement, fs::read(&path).unwrap()).unwrap();
                fs::rename(replacement, &path).unwrap();
            }
        }
        let disk = fs::read(&path).ok();
        assert!(save_post(&post)
            .unwrap_err()
            .to_string()
            .contains("externally"));
        assert_eq!(fs::read(&path).ok(), disk);
        assert_eq!(post.frontmatter["title"], "Unsaved");
    }
}

#[test]
#[cfg(unix)]
fn safety_save_preserves_permissions_and_write_failure_retains_edits() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, path, mut post) = fixture("---\ntitle: Original\n---\nbody\n");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    post.frontmatter
        .insert("title".into(), serde_json::json!("Saved"));
    save_post(&post).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    post = read_post(&path).unwrap();
    post.content.push_str(" unsaved");
    let disk = fs::read(&path).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = save_post(&post);
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), disk);
    assert!(post.content.ends_with(" unsaved"));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn safety_body_only_save_keeps_header_and_whitespace_boundaries() {
    let source = "+++\r\n# comment\r\ntitle = \"Original\"\r\n+++\r\n\r\n\"quote\"\r\n\r\n";
    let (_dir, path, mut post) = fixture(source);
    post.content = smartquotes(&post.content);
    save_post(&post).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        source.replace("\"quote\"", "“quote”")
    );
}

#[test]
fn safety_smartquotes_preserves_fences_and_multibacktick_spans() {
    for fence in ["```", "````", "~~~", "~~~~"] {
        let source =
            format!("\"before\"\n{fence}rust\n\"raw\" -- ... 'code' `odd\n{fence}\n\"after\"");
        let expected = source
            .replace("\"before\"", "“before”")
            .replace("\"after\"", "“after”");
        assert_eq!(smartquotes(&source), expected);
    }
    for span in [
        "`\"code\" -- ...`",
        "``\"code\" ` -- ...``",
        "```\"code\" `` -- ...```",
        "``\"line\ncode\"``",
    ] {
        let source = format!("\"prose\" {span} \"more\"");
        assert_eq!(smartquotes(&source), format!("“prose” {span} “more”"));
    }
    assert_eq!(
        smartquotes("~~~\n\"unclosed\" -- ..."),
        "~~~\n\"unclosed\" -- ..."
    );
    assert_eq!(smartquotes("`unmatched \"prose\""), "`unmatched “prose”");
    assert_eq!(
        smartquotes("   ```\n\"raw\"\n   ```\n\"text\""),
        "   ```\n\"raw\"\n   ```\n“text”"
    );
    for code in [
        "> ~~~\n> \"raw\" -- ...\n> ~~~",
        "- ~~~\n  \"raw\" -- ...\n  ~~~",
        "1. ```\n   \"raw\" -- ...\n   ```",
        "    \"indented\" -- ...",
        "\t\"tab code\" -- ...",
    ] {
        assert_eq!(
            smartquotes(&format!("{code}\n\"prose\"")),
            format!("{code}\n“prose”")
        );
    }
}
