use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::config::{Config, SsgType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub path: PathBuf,
    pub title: String,
    pub date: Option<DateTime<Utc>>,
    pub draft: bool,
    pub content_type: String,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
    pub content: String,
    pub frontmatter: HashMap<String, serde_json::Value>,
    /// Raw YAML text between --- delimiters, preserved for lossless save
    pub raw_frontmatter: String,
    /// Snapshot of frontmatter at load time, used to detect changes on save
    pub original_frontmatter: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    title: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    content_type: String,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

/// Parse frontmatter and body from markdown content
/// Returns (parsed HashMap, body text, raw YAML text between --- delimiters)
fn parse_frontmatter(
    content: &str,
) -> Result<(HashMap<String, serde_json::Value>, String, String)> {
    if !content.starts_with("---") {
        return Ok((HashMap::new(), content.to_string(), String::new()));
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return Ok((HashMap::new(), content.to_string(), String::new()));
    }

    let raw_yaml = parts[1].to_string();

    let frontmatter: Frontmatter =
        serde_yaml::from_str(parts[1]).context("Failed to parse YAML frontmatter")?;

    let body = parts[2].trim().to_string();

    // Convert to HashMap
    let mut fm_map = HashMap::new();
    fm_map.insert(
        "title".to_string(),
        serde_json::Value::String(frontmatter.title.clone()),
    );

    if let Some(date) = &frontmatter.date {
        fm_map.insert("date".to_string(), serde_json::Value::String(date.clone()));
    }

    fm_map.insert(
        "draft".to_string(),
        serde_json::Value::Bool(frontmatter.draft),
    );
    fm_map.insert(
        "content_type".to_string(),
        serde_json::Value::String(frontmatter.content_type.clone()),
    );

    if !frontmatter.categories.is_empty() {
        let cats: Vec<serde_json::Value> = frontmatter
            .categories
            .iter()
            .map(|c| serde_json::Value::String(c.clone()))
            .collect();
        fm_map.insert("categories".to_string(), serde_json::Value::Array(cats));
    } else if let Some(cat) = &frontmatter.category {
        fm_map.insert(
            "categories".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String(cat.clone())]),
        );
    }

    if !frontmatter.tags.is_empty() {
        let tags: Vec<serde_json::Value> = frontmatter
            .tags
            .iter()
            .map(|t| serde_json::Value::String(t.clone()))
            .collect();
        fm_map.insert("tags".to_string(), serde_json::Value::Array(tags));
    }

    // Add extra fields
    for (key, value) in frontmatter.extra {
        fm_map.insert(key, value);
    }

    Ok((fm_map, body, raw_yaml))
}

/// Read a single post from a file
pub fn read_post(path: &Path) -> Result<Post> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read post: {}", path.display()))?;

    let (frontmatter, body, raw_yaml) = parse_frontmatter(&content)?;

    // Extract fields
    let title = frontmatter
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled")
        .to_string();

    let date = frontmatter
        .get("date")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            // Try RFC 3339 first (2023-07-08T00:00:00Z)
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                return Some(dt.with_timezone(&Utc));
            }
            // Try ISO date format (2023-07-08)
            if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
                return Some(naive_date.and_hms_opt(0, 0, 0)?.and_utc());
            }
            None
        });

    let draft = frontmatter
        .get("draft")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let content_type = frontmatter
        .get("content_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let categories = frontmatter
        .get("categories")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let tags = frontmatter
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(Post {
        path: path.to_path_buf(),
        title,
        date,
        draft,
        content_type,
        categories,
        tags,
        content: body,
        original_frontmatter: frontmatter.clone(),
        frontmatter,
        raw_frontmatter: raw_yaml,
    })
}

/// Result of scanning posts: successful posts and any parse errors
pub struct ScanResult {
    pub posts: Vec<Post>,
    pub errors: Vec<(PathBuf, String)>,
}

/// Scan directory for all markdown posts
pub fn scan_posts(config: &Config) -> Result<ScanResult> {
    let content_path = config.content_path();

    if !content_path.exists() {
        return Ok(ScanResult {
            posts: Vec::new(),
            errors: Vec::new(),
        });
    }

    let mut posts = Vec::new();
    let mut errors = Vec::new();

    for entry in WalkDir::new(&content_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip non-markdown files
        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|s| s.to_str());
        if ext != Some("md") && ext != Some("markdown") {
            continue;
        }

        // Skip Hugo section index files (_index.md, index.md)
        if config.ssg == SsgType::Hugo {
            let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if file_name == "_index.md" || file_name == "index.md" {
                continue;
            }
        }

        match read_post(path) {
            Ok(post) => posts.push(post),
            Err(e) => errors.push((path.to_path_buf(), format!("{:#}", e))),
        }
    }

    // Sort by date, newest first
    posts.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(ScanResult { posts, errors })
}

