use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::config::{Config, SsgType};

#[derive(Debug, Clone, PartialEq, Default)]
pub enum FrontmatterFormat {
    #[default]
    Yaml,
    Toml,
}

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
    /// Raw frontmatter text between delimiters, preserved for lossless save
    pub raw_frontmatter: String,
    /// Snapshot of frontmatter at load time, used to detect changes on save
    pub original_frontmatter: HashMap<String, serde_json::Value>,
    /// Snapshot of content at load time, used to detect content-only changes
    pub original_content: String,
    /// Exact loaded bytes and file identity: optimistic write admission, never a cache authority.
    #[serde(skip)]
    pub original_source: Option<String>,
    #[serde(skip)]
    pub original_identity: Option<(u64, u64)>,
    /// Whether the frontmatter uses YAML (---) or TOML (+++) delimiters
    #[serde(skip)]
    pub format: FrontmatterFormat,
}

/// Convert a toml::Value to serde_json::Value
fn toml_value_to_json(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s.clone()),
        toml::Value::Integer(i) => serde_json::json!(*i),
        toml::Value::Float(f) => serde_json::json!(*f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(toml_value_to_json).collect())
        }
        toml::Value::Table(table) => {
            let map: serde_json::Map<String, serde_json::Value> = table
                .iter()
                .map(|(k, v)| (k.clone(), toml_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
    }
}

/// Return slices with the same shape as splitn, but only accept whole delimiter lines.
fn frontmatter_parts<'a>(source: &'a str, delimiter: &str) -> Vec<&'a str> {
    let mut lines = source.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return vec![source];
    };
    if first.trim_end_matches(['\r', '\n']) != delimiter {
        return vec![source];
    }
    let mut offset = first.len();
    for line in lines {
        if line.trim_end_matches(['\r', '\n']) == delimiter {
            return vec![
                "",
                &source[delimiter.len()..offset],
                &source[offset + delimiter.len()..],
            ];
        }
        offset += line.len();
    }
    vec![source]
}

fn file_identity(metadata: &fs::Metadata) -> (u64, u64) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        (metadata.dev(), metadata.ino())
    }
    #[cfg(not(unix))]
    {
        (metadata.len(), 0)
    }
}

fn check_baseline(post: &Post) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(&post.path).context(
        "File changed or deleted externally; edits retained, refresh/reconcile before saving",
    )?;
    let bytes =
        fs::read_to_string(&post.path).context("Cannot read file baseline; edits retained")?;
    if metadata.file_type().is_symlink()
        || post.original_identity != Some(file_identity(&metadata))
        || post.original_source.as_deref() != Some(bytes.as_str())
    {
        anyhow::bail!("File changed externally; edits retained, refresh/reconcile before saving");
    }
    Ok(metadata)
}

fn normalize_category(frontmatter: &mut HashMap<String, serde_json::Value>) {
    if let Some(category) = frontmatter
        .get("category")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
    {
        frontmatter.remove("category");
        frontmatter
            .entry("categories".into())
            .or_insert_with(|| serde_json::json!([category]));
    }
}

/// Separate boundary blank lines from the editable body. Keep indentation and
/// trailing spaces on nonblank lines: they can be Markdown/code syntax, not padding.
/// Load and save must use the same boundaries, including CRLF and whitespace-only bodies.
fn body_projection(body: &str) -> (&str, &str, &str) {
    let mut offset = 0;
    let mut start = None;
    let mut end = 0;
    for line in body.split_inclusive('\n') {
        if !line.trim().is_empty() {
            start.get_or_insert(offset);
            end = offset + line.trim_end_matches(['\r', '\n']).len();
        }
        offset += line.len();
    }
    let start = start.unwrap_or(body.len());
    let end = end.max(start);
    (&body[..start], &body[start..end], &body[end..])
}

/// Parse frontmatter and body from markdown content
/// Returns (parsed HashMap, body text, raw frontmatter text, format)
fn parse_frontmatter(
    content: &str,
) -> Result<(
    HashMap<String, serde_json::Value>,
    String,
    String,
    FrontmatterFormat,
)> {
    // Delimiters must occupy whole lines, not occur inside values or prose.
    if content.starts_with("+++") {
        let parts = frontmatter_parts(content, "+++");
        if parts.len() < 3 {
            return Ok((
                HashMap::new(),
                content.to_string(),
                String::new(),
                FrontmatterFormat::Toml,
            ));
        }

        let raw_toml = parts[1].to_string();
        let body = body_projection(parts[2]).1.to_string();

        let toml_table: toml::Table =
            toml::from_str(parts[1]).context("Failed to parse TOML frontmatter")?;

        // Convert TOML table to serde_json HashMap
        let mut fm_map: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, value) in &toml_table {
            fm_map.insert(key.clone(), toml_value_to_json(value));
        }

        // Normalize: merge singular "category" into "categories" array (same as YAML path)
        normalize_category(&mut fm_map);

        return Ok((fm_map, body, raw_toml, FrontmatterFormat::Toml));
    }

    // Detect YAML frontmatter (---...---)
    if !content.starts_with("---") {
        return Ok((
            HashMap::new(),
            content.to_string(),
            String::new(),
            FrontmatterFormat::Yaml,
        ));
    }

    let parts = frontmatter_parts(content, "---");
    if parts.len() < 3 {
        return Ok((
            HashMap::new(),
            content.to_string(),
            String::new(),
            FrontmatterFormat::Yaml,
        ));
    }

    let raw_yaml = parts[1].to_string();

    let mut fm_map: HashMap<String, serde_json::Value> = if parts[1].trim().is_empty() {
        HashMap::new()
    } else {
        serde_yaml::from_str(parts[1]).context("Failed to parse YAML frontmatter")?
    };
    // Keep every actual key/value (including empty arrays and absent title), with
    // the established singular-category projection used by the metadata UI.
    normalize_category(&mut fm_map);
    let body = body_projection(parts[2]).1.to_string();

    Ok((fm_map, body, raw_yaml, FrontmatterFormat::Yaml))
}

