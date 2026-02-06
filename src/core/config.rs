use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub site_name: String,
    pub site_path: String,
    pub content_dir: String,
    pub ssg: SsgType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notion_database_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notion_token: Option<String>,
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
            notion_database_id: None,
            notion_token: None,
        }
    }
}

impl Config {
    /// Get the config file path
    fn config_path() -> Result<PathBuf> {
        // Use ~/.config/textorium to match Python version
        let home = std::env::var("HOME")
            .context("Could not determine home directory")?;
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

        let content = fs::read_to_string(&path)
            .context("Failed to read config file")?;
        let config: Config = serde_json::from_str(&content)
            .context("Failed to parse config file")?;
        Ok(config)
    }

    /// Save config to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)
            .context("Failed to write config file")?;
        Ok(())
    }

    /// Get the content path (site_path + content_dir)
    pub fn content_path(&self) -> PathBuf {
        PathBuf::from(&self.site_path).join(&self.content_dir)
    }

    /// Get the preview URL for a post
    /// Constructs the URL by combining the SSG dev server URL with the post's relative path
    pub fn preview_url(&self, post_path: &PathBuf) -> Option<String> {
        let site_path = PathBuf::from(&self.site_path);

        // Get path relative to site root
        let relative_path = post_path.strip_prefix(&site_path).ok()?;

        // Convert to URL path (remove .md extension, convert to forward slashes)
        let url_path = relative_path
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");

        // Construct full URL
        let base_url = self.ssg.dev_server_url();
        Some(format!("{}/{}", base_url, url_path))
    }
}

/// Detect SSG type from directory structure
fn detect_ssg(path: &str) -> SsgType {
    let path = PathBuf::from(path);

    // Hugo: has hugo.toml, hugo.yaml, or config.toml
    if path.join("hugo.toml").exists()
        || path.join("hugo.yaml").exists()
        || path.join("config.toml").exists() {
        return SsgType::Hugo;
    }

    // Jekyll: has _config.yml
    if path.join("_config.yml").exists() {
        return SsgType::Jekyll;
    }

    // 11ty: has .eleventy.js or eleventy.config.js
    if path.join(".eleventy.js").exists()
        || path.join("eleventy.config.js").exists() {
        return SsgType::Eleventy;
    }

    // Default to Hugo
    SsgType::Hugo
}

/// Detect content directory
fn detect_content_dir(path: &str, ssg: &SsgType) -> String {
    let path = PathBuf::from(path);

    match ssg {
        SsgType::Hugo => {
            if path.join("content").exists() {
                "content".to_string()
            } else {
                "content".to_string() // Hugo default
            }
        }
        SsgType::Jekyll => {
            if path.join("_posts").exists() {
                "_posts".to_string()
            } else {
                "_posts".to_string() // Jekyll default
            }
        }
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
    let path = fs::canonicalize(path)
        .context("Could not resolve site path")?;
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
        ..Default::default()
    };

    config.save()?;
    Ok(())
}
