use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::config::{Config, SsgType};

#[derive(Debug, Clone, PartialEq)]
pub enum FrontmatterFormat {
    Yaml,
    Toml,
}

impl Default for FrontmatterFormat {
    fn default() -> Self {
        FrontmatterFormat::Yaml
    }
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
    /// Whether the frontmatter uses YAML (---) or TOML (+++) delimiters
    #[serde(skip)]
    pub format: FrontmatterFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Frontmatter {
    #[serde(default)]
    title: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    draft: Option<bool>,
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

/// Parse frontmatter and body from markdown content
/// Returns (parsed HashMap, body text, raw frontmatter text, format)
fn parse_frontmatter(
    content: &str,
) -> Result<(HashMap<String, serde_json::Value>, String, String, FrontmatterFormat)> {
    // Detect TOML frontmatter (+++...+++)
    if content.starts_with("+++") {
        let parts: Vec<&str> = content.splitn(3, "+++").collect();
        if parts.len() < 3 {
            return Ok((HashMap::new(), content.to_string(), String::new(), FrontmatterFormat::Toml));
        }

        let raw_toml = parts[1].to_string();
        let body = parts[2].trim().to_string();

        let toml_table: toml::Table =
            toml::from_str(parts[1]).context("Failed to parse TOML frontmatter")?;

        // Convert TOML table to serde_json HashMap
        let mut fm_map: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, value) in &toml_table {
            fm_map.insert(key.clone(), toml_value_to_json(value));
        }

        // Normalize: merge singular "category" into "categories" array (same as YAML path)
        if let Some(cat) = fm_map.remove("category") {
            if !fm_map.contains_key("categories") {
                if let Some(s) = cat.as_str() {
                    fm_map.insert(
                        "categories".to_string(),
                        serde_json::Value::Array(vec![serde_json::Value::String(s.to_string())]),
                    );
                }
            }
        }

        return Ok((fm_map, body, raw_toml, FrontmatterFormat::Toml));
    }

    // Detect YAML frontmatter (---...---)
    if !content.starts_with("---") {
        return Ok((HashMap::new(), content.to_string(), String::new(), FrontmatterFormat::Yaml));
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return Ok((HashMap::new(), content.to_string(), String::new(), FrontmatterFormat::Yaml));
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

    if let Some(draft) = frontmatter.draft {
        fm_map.insert("draft".to_string(), serde_json::Value::Bool(draft));
    }

    if !frontmatter.content_type.is_empty() {
        fm_map.insert(
            "content_type".to_string(),
            serde_json::Value::String(frontmatter.content_type.clone()),
        );
    }

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
        self.title = self.frontmatter.get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled")
            .to_string();
        self.date = self.frontmatter.get("date")
            .and_then(|v| v.as_str())
            .and_then(parse_date_string);
        self.draft = self.frontmatter.get("draft")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.content_type = self.frontmatter.get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        self.tags = self.frontmatter.get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        self.categories = self.frontmatter.get("categories")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
    }
}

/// Read a single post from a file
pub fn read_post(path: &Path) -> Result<Post> {
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
    let content_path = config.content_path();

    if !content_path.exists() {
        return Ok(ScanResult {
            posts: Vec::new(),
            errors: Vec::new(),
        });
    }

    let mut posts = Vec::new();
    let mut errors = Vec::new();

    for entry_result in WalkDir::new(&content_path)
        .follow_links(true)
        .max_depth(20)
        .into_iter()
    {
        let entry = match entry_result {
            Ok(e) => e,
            Err(e) => {
                let path = e.path().unwrap_or(std::path::Path::new("<unknown>")).to_path_buf();
                errors.push((path, format!("IO error: {}", e)));
                continue;
            }
        };
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

/// Serialize a serde_json::Value as inline TOML suitable for frontmatter
fn value_to_toml_inline(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
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
                    !before_eq.is_empty() && before_eq.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
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
/// fsync, then rename over the target. Prevents data loss on crash/power loss.
fn atomic_write(path: &Path, content: &str) -> Result<()> {
    use std::io::Write;

    let tmp_path = path.with_extension("md.tmp");
    let result = (|| -> Result<()> {
        let mut f = fs::File::create(&tmp_path)
            .with_context(|| format!("Failed to create temp file: {}", tmp_path.display()))?;
        f.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write temp file: {}", tmp_path.display()))?;
        f.sync_all()
            .with_context(|| format!("Failed to sync temp file: {}", tmp_path.display()))?;
        fs::rename(&tmp_path, path)
            .with_context(|| format!("Failed to rename {} to {}", tmp_path.display(), path.display()))?;
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
    let (open_delim, close_delim) = match post.format {
        FrontmatterFormat::Yaml => ("---", "---"),
        FrontmatterFormat::Toml => ("+++", "+++"),
    };

    // If frontmatter is unchanged, write back the original file exactly
    if post.frontmatter == post.original_frontmatter {
        let full_content = format!("{}{}{}\n\n{}\n", open_delim, post.raw_frontmatter, close_delim, post.content);
        atomic_write(&post.path, &full_content)?;
        return Ok(());
    }

    // Frontmatter was modified — patch the raw text
    let raw_lines: Vec<&str> = post.raw_frontmatter.lines().collect();
    let mut result_lines: Vec<String> = Vec::new();
    let mut processed_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    let is_toml = post.format == FrontmatterFormat::Toml;

    // Process existing lines, replacing modified values and skipping deleted keys
    let mut i = 0;
    while i < raw_lines.len() {
        let line = raw_lines[i];
        let trimmed = line.trim();

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
    let frontmatter_text = result_lines.join("\n");
    let full_content = format!("{}\n{}\n{}\n\n{}\n", open_delim, frontmatter_text, close_delim, post.content);

    atomic_write(&post.path, &full_content)?;

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
            lines.push(format!("categories: [\"{}\"]\n", escaped).trim_end_matches('\n').to_string());
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
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
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
    let mut in_code = false;

    while i < len {
        let ch = chars[i];

        // Toggle code span tracking
        if ch == '`' {
            in_code = !in_code;
            result.push(ch);
            i += 1;
            continue;
        }

        // Skip conversion inside code spans
        if in_code {
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
mod tests {
    /// Reverse smart quote conversion: curly quotes back to straight, em dash to --, ellipsis to ...
    fn straightquotes(text: &str) -> String {
        text.replace('\u{201C}', "\"")
            .replace('\u{201D}', "\"")
            .replace('\u{2018}', "'")
            .replace('\u{2019}', "'")
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
        post.original_frontmatter = reloaded.original_frontmatter;
        post.raw_frontmatter = reloaded.raw_frontmatter;

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
        assert!(saved.contains("+++\n\n"), "Should have closing +++ delimiter");
        assert!(saved.contains("title = \"Updated TOML\""));
        assert!(!saved.contains("---"), "Should not contain YAML delimiters");
    }

    #[test]
    fn test_save_toml_preserves_field_order_on_edit() {
        let original = "+++\ntitle = \"Original\"\ndate = 2025-01-15T10:00:00Z\ndraft = true\n+++\n\nBody.\n";
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
            result.errors.iter().any(|(_, msg)| msg.contains("IO error")),
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
        assert!(post.content.starts_with("Just plain markdown with no frontmatter."));
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
        assert_eq!(
            smartquotes(r#"use `"raw"` here"#),
            "use `\"raw\"` here"
        );
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