/// Parse a date string, trying RFC 3339 first then ISO date format
pub fn parse_date_string(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(naive_date) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return naive_date.and_hms_opt(0, 0, 0).map(|dt| dt.and_utc());
    }
    None
}

impl Post {
    /// Sync struct fields (title, date, draft, etc.) from the frontmatter HashMap.
    /// Call after any mutation to frontmatter to keep struct fields consistent.
    pub fn sync_fields_from_frontmatter(&mut self) {
        self.title = self
            .frontmatter
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();
        self.date = self
            .frontmatter
            .get("date")
            .and_then(|v| v.as_str())
            .and_then(parse_date_string);
        self.draft = self
            .frontmatter
            .get("draft")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.content_type = self
            .frontmatter
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        self.tags = self
            .frontmatter
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        self.categories = self
            .frontmatter
            .get("categories")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
    }
}

/// Read a single post from a file
pub fn read_post(path: &Path) -> Result<Post> {
    let identity = file_identity(&fs::symlink_metadata(path)?);
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read post: {}", path.display()))?;

    let (frontmatter, body, raw_frontmatter_text, format) = parse_frontmatter(&content)?;

    let mut post = Post {
        path: path.to_path_buf(),
        title: String::new(),
        date: None,
        draft: false,
        content_type: String::new(),
        categories: Vec::new(),
        tags: Vec::new(),
        content: body.clone(),
        original_frontmatter: frontmatter.clone(),
        original_content: body,
        original_source: Some(content),
        original_identity: Some(identity),
        frontmatter,
        raw_frontmatter: raw_frontmatter_text,
        format,
    };
    post.sync_fields_from_frontmatter();
    Ok(post)
}

/// Result of scanning posts: successful posts and any parse errors
pub struct ScanResult {
    pub posts: Vec<Post>,
    pub errors: Vec<(PathBuf, String)>,
}

/// Scan directory for all markdown posts
pub fn scan_posts(config: &Config) -> Result<ScanResult> {
    let mut posts = Vec::new();
    let mut errors = Vec::new();

    for content_path in config.content_paths() {
        if !content_path.exists() {
            continue;
        }
        for entry_result in WalkDir::new(&content_path)
            .follow_links(false)
            .max_depth(20)
            .into_iter()
            .filter_entry(|entry| {
                !entry.file_type().is_dir()
                    || !matches!(
                        entry.file_name().to_str(),
                        Some(
                            ".git"
                                | "node_modules"
                                | "vendor"
                                | "target"
                                | "dist"
                                | "public"
                                | "_site"
                                | "_output"
                                | ".next"
                                | ".astro"
                                | ".textorium"
                        )
                    )
            })
        {
            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    let path = e
                        .path()
                        .unwrap_or(std::path::Path::new("<unknown>"))
                        .to_path_buf();
                    errors.push((path, format!("IO error: {}", e)));
                    continue;
                }
            };
            let path = entry.path();

            // Skip non-markdown files
            if !entry.file_type().is_file() {
                continue;
            }

            let ext = path.extension().and_then(|s| s.to_str());
            if ext != Some("md") && ext != Some("markdown") {
                continue;
            }

            // Hugo branch indexes are sections; leaf index.md files are posts.
            if config.ssg == SsgType::Hugo {
                let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if file_name == "_index.md" {
                    continue;
                }
            }

            match read_post(path) {
                Ok(post) => posts.push(post),
                Err(e) => errors.push((path.to_path_buf(), format!("{:#}", e))),
            }
        }
    }
    // Sort by date, newest first
    posts.sort_by_key(|b| std::cmp::Reverse(b.date));

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

/// Serialize a serde_json::Value as inline TOML suitable for frontmatter
fn value_to_toml_inline(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => {
            format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
        }
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_toml_inline).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(_) | serde_json::Value::Null => {
            // For complex types, fall back to a simple representation
            format!("\"{}\"", value)
        }
    }
}

