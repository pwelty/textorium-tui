use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub site_name: String,
    pub site_path: String,
    pub content_dir: String,
    pub ssg: SsgType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SsgType {
    Hugo,
    Jekyll,
    #[serde(rename = "11ty")]
    Eleventy,
}

impl SsgType {
    /// Get the default dev server URL for this SSG type
    pub fn dev_server_url(&self) -> &str {
        match self {
            SsgType::Hugo => "http://localhost:1313",
            SsgType::Jekyll => "http://localhost:4000",
            SsgType::Eleventy => "http://localhost:8080",
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            site_name: "site".to_string(),
            site_path: String::new(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            editor: None,
        }
    }
}

impl Config {
    /// Get the config file path
    fn config_path() -> Result<PathBuf> {
        // Use ~/.config/textorium to match Python version
        let home = std::env::var("HOME").context("Could not determine home directory")?;
        let config_dir = PathBuf::from(home).join(".config").join("textorium");
        fs::create_dir_all(&config_dir)?;
        Ok(config_dir.join("config.json"))
    }

    /// Load config from disk
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path).context("Failed to read config file")?;
        let config: Config =
            serde_json::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content).context("Failed to write config file")?;
        Ok(())
    }

    /// Get the content path (site_path + content_dir)
    pub fn content_path(&self) -> PathBuf {
        PathBuf::from(&self.site_path).join(&self.content_dir)
    }

    /// Get the preview URL for a post
    /// Constructs the URL by combining the SSG dev server URL with the post's relative path
    pub fn preview_url(&self, post_path: &Path) -> Option<String> {
        let content_path = self.content_path();

        // Get path relative to content directory (not site root)
        let relative_path = post_path.strip_prefix(&content_path).ok()?;

        // Convert to URL path (remove .md extension, convert to forward slashes)
        let url_path = relative_path
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");

        // Construct full URL with trailing slash for pretty URLs
        let base_url = self.ssg.dev_server_url();
        Some(format!("{}/{}/", base_url, url_path))
    }
}

/// Detect SSG type from directory structure
fn detect_ssg(path: &str) -> SsgType {
    let path = PathBuf::from(path);

    // Hugo: has hugo.toml, hugo.yaml, or config.toml
    if path.join("hugo.toml").exists()
        || path.join("hugo.yaml").exists()
        || path.join("config.toml").exists()
    {
        return SsgType::Hugo;
    }

    // Jekyll: has _config.yml
    if path.join("_config.yml").exists() {
        return SsgType::Jekyll;
    }

    // 11ty: has .eleventy.js or eleventy.config.js
    if path.join(".eleventy.js").exists() || path.join("eleventy.config.js").exists() {
        return SsgType::Eleventy;
    }

    // Default to Hugo
    SsgType::Hugo
}

/// Detect content directory
fn detect_content_dir(path: &str, ssg: &SsgType) -> String {
    let path = PathBuf::from(path);

    match ssg {
        SsgType::Hugo => "content".to_string(),
        SsgType::Jekyll => "_posts".to_string(),
        SsgType::Eleventy => {
            if path.join("posts").exists() {
                "posts".to_string()
            } else if path.join("src").exists() {
                "src".to_string()
            } else {
                "posts".to_string() // 11ty common default
            }
        }
    }
}

