use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use super::config::Config;

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
fn parse_frontmatter(content: &str) -> Result<(HashMap<String, serde_json::Value>, String)> {
    if !content.starts_with("---") {
        return Ok((HashMap::new(), content.to_string()));
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return Ok((HashMap::new(), content.to_string()));
    }

    let frontmatter: Frontmatter = serde_yaml::from_str(parts[1])
        .context("Failed to parse YAML frontmatter")?;

    let body = parts[2].trim().to_string();

    // Convert to HashMap
    let mut fm_map = HashMap::new();
    fm_map.insert("title".to_string(), serde_json::Value::String(frontmatter.title.clone()));

    if let Some(date) = &frontmatter.date {
        fm_map.insert("date".to_string(), serde_json::Value::String(date.clone()));
    }

    fm_map.insert("draft".to_string(), serde_json::Value::Bool(frontmatter.draft));
    fm_map.insert("content_type".to_string(), serde_json::Value::String(frontmatter.content_type.clone()));

    if !frontmatter.categories.is_empty() {
        let cats: Vec<serde_json::Value> = frontmatter.categories
            .iter()
            .map(|c| serde_json::Value::String(c.clone()))
            .collect();
        fm_map.insert("categories".to_string(), serde_json::Value::Array(cats));
    }

    if !frontmatter.tags.is_empty() {
        let tags: Vec<serde_json::Value> = frontmatter.tags
            .iter()
            .map(|t| serde_json::Value::String(t.clone()))
            .collect();
        fm_map.insert("tags".to_string(), serde_json::Value::Array(tags));
    }

    // Add extra fields
    for (key, value) in frontmatter.extra {
        fm_map.insert(key, value);
    }

    Ok((fm_map, body))
}

/// Read a single post from a file
pub fn read_post(path: &Path) -> Result<Post> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read post: {}", path.display()))?;

    let (frontmatter, body) = parse_frontmatter(&content)?;

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
        frontmatter,
    })
}

/// Scan directory for all markdown posts
pub fn scan_posts(config: &Config) -> Result<Vec<Post>> {
    let content_path = config.content_path();

    if !content_path.exists() {
        return Ok(Vec::new());
    }

    let mut posts = Vec::new();

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

        // Try to read the post
        if let Ok(post) = read_post(path) {
            posts.push(post);
        }
    }

    // Sort by date, newest first
    posts.sort_by(|a, b| b.date.cmp(&a.date));

    Ok(posts)
}

/// Save a post back to disk
pub fn save_post(post: &Post) -> Result<()> {
    // Reconstruct frontmatter
    let mut fm_lines = vec!["---".to_string()];

    // Serialize frontmatter map as YAML
    let yaml_str = serde_yaml::to_string(&post.frontmatter)?;
    fm_lines.push(yaml_str.trim().to_string());
    fm_lines.push("---".to_string());

    // Combine with content
    let full_content = format!("{}\n\n{}", fm_lines.join("\n"), post.content);

    fs::write(&post.path, full_content)
        .with_context(|| format!("Failed to write post: {}", post.path.display()))?;

    Ok(())
}