/// Find the line range for a top-level TOML key in raw frontmatter lines.
/// Returns (start_index, end_index_exclusive) or None if not found.
fn find_toml_key_lines(lines: &[&str], key: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| {
        let trimmed = line.trim();
        trimmed.starts_with(key) && {
            let rest = trimmed[key.len()..].trim_start();
            rest.starts_with('=')
        }
    })?;

    // TOML top-level keys are single-line (no multi-line continuation for simple values)
    // But arrays/tables can span lines — find next top-level key or end
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#') && {
                // A new top-level key: word followed by =
                if let Some(eq_pos) = trimmed.find('=') {
                    let before_eq = trimmed[..eq_pos].trim();
                    !before_eq.is_empty()
                        && before_eq
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
                } else {
                    false
                }
            }
        })
        .map(|pos| start + 1 + pos)
        .unwrap_or(lines.len());

    Some((start, end))
}

/// Write content to a file atomically: write to a temp file in the same directory,
/// fsync its content, recheck the baseline, then rename over the target.
/// This protects replacement atomicity, not cross-process races or directory durability.
fn atomic_write(post: &Post, content: &str) -> Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = &post.path;
    let metadata = check_baseline(post)?;

    let tmp_path = path.with_file_name(format!(
        ".textorium-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    // Never truncate a preexisting sibling file (including another process's temp).
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .context("Failed to create unique save file; edits retained")?;
    let result = (|| -> Result<()> {
        f.set_permissions(metadata.permissions())?;
        f.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write temp file: {}", tmp_path.display()))?;
        f.sync_all()
            .with_context(|| format!("Failed to sync temp file: {}", tmp_path.display()))?;
        // Optimistic check, not a cross-process filesystem transaction.
        check_baseline(post)?;
        fs::rename(&tmp_path, path).with_context(|| {
            format!(
                "Failed to rename {} to {}",
                tmp_path.display(),
                path.display()
            )
        })?;
        Ok(())
    })();

    // Clean up temp file on error
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }

    result
}

/// Save a post back to disk, preserving original frontmatter formatting where possible
pub fn save_post(post: &Post) -> Result<()> {
    if post.frontmatter == post.original_frontmatter && post.content == post.original_content {
        return Ok(()); // A true no-op: no stat, rewrite, or absent-field materialization.
    }
    check_baseline(post)?;
    let (mut open_delim, mut close_delim) = match post.format {
        FrontmatterFormat::Yaml => ("---", "---"),
        FrontmatterFormat::Toml => ("+++", "+++"),
    };

    let source = post
        .original_source
        .as_deref()
        .context("Missing loaded baseline; reload before saving")?;
    let parts = frontmatter_parts(source, open_delim);
    let has_frontmatter = parts.len() == 3;
    if !has_frontmatter {
        open_delim = "---";
        close_delim = "---";
    }
    let body = if has_frontmatter { parts[2] } else { source };
    // Reattach only the blank-line envelope omitted by the loaded projection.
    let saved_body = if post.content == post.original_content {
        body.to_string()
    } else if has_frontmatter && body.is_empty() {
        // A header ending at EOF needs a delimiter line break before a new body.
        let newline = if post.raw_frontmatter.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        format!("{}{}", newline, post.content)
    } else if has_frontmatter {
        let (leading, _, trailing) = body_projection(body);
        format!("{}{}{}", leading, post.content, trailing)
    } else {
        post.content.clone()
    };

    // Content-only saves retain the exact original header (or remain plain Markdown).
    if post.frontmatter == post.original_frontmatter {
        let prefix = if has_frontmatter {
            &source[..source.len() - body.len()]
        } else {
            ""
        };
        let full_content = format!("{}{}", prefix, saved_body);
        atomic_write(post, &full_content)?;
        return Ok(());
    }

    // Frontmatter was modified — patch the raw text
    let raw_lines: Vec<&str> = post.raw_frontmatter.lines().collect();
    let mut result_lines: Vec<String> = Vec::new();
    let mut processed_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    let is_toml = open_delim == "+++";

    // Process existing lines, replacing modified values and skipping deleted keys
    let mut i = 0;
    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();
        if (trimmed.starts_with("category:") || trimmed.starts_with("category ="))
            && post.frontmatter.get("categories") == post.original_frontmatter.get("categories")
        {
            result_lines.push(line.to_string());
            i += 1;
            continue;
        }

        if is_toml {
            // TOML: top-level key lines use `key = value`
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                if let Some(eq_pos) = trimmed.find('=') {
                    let key = trimmed[..eq_pos].trim().to_string();

                    if let Some((start, end)) = find_toml_key_lines(&raw_lines, &key) {
                        if start == i {
                            processed_keys.insert(key.clone());

                            if let Some(new_value) = post.frontmatter.get(&key) {
                                let original_value = post.original_frontmatter.get(&key);
                                if original_value == Some(new_value) {
                                    for line in &raw_lines[start..end] {
                                        result_lines.push(line.to_string());
                                    }
                                } else {
                                    result_lines.push(format!(
                                        "{} = {}",
                                        key,
                                        value_to_toml_inline(new_value)
                                    ));
                                }
                                i = end;
                                continue;
                            } else {
                                i = end;
                                continue;
                            }
                        }
                    }
                }
            }
        } else {
            // YAML: top-level key lines use `key: value`
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
                                let original_value = post.original_frontmatter.get(&key);
                                if original_value == Some(new_value) {
                                    for line in &raw_lines[start..end] {
                                        result_lines.push(line.to_string());
                                    }
                                } else {
                                    result_lines.push(format!(
                                        "{}: {}",
                                        key,
                                        value_to_yaml_inline(new_value)
                                    ));
                                }
                                i = end;
                                continue;
                            } else {
                                i = end;
                                continue;
                            }
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
            if is_toml {
                result_lines.push(format!("{} = {}", key, value_to_toml_inline(value)));
            } else {
                result_lines.push(format!("{}: {}", key, value_to_yaml_inline(value)));
            }
        }
    }

    // Reconstruct the file
    let newline = if post.raw_frontmatter.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let frontmatter_text = result_lines.join(newline);
    let frontmatter_text = frontmatter_text.trim_start_matches(['\r', '\n']);
    let boundary = if frontmatter_text.ends_with(newline) {
        ""
    } else {
        newline
    };
    let full_content = format!(
        "{}{}{}{}{}{}{}",
        open_delim,
        newline,
        frontmatter_text,
        boundary,
        close_delim,
        if has_frontmatter { "" } else { newline },
        saved_body
    );

    let (saved_frontmatter, _, _, _) = parse_frontmatter(&full_content)?;
    if saved_frontmatter != post.frontmatter {
        anyhow::bail!("Cannot safely serialize this frontmatter edit; edits retained, use external editor after reconciliation");
    }
    atomic_write(post, &full_content)?;

    Ok(())
}