/// Configure textorium to use a site
pub fn configure_site(path: &str) -> Result<()> {
    let path = fs::canonicalize(path).context("Could not resolve site path")?;
    let path_str = path.to_string_lossy().to_string();

    let ssg = detect_ssg(&path_str);
    let content_dir = detect_content_dir(&path_str, &ssg);

    let site_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("site")
        .to_string();

    let config = Config {
        site_name,
        site_path: path_str,
        content_dir,
        ssg,
        editor: std::env::var("EDITOR").ok(),
    };

    config.save()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // --- detect_ssg ---

    #[test]
    fn test_detect_ssg_hugo_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hugo.toml"), "").unwrap();
        assert_eq!(detect_ssg(&dir.path().to_string_lossy()), SsgType::Hugo);
    }

    #[test]
    fn test_detect_ssg_hugo_config_toml() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("config.toml"), "").unwrap();
        assert_eq!(detect_ssg(&dir.path().to_string_lossy()), SsgType::Hugo);
    }

    #[test]
    fn test_detect_ssg_jekyll() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("_config.yml"), "").unwrap();
        assert_eq!(detect_ssg(&dir.path().to_string_lossy()), SsgType::Jekyll);
    }

    #[test]
    fn test_detect_ssg_eleventy() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".eleventy.js"), "").unwrap();
        assert_eq!(
            detect_ssg(&dir.path().to_string_lossy()),
            SsgType::Eleventy
        );
    }

    #[test]
    fn test_detect_ssg_hugo_wins_priority() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("hugo.toml"), "").unwrap();
        fs::write(dir.path().join("_config.yml"), "").unwrap();
        assert_eq!(detect_ssg(&dir.path().to_string_lossy()), SsgType::Hugo);
    }

    #[test]
    fn test_detect_ssg_empty_defaults_hugo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_ssg(&dir.path().to_string_lossy()), SsgType::Hugo);
    }

    // --- detect_content_dir ---

    #[test]
    fn test_detect_content_dir_hugo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_content_dir(&dir.path().to_string_lossy(), &SsgType::Hugo),
            "content"
        );
    }

    #[test]
    fn test_detect_content_dir_jekyll() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_content_dir(&dir.path().to_string_lossy(), &SsgType::Jekyll),
            "_posts"
        );
    }

    #[test]
    fn test_detect_content_dir_eleventy_with_posts() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("posts")).unwrap();
        assert_eq!(
            detect_content_dir(&dir.path().to_string_lossy(), &SsgType::Eleventy),
            "posts"
        );
    }

    #[test]
    fn test_detect_content_dir_eleventy_with_src() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        assert_eq!(
            detect_content_dir(&dir.path().to_string_lossy(), &SsgType::Eleventy),
            "src"
        );
    }

    #[test]
    fn test_detect_content_dir_eleventy_default() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_content_dir(&dir.path().to_string_lossy(), &SsgType::Eleventy),
            "posts"
        );
    }

    // --- preview_url ---

    #[test]
    fn test_preview_url_hugo() {
        let config = Config {
            site_path: "/site".to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };
        let url = config
            .preview_url(Path::new("/site/content/posts/my-post.md"))
            .unwrap();
        assert_eq!(url, "http://localhost:1313/posts/my-post/");
    }

    #[test]
    fn test_preview_url_nested_path() {
        let config = Config {
            site_path: "/site".to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };
        let url = config
            .preview_url(Path::new("/site/content/blog/2026/post.md"))
            .unwrap();
        assert_eq!(url, "http://localhost:1313/blog/2026/post/");
    }

    #[test]
    fn test_preview_url_outside_content_returns_none() {
        let config = Config {
            site_path: "/site".to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };
        assert!(config.preview_url(Path::new("/other/path.md")).is_none());
    }

    // --- content_path ---

    #[test]
    fn test_content_path() {
        let config = Config {
            site_path: "/home/user/blog".to_string(),
            content_dir: "content".to_string(),
            ssg: SsgType::Hugo,
            ..Default::default()
        };
        assert_eq!(
            config.content_path(),
            PathBuf::from("/home/user/blog/content")
        );
    }

    // --- dev_server_url ---

    #[test]
    fn test_dev_server_urls() {
        assert_eq!(SsgType::Hugo.dev_server_url(), "http://localhost:1313");
        assert_eq!(SsgType::Jekyll.dev_server_url(), "http://localhost:4000");
        assert_eq!(SsgType::Eleventy.dev_server_url(), "http://localhost:8080");
    }
}