/// Serialize a serde_json::Value as inline YAML suitable for frontmatter
fn value_to_yaml_inline(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => {
            // Quote strings that contain special YAML characters
            if s.contains(':')
                || s.contains('#')
                || s.contains('[')
                || s.contains(']')
                || s.contains('{')
                || s.contains('}')
                || s.contains(',')
                || s.contains('&')
                || s.contains('*')
                || s.contains('!')
                || s.contains('|')
                || s.contains('>')
                || s.contains('\'')
                || s.contains('\n')
                || s.starts_with(' ')
                || s.ends_with(' ')
            {
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            } else {
                s.clone()
            }
        }
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_yaml_inline).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(_) | serde_json::Value::Null => {
            // Fall back to serde_yaml for complex types
            serde_yaml::to_string(value)
                .unwrap_or_default()
                .trim()
                .to_string()
        }
    }
}

/// Find the line range for a top-level YAML key in raw frontmatter lines.
/// Returns (start_index, end_index_exclusive) or None if not found.
fn find_key_lines(lines: &[&str], key: &str) -> Option<(usize, usize)> {
    let prefix = format!("{}:", key);
    let start = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed == prefix || trimmed.starts_with(&format!("{}: ", key))
    })?;

    // Find end: next top-level key or end of lines
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !line.starts_with(' ')
                && !line.starts_with('\t')
                && !trimmed.starts_with('-')
        })
        .map(|pos| start + 1 + pos)
        .unwrap_or(lines.len());

    Some((start, end))
}

/// Save a post back to disk, preserving original frontmatter formatting where possible
pub fn save_post(post: &Post) -> Result<()> {
    // If frontmatter is unchanged, write back the original file exactly
    if post.frontmatter == post.original_frontmatter {
        let full_content = format!("---{}---\n\n{}\n", post.raw_frontmatter, post.content);
        fs::write(&post.path, full_content)
            .with_context(|| format!("Failed to write post: {}", post.path.display()))?;
        return Ok(());
    }

    // Frontmatter was modified — patch the raw YAML
    let raw_lines: Vec<&str> = post.raw_frontmatter.lines().collect();
    let mut result_lines: Vec<String> = Vec::new();
    let mut processed_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Process existing lines, replacing modified values and skipping deleted keys
    let mut i = 0;
    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();

        // Check if this is a top-level key line
        if !trimmed.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.starts_with('-')
        {
            if let Some(colon_pos) = trimmed.find(':') {
                let key = trimmed[..colon_pos].to_string();

                if let Some((start, end)) = find_key_lines(&raw_lines, &key) {
                    if start == i {
                        processed_keys.insert(key.clone());

                        if let Some(new_value) = post.frontmatter.get(&key) {
                            // Key still exists — check if value changed
                            let original_value = post.original_frontmatter.get(&key);
                            if original_value == Some(new_value) {
                                // Unchanged — keep original lines
                                for line in &raw_lines[start..end] {
                                    result_lines.push(line.to_string());
                                }
                            } else {
                                // Changed — write new value inline
                                result_lines.push(format!(
                                    "{}: {}",
                                    key,
                                    value_to_yaml_inline(new_value)
                                ));
                            }
                            i = end;
                            continue;
                        } else {
                            // Key was deleted — skip all its lines
                            i = end;
                            continue;
                        }
                    }
                }
            }
        }

        // Not a top-level key or not matched — keep the line as-is
        result_lines.push(line.to_string());
        i += 1;
    }

    // Append any new keys that weren't in the original
    for (key, value) in &post.frontmatter {
        if !processed_keys.contains(key) && !post.original_frontmatter.contains_key(key) {
            result_lines.push(format!("{}: {}", key, value_to_yaml_inline(value)));
        }
    }

    // Reconstruct the file
    let frontmatter_text = result_lines.join("\n");
    let full_content = format!("---\n{}\n---\n\n{}\n", frontmatter_text, post.content);

    fs::write(&post.path, full_content)
        .with_context(|| format!("Failed to write post: {}", post.path.display()))?;

    Ok(())
}