/// Options for creating a new post
pub struct CreatePostOptions {
    pub title: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    /// Frontmatter fields from a template, if any
    pub template_fields: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// Create a new post file with YAML frontmatter and return the path
pub fn create_post(config: &Config, options: &CreatePostOptions) -> Result<PathBuf> {
    let slug = slugify(&options.title);
    let now = Utc::now();

    // Validate category against path traversal
    if let Some(ref cat) = options.category {
        if cat.contains("..") || cat.contains('/') || cat.contains('\\') || cat.starts_with('.') {
            anyhow::bail!("Invalid category: must not contain path separators or '..'");
        }
    }

    // Build file path based on SSG type
    let content_path = config.content_path();
    let file_path = match config.ssg {
        SsgType::Hugo => {
            let section = options.category.as_deref().unwrap_or("posts");
            content_path.join(section).join(format!("{}.md", slug))
        }
        SsgType::Jekyll => content_path.join(format!("{}-{}.md", now.format("%Y-%m-%d"), slug)),
        SsgType::Eleventy => content_path.join(format!("{}.md", slug)),
        SsgType::Astro => {
            // Astro content collections: src/content/<collection>/<slug>.md
            let collection = options.category.as_deref().unwrap_or("blog");
            content_path.join(collection).join(format!("{}.md", slug))
        }
    };

    // Ensure parent directory exists
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Build frontmatter
    let date_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let frontmatter_body = if let Some(ref tmpl_fields) = options.template_fields {
        // Template-based frontmatter
        crate::core::templates::frontmatter_from_template(
            &options.title,
            &date_str,
            tmpl_fields,
            options.category.as_deref(),
            options.tags.as_deref(),
        )
    } else {
        // Minimal default frontmatter
        let mut lines = vec![
            format!("title: \"{}\"", options.title.replace('"', "\\\"")),
            format!("date: {}", date_str),
            "draft: true".to_string(),
        ];
        if let Some(cat) = &options.category {
            let escaped = cat.replace('"', "\\\"");
            lines.push(
                format!("categories: [\"{}\"]\n", escaped)
                    .trim_end_matches('\n')
                    .to_string(),
            );
        }
        if let Some(tags) = &options.tags {
            let quoted: Vec<String> = tags
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
                .collect();
            lines.push(format!("tags: [{}]", quoted.join(", ")));
        }
        lines.join("\n")
    };
    let frontmatter = format!("---\n{}\n---\n", frontmatter_body);

    fs::write(&file_path, &frontmatter)
        .with_context(|| format!("Failed to write post: {}", file_path.display()))?;

    Ok(file_path)
}

/// Convert a title to a URL-friendly slug
fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("-")
}

/// Convert straight quotes to typographically correct curly/smart quotes.
/// Also converts `--` to em dash and `...` to ellipsis.
/// Skips conversion inside code spans (backtick-delimited).
pub fn smartquotes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let protected_bytes = markdown_code_mask(text);
    let protected: Vec<bool> = text
        .char_indices()
        .map(|(offset, _)| protected_bytes[offset])
        .collect();

    while i < len {
        let ch = chars[i];
        if protected[i] {
            result.push(ch);
            i += 1;
            continue;
        }

        // Ellipsis: ... → …
        if ch == '.' && i + 2 < len && chars[i + 1] == '.' && chars[i + 2] == '.' {
            result.push('\u{2026}');
            i += 3;
            continue;
        }

        // Em dash: -- → —
        if ch == '-' && i + 1 < len && chars[i + 1] == '-' {
            result.push('\u{2014}');
            i += 2;
            continue;
        }

        // Double quotes
        if ch == '"' {
            if is_opening_context(&chars, i) {
                result.push('\u{201C}'); // left double quote
            } else {
                result.push('\u{201D}'); // right double quote
            }
            i += 1;
            continue;
        }

        // Single quotes / apostrophes
        if ch == '\'' {
            if is_opening_context(&chars, i) {
                result.push('\u{2018}'); // left single quote
            } else {
                result.push('\u{2019}'); // right single quote / apostrophe
            }
            i += 1;
            continue;
        }

        result.push(ch);
        i += 1;
    }

    result
}

/// Consume Markdown indentation using four-column tab stops, without altering bytes.
fn markdown_indent<'a>(line: &'a str, column: &mut usize) -> (&'a str, usize) {
    let start = *column;
    let mut end = 0;
    for byte in line.bytes() {
        match byte {
            b' ' => *column += 1,
            b'\t' => *column += 4 - *column % 4,
            _ => break,
        }
        end += 1;
    }
    (&line[end..], *column - start)
}

/// Strip blockquote markers plus their ONE optional whitespace column. A tab
/// after `>` may leave virtual indentation: do not erase it or the following spaces.
fn markdown_quote_content(line: &str) -> (&str, usize) {
    let mut column = 0;
    let (mut content, mut indent) = markdown_indent(line, &mut column);
    while indent <= 3 && content.starts_with('>') {
        content = &content[1..];
        column += 1;
        let mut remaining = 0;
        if content.starts_with(' ') {
            content = &content[1..];
            column += 1;
        } else if content.starts_with('\t') {
            content = &content[1..];
            let width = 4 - column % 4;
            column += width;
            remaining = width - 1;
        }
        let (rest, spacing) = markdown_indent(content, &mut column);
        content = rest;
        indent = remaining + spacing;
    }
    (content, indent)
}

/// Protect fenced blocks first, then matched equal-length backtick spans in prose.
fn markdown_code_mask(text: &str) -> Vec<bool> {
    let bytes = text.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut fence: Option<(u8, usize, usize)> = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        // Recognize common blockquote/list containers without rewriting their bytes.
        let (mut trimmed, indent) = markdown_quote_content(line);
        let mut container_indent = 0;
        if fence.is_none() && indent <= 3 {
            let list_prefix = if ["- ", "+ ", "* "]
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
            {
                2
            } else {
                let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
                if (1..=9).contains(&digits)
                    && (trimmed[digits..].starts_with(". ") || trimmed[digits..].starts_with(") "))
                {
                    digits + 2
                } else {
                    0
                }
            };
            if list_prefix > 0 {
                trimmed = &trimmed[list_prefix..];
                container_indent = list_prefix;
            }
        }
        let marker = trimmed.as_bytes().first().copied().unwrap_or(0);
        let run = trimmed.bytes().take_while(|b| *b == marker).count();
        let was_fenced = fence.is_some();
        if let Some((character, length, max_indent)) = fence {
            if indent <= max_indent
                && marker == character
                && run >= length
                && trimmed[run..].trim().is_empty()
            {
                fence = None;
            }
        } else if indent <= 3
            && matches!(marker, b'`' | b'~')
            && run >= 3
            && (marker != b'`' || !trimmed[run..].contains('`'))
        {
            fence = Some((marker, run, 3 + container_indent));
        }
        if was_fenced || fence.is_some() || indent >= 4 || line.starts_with('\t') {
            mask[offset..offset + line.len()].fill(true);
        }
        offset += line.len();
    }
    let mut i = 0;
    while i < bytes.len() {
        if mask[i] || bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let escapes = bytes[..i].iter().rev().take_while(|b| **b == b'\\').count();
        let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
        if escapes % 2 == 1 {
            i += run;
            continue;
        }
        let mut j = i + run;
        let mut end = None;
        while j < bytes.len() && !mask[j] {
            if bytes[j] == b'`' {
                let closing = bytes[j..].iter().take_while(|b| **b == b'`').count();
                if closing == run {
                    end = Some(j + closing);
                    break;
                }
                j += closing;
            } else {
                j += 1;
            }
        }
        if let Some(end) = end {
            mask[i..end].fill(true);
            i = end;
        } else {
            i += run;
        }
    }
    mask
}

/// Returns true if a quote at position `i` should be an opening quote.
/// Opening context: start of string, after whitespace, or after opening punctuation.
fn is_opening_context(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1];
    prev.is_whitespace() || matches!(prev, '(' | '[' | '{' | '\u{2014}' | '\u{2013}' | '\n')
}

#[cfg(test)]
#[path = "posts_safety_tests.rs"]
mod safety_tests;