/// Options for creating a new post
pub struct CreatePostOptions {
    pub title: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Create a new post file with YAML frontmatter and return the path
pub fn create_post(config: &Config, options: &CreatePostOptions) -> Result<PathBuf> {
    let slug = slugify(&options.title);
    let now = Utc::now();

    // Build file path based on SSG type
    let content_path = config.content_path();
    let file_path = match config.ssg {
        SsgType::Hugo => content_path.join("posts").join(format!("{}.md", slug)),
        SsgType::Jekyll => content_path.join(format!("{}-{}.md", now.format("%Y-%m-%d"), slug)),
        SsgType::Eleventy => content_path.join(format!("{}.md", slug)),
    };

    // Ensure parent directory exists
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Build frontmatter
    let date_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut frontmatter = format!(
        "---\ntitle: \"{}\"\ndate: {}\ndraft: true\n",
        options.title.replace('"', "\\\""),
        date_str
    );

    if let Some(cat) = &options.category {
        frontmatter.push_str(&format!("categories: [{}]\n", cat));
    }

    if let Some(tags) = &options.tags {
        let tags_str = tags.join(", ");
        frontmatter.push_str(&format!("tags: [{}]\n", tags_str));
    }

    frontmatter.push_str("---\n");

    fs::write(&file_path, &frontmatter)
        .with_context(|| format!("Failed to write post: {}", file_path.display()))?;

    Ok(file_path)
}

/// Convert a title to a URL-friendly slug
fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_post(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn test_save_unchanged_post_is_byte_identical() {
        let original = "---\ntitle: My post\ndate: 2025-01-15\ndraft: false\ntags: [rust, tui]\ncategories: [dev]\n---\n\nHello world.\n";
        let f = create_temp_post(original);
        let post = read_post(f.path()).unwrap();

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        assert_eq!(
            saved, original,
            "Unchanged post should be byte-identical after save"
        );
    }

    #[test]
    fn test_save_preserves_field_order_on_edit() {
        let original = "---\ntitle: Original title\ndate: 2025-01-15\ndraft: true\ntags: [a, b]\n---\n\nBody text.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        // Change the title
        post.frontmatter.insert(
            "title".to_string(),
            serde_json::Value::String("New title".to_string()),
        );

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        // Title should come before date (preserved order)
        let title_pos = saved.find("title:").unwrap();
        let date_pos = saved.find("date:").unwrap();
        assert!(
            title_pos < date_pos,
            "Field order should be preserved after edit"
        );
        assert!(saved.contains("title: New title"));
        assert!(saved.contains("date: 2025-01-15"));
    }

    #[test]
    fn test_save_only_modifies_changed_fields() {
        let original =
            "---\ntitle: My post\ndate: 2025-01-15T10:00:00Z\ndraft: false\n---\n\nContent here.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        // Change only draft
        post.frontmatter
            .insert("draft".to_string(), serde_json::Value::Bool(true));

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        // Original date format should be preserved exactly
        assert!(
            saved.contains("date: 2025-01-15T10:00:00Z"),
            "Unchanged fields should be preserved exactly"
        );
        assert!(saved.contains("draft: true"), "Changed field should update");
    }

    #[test]
    fn test_save_handles_added_fields() {
        let original = "---\ntitle: My post\n---\n\nBody.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        post.frontmatter.insert(
            "author".to_string(),
            serde_json::Value::String("Paul".to_string()),
        );

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        assert!(
            saved.contains("author: Paul"),
            "New field should be appended"
        );
        assert!(
            saved.contains("title: My post"),
            "Existing fields preserved"
        );
    }

    #[test]
    fn test_save_handles_deleted_fields() {
        let original = "---\ntitle: My post\nauthor: Paul\ndraft: false\n---\n\nBody.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        post.frontmatter.remove("author");

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        assert!(!saved.contains("author"), "Deleted field should be removed");
        assert!(saved.contains("title: My post"));
        assert!(saved.contains("draft: false"));
    }

    #[test]
    fn test_slugify_basic() {
        assert_eq!(slugify("My First Post"), "my-first-post");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(
            slugify("Hello, World! It's 2026"),
            "hello-world-it-s-2026"
        );
    }

    #[test]
    fn test_slugify_extra_spaces() {
        assert_eq!(slugify("  Too   Many  Spaces  "), "too-many-spaces");
    }

    #[test]
    fn test_create_post_hugo() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };

        let options = CreatePostOptions {
            title: "My Test Post".to_string(),
            category: Some("dev".to_string()),
            tags: Some(vec!["rust".to_string(), "tui".to_string()]),
        };

        let path = create_post(&config, &options).unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("my-test-post.md"));
        assert!(path.to_string_lossy().contains("content/posts/"));

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("title: \"My Test Post\""));
        assert!(content.contains("draft: true"));
        assert!(content.contains("categories: [dev]"));
        assert!(content.contains("tags: [rust, tui]"));
    }

    #[test]
    fn test_create_post_jekyll() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "_posts".to_string(),
            ssg: SsgType::Jekyll,
            ..Default::default()
        };

        let options = CreatePostOptions {
            title: "Jekyll Post".to_string(),
            category: None,
            tags: None,
        };

        let path = create_post(&config, &options).unwrap();

        assert!(path.exists());
        // Jekyll format: _posts/YYYY-MM-DD-slug.md
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(filename.ends_with("-jekyll-post.md"));
        assert!(filename.len() > 15); // date prefix + slug

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("title: \"Jekyll Post\""));
        assert!(content.contains("draft: true"));
        assert!(!content.contains("categories:"));
        assert!(!content.contains("tags:"));
    }

    #[test]
    fn test_create_post_no_category_no_tags() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };

        let options = CreatePostOptions {
            title: "Plain Post".to_string(),
            category: None,
            tags: None,
        };

        let path = create_post(&config, &options).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        assert!(!content.contains("categories:"));
        assert!(!content.contains("tags:"));
        assert!(content.starts_with("---\n"));
        assert!(content.contains("---\n"));
    }
}