#[cfg(test)]
mod tests {
    /// Reverse smart quote conversion: curly quotes back to straight, em dash to --, ellipsis to ...
    fn straightquotes(text: &str) -> String {
        text.replace(['\u{201C}', '\u{201D}'], "\"")
            .replace(['\u{2018}', '\u{2019}'], "'")
            .replace('\u{2014}', "--")
            .replace('\u{2026}', "...")
    }

    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_post(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    // Synthetic coverage adapted from Textorium's July core tests; see
    // docs/canonical-source-migration.md for provenance and the complete mapping.
    #[test]
    fn parse_frontmatter_well_formed() {
        let md = "---\ntitle: Hello\ndraft: true\ntags:\n  - a\n  - b\nweight: 5\n---\n\nBody.";
        let (fm, body, _, _) = parse_frontmatter(md).unwrap();
        assert_eq!(fm.get("title").unwrap(), "Hello");
        assert_eq!(fm.get("draft").unwrap(), &serde_json::json!(true));
        assert_eq!(fm.get("tags").unwrap(), &serde_json::json!(["a", "b"]));
        assert_eq!(fm.get("weight").unwrap(), &serde_json::json!(5));
        assert_eq!(body, "Body.");
    }

    #[test]
    fn parse_frontmatter_single_delimiter_is_not_frontmatter() {
        let md = "---\ntitle: Broken\nno closing fence";
        let (fm, body, _, _) = parse_frontmatter(md).unwrap();
        assert!(fm.is_empty());
        assert_eq!(body, md);
    }

    #[test]
    fn parse_frontmatter_empty_body() {
        let md = "---\ntitle: Only Meta\n---\n";
        let (fm, body, _, _) = parse_frontmatter(md).unwrap();
        assert_eq!(fm.get("title").unwrap(), "Only Meta");
        assert_eq!(body, "");
    }

    #[test]
    fn read_post_parses_rfc3339_date() {
        let f = create_temp_post("---\ntitle: T\ndate: 2026-07-03T09:30:00Z\n---\n\nBody.");
        let post = read_post(f.path()).unwrap();
        let expected = DateTime::parse_from_rfc3339("2026-07-03T09:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(post.date.unwrap(), expected);
    }

    #[test]
    fn read_post_parses_iso_date() {
        let f = create_temp_post("---\ntitle: T\ndate: 2026-07-03\n---\n\nBody.");
        let post = read_post(f.path()).unwrap();
        let expected = chrono::NaiveDate::from_ymd_opt(2026, 7, 3)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert_eq!(post.date.unwrap(), expected);
    }

    #[test]
    fn read_post_invalid_date_is_none() {
        let f = create_temp_post("---\ntitle: T\ndate: not-a-date\n---\n\nBody.");
        let post = read_post(f.path()).unwrap();
        assert!(post.date.is_none());
    }

    #[test]
    fn save_then_read_roundtrips_without_corruption() {
        let f = create_temp_post(
            "---\ntitle: Round Trip\ndate: 2026-07-03\ndraft: true\ntags:\n  - rust\n  - ssg\nweight: 7\n---\n\nOriginal body.\n",
        );
        let original = read_post(f.path()).unwrap();
        save_post(&original).unwrap();
        let reread = read_post(f.path()).unwrap();
        assert_eq!(reread.title, "Round Trip");
        assert!(reread.draft);
        assert_eq!(reread.tags, vec!["rust".to_string(), "ssg".to_string()]);
        assert_eq!(reread.date, original.date);
        assert_eq!(reread.content, original.content);
        assert_eq!(
            reread.frontmatter.get("weight").unwrap(),
            &serde_json::json!(7)
        );
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
        assert_eq!(slugify("Hello, World! It's 2026"), "hello-world-it-s-2026");
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
            template_fields: None,
        };

        let path = create_post(&config, &options).unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("my-test-post.md"));
        assert!(
            path.to_string_lossy().contains("content/dev/"),
            "Hugo post with --category dev should go in content/dev/, got: {}",
            path.display()
        );

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("title: \"My Test Post\""));
        assert!(content.contains("draft: true"));
        assert!(content.contains("categories: [\"dev\"]"));
        assert!(content.contains("tags: [\"rust\", \"tui\"]"));
    }

    #[test]
    fn test_create_post_hugo_default_section() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };

        let options = CreatePostOptions {
            title: "No Category Post".to_string(),
            category: None,
            tags: None,
            template_fields: None,
        };

        let path = create_post(&config, &options).unwrap();

        assert!(path.exists());
        assert!(
            path.to_string_lossy().contains("content/posts/"),
            "Hugo post without category should default to content/posts/, got: {}",
            path.display()
        );
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
            template_fields: None,
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
            template_fields: None,
        };

        let path = create_post(&config, &options).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        assert!(!content.contains("categories:"));
        assert!(!content.contains("tags:"));
        assert!(content.starts_with("---\n"));
        assert!(content.contains("---\n"));
    }

    #[test]
    fn test_create_post_rejects_path_traversal_category() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };

        let options = CreatePostOptions {
            title: "Evil Post".to_string(),
            category: Some("../../../tmp".to_string()),
            tags: None,
            template_fields: None,
        };

        let result = create_post(&config, &options);
        assert!(result.is_err(), "Should reject path traversal in category");
        assert!(
            result.unwrap_err().to_string().contains("Invalid category"),
            "Error should mention invalid category"
        );
    }

    #[test]
    fn test_create_post_escapes_yaml_special_chars() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };

        let options = CreatePostOptions {
            title: "Test Post".to_string(),
            category: Some("my\"category".to_string()),
            tags: Some(vec![
                "tag with \"quotes\"".to_string(),
                "normal-tag".to_string(),
            ]),
            template_fields: None,
        };

        let path = create_post(&config, &options).unwrap();
        let content = fs::read_to_string(&path).unwrap();

        // Verify the YAML is parseable
        let post = read_post(&path).unwrap();
        assert!(
            post.categories.iter().any(|c| c.contains("my")),
            "Category should be preserved"
        );
        assert_eq!(post.tags.len(), 2, "Both tags should be present");

        // Verify quotes are escaped in the raw content
        assert!(
            content.contains("\\\""),
            "Special chars should be escaped in frontmatter"
        );
    }

    #[test]
    fn test_multi_save_cycle_produces_correct_output() {
        let original = "---\ntitle: My post\ndate: 2025-01-15\ndraft: true\n---\n\nBody.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        // First edit: change title
        post.frontmatter.insert(
            "title".to_string(),
            serde_json::Value::String("Updated title".to_string()),
        );
        save_post(&post).unwrap();

        // Simulate what app.rs should do: reload baseline
        let reloaded = read_post(f.path()).unwrap();
        post = reloaded;

        // Second save with no further changes should trigger unchanged fast path
        save_post(&post).unwrap();
        let saved = fs::read_to_string(f.path()).unwrap();
        assert!(saved.contains("title: Updated title"));
        assert!(saved.contains("date: 2025-01-15"));

        // Third edit: change draft
        post.frontmatter
            .insert("draft".to_string(), serde_json::Value::Bool(false));
        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        assert!(saved.contains("title: Updated title"));
        assert!(saved.contains("draft: false"));
        assert!(saved.contains("date: 2025-01-15"));
    }

    #[test]
    fn test_save_does_not_inject_phantom_fields() {
        let original = "---\ntitle: Minimal post\n---\n\nJust a body.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        // Verify phantom fields are NOT in the frontmatter HashMap
        assert!(
            !post.frontmatter.contains_key("draft"),
            "draft should not be in frontmatter when absent from source"
        );
        assert!(
            !post.frontmatter.contains_key("content_type"),
            "content_type should not be in frontmatter when absent from source"
        );

        // Edit a different field to trigger the diff path in save_post
        post.frontmatter.insert(
            "title".to_string(),
            serde_json::Value::String("Updated minimal".to_string()),
        );
        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        assert!(saved.contains("title: Updated minimal"));
        assert!(
            !saved.contains("draft"),
            "draft should not be injected into saved file"
        );
        assert!(
            !saved.contains("content_type"),
            "content_type should not be injected into saved file"
        );
    }

    #[test]
    fn test_parse_toml_frontmatter() {
        let content = "+++\ntitle = \"My TOML Post\"\ndate = 2025-01-15T10:00:00Z\ndraft = true\ntags = [\"rust\", \"tui\"]\n+++\n\nBody text.\n";
        let f = create_temp_post(content);
        let post = read_post(f.path()).unwrap();

        assert_eq!(post.title, "My TOML Post");
        assert!(post.draft);
        assert_eq!(post.tags, vec!["rust", "tui"]);
        assert!(post.date.is_some());
        assert_eq!(post.format, FrontmatterFormat::Toml);
        assert_eq!(post.content, "Body text.");
    }

    #[test]
    fn test_save_unchanged_toml_post_is_byte_identical() {
        let original = "+++\ntitle = \"My TOML Post\"\ndate = 2025-01-15T10:00:00Z\ndraft = false\ntags = [\"rust\", \"tui\"]\n+++\n\nHello world.\n";
        let f = create_temp_post(original);
        let post = read_post(f.path()).unwrap();

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        assert_eq!(
            saved, original,
            "Unchanged TOML post should be byte-identical after save"
        );
    }

    #[test]
    fn test_save_toml_post_preserves_delimiters() {
        let original = "+++\ntitle = \"My TOML Post\"\ndraft = true\n+++\n\nBody.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        post.frontmatter.insert(
            "title".to_string(),
            serde_json::Value::String("Updated TOML".to_string()),
        );

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        assert!(
            saved.starts_with("+++\n"),
            "TOML post should preserve +++ delimiters, got: {}",
            saved
        );
        assert!(
            saved.contains("+++\n\n"),
            "Should have closing +++ delimiter"
        );
        assert!(saved.contains("title = \"Updated TOML\""));
        assert!(!saved.contains("---"), "Should not contain YAML delimiters");
    }

    #[test]
    fn test_save_toml_preserves_field_order_on_edit() {
        let original =
            "+++\ntitle = \"Original\"\ndate = 2025-01-15T10:00:00Z\ndraft = true\n+++\n\nBody.\n";
        let f = create_temp_post(original);
        let mut post = read_post(f.path()).unwrap();

        post.frontmatter.insert(
            "title".to_string(),
            serde_json::Value::String("New title".to_string()),
        );

        save_post(&post).unwrap();

        let saved = fs::read_to_string(f.path()).unwrap();
        let title_pos = saved.find("title =").unwrap();
        let date_pos = saved.find("date =").unwrap();
        assert!(
            title_pos < date_pos,
            "Field order should be preserved after edit"
        );
    }

    #[test]
    fn test_toml_datetime_parsed_correctly() {
        let content = "+++\ntitle = \"Date Test\"\ndate = 2025-03-15T14:30:00Z\n+++\n\nBody.\n";
        let f = create_temp_post(content);
        let post = read_post(f.path()).unwrap();

        assert!(post.date.is_some());
        let date = post.date.unwrap();
        assert_eq!(date.format("%Y-%m-%d").to_string(), "2025-03-15");
    }

    #[test]
    fn test_scan_posts_completes_with_symlink_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let content_dir = dir.path().join("content");
        fs::create_dir_all(&content_dir).unwrap();

        // Create a symlink cycle: content/loop -> content
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&content_dir, content_dir.join("loop")).unwrap();
        }

        // Create a valid post so we verify scanning still works
        fs::write(
            content_dir.join("test.md"),
            "---\ntitle: Test\n---\n\nBody.\n",
        )
        .unwrap();

        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };

        // This should complete without hanging thanks to max_depth(20)
        let result = scan_posts(&config).unwrap();
        assert!(
            !result.posts.is_empty(),
            "Should find at least the test post"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_scan_reports_unreadable_files() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let content_dir = dir.path().join("content");
        fs::create_dir_all(&content_dir).unwrap();

        // Create a valid post
        fs::write(
            content_dir.join("good.md"),
            "---\ntitle: Good\n---\n\nBody.\n",
        )
        .unwrap();

        // Create an unreadable subdirectory
        let bad_dir = content_dir.join("noaccess");
        fs::create_dir_all(&bad_dir).unwrap();
        fs::write(bad_dir.join("hidden.md"), "---\ntitle: Hidden\n---\n").unwrap();
        fs::set_permissions(&bad_dir, fs::Permissions::from_mode(0o000)).unwrap();

        let config = Config {
            site_path: dir.path().to_string_lossy().to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };

        let result = scan_posts(&config).unwrap();
        assert_eq!(result.posts.len(), 1, "Should find only the good post");
        assert!(
            !result.errors.is_empty(),
            "Should report IO errors for unreadable directory"
        );
        assert!(
            result
                .errors
                .iter()
                .any(|(_, msg)| msg.contains("IO error")),
            "Error should be tagged as IO error"
        );

        // Restore permissions so tempdir cleanup works
        fs::set_permissions(&bad_dir, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn test_no_frontmatter_still_works() {
        let content = "Just plain markdown with no frontmatter.\n";
        let f = create_temp_post(content);
        let post = read_post(f.path()).unwrap();

        assert_eq!(post.title, "Untitled");
        assert!(post.frontmatter.is_empty());
        assert_eq!(post.format, FrontmatterFormat::Yaml);
        assert_eq!(post.content, content);
    }

    #[test]
    fn test_smartquotes_double_quotes() {
        assert_eq!(
            smartquotes(r#"He said "hello" to her"#),
            "He said \u{201C}hello\u{201D} to her"
        );
    }

    #[test]
    fn test_smartquotes_apostrophe_contraction() {
        assert_eq!(smartquotes("don't"), "don\u{2019}t");
        assert_eq!(smartquotes("it's"), "it\u{2019}s");
    }

    #[test]
    fn test_smartquotes_nested_quotes() {
        assert_eq!(
            smartquotes(r#"She said "he said 'hello'""#),
            "She said \u{201C}he said \u{2018}hello\u{2019}\u{201D}"
        );
    }

    #[test]
    fn test_smartquotes_code_span_skip() {
        assert_eq!(smartquotes(r#"use `"raw"` here"#), "use `\"raw\"` here");
    }

    #[test]
    fn test_smartquotes_em_dash() {
        assert_eq!(smartquotes("word--word"), "word\u{2014}word");
    }

    #[test]
    fn test_smartquotes_ellipsis() {
        assert_eq!(smartquotes("wait..."), "wait\u{2026}");
    }

    #[test]
    fn test_straightquotes_roundtrip() {
        let input = r#"He said "don't wait..." -- she replied"#;
        let smart = smartquotes(input);
        assert_ne!(smart, input);
        assert_eq!(straightquotes(&smart), input);
    }
}
