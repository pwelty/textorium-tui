use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Per-site configuration entry stored in the multi-site config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteEntry {
    pub name: String,
    pub path: String,
    pub content_dir: String,
    pub ssg: SsgType,
}

/// On-disk multi-site config format.
/// `active_site` is the `name` of the currently active site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSiteConfig {
    pub sites: Vec<SiteEntry>,
    pub active_site: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor: Option<String>,
}

/// The active-site view. All existing code uses this struct unchanged.
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
    Astro,
}

impl SsgType {
    /// Get the default dev server URL for this SSG type
    pub fn dev_server_url(&self) -> &str {
        match self {
            SsgType::Hugo => "http://localhost:1313",
            SsgType::Jekyll => "http://localhost:4000",
            SsgType::Eleventy => "http://localhost:8080",
            SsgType::Astro => "http://localhost:4321",
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
        Self::load_from_file(&path)
    }

    /// Load config from a specific config directory
    pub fn load_from(config_dir: &Path) -> Result<Self> {
        let path = config_dir.join("config.json");
        Self::load_from_file(&path)
    }

    fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(path).context("Failed to read config file")?;
        let json: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse config file")?;

        // Detect format: multi-site has a "sites" array at top level
        if json.get("sites").is_some() {
            let multi: MultiSiteConfig = serde_json::from_value(json)
                .context("Failed to parse multi-site config")?;
            return multi.active_config();
        }

        // Legacy flat format: deserialize directly
        let config: Config =
            serde_json::from_str(&content).context("Failed to parse config file")?;
        Ok(config)
    }

    /// Save config to disk. Updates the active site in the multi-site config
    /// (or creates a new single-site multi-site config).
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        self.save_to_file(&path)
    }

    /// Save config to a specific config directory
    pub fn save_to(&self, config_dir: &Path) -> Result<()> {
        fs::create_dir_all(config_dir)?;
        let path = config_dir.join("config.json");
        self.save_to_file(&path)
    }

    fn save_to_file(&self, path: &Path) -> Result<()> {
        // Always write multi-site format (upsert active site)
        let mut multi = if path.exists() {
            let content = fs::read_to_string(path).unwrap_or_default();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if json.get("sites").is_some() {
                    serde_json::from_value::<MultiSiteConfig>(json).unwrap_or_else(|_| {
                        MultiSiteConfig {
                            sites: Vec::new(),
                            active_site: self.site_name.clone(),
                            editor: self.editor.clone(),
                        }
                    })
                } else {
                    // Legacy single-site — migrate
                    let legacy: Config = serde_json::from_str(&content).unwrap_or_else(|_| self.clone());
                    MultiSiteConfig {
                        sites: vec![SiteEntry {
                            name: legacy.site_name.clone(),
                            path: legacy.site_path.clone(),
                            content_dir: legacy.content_dir.clone(),
                            ssg: legacy.ssg.clone(),
                        }],
                        active_site: legacy.site_name.clone(),
                        editor: legacy.editor.clone(),
                    }
                }
            } else {
                MultiSiteConfig {
                    sites: Vec::new(),
                    active_site: self.site_name.clone(),
                    editor: self.editor.clone(),
                }
            }
        } else {
            MultiSiteConfig {
                sites: Vec::new(),
                active_site: self.site_name.clone(),
                editor: self.editor.clone(),
            }
        };

        // Upsert the active site entry
        let entry = SiteEntry {
            name: self.site_name.clone(),
            path: self.site_path.clone(),
            content_dir: self.content_dir.clone(),
            ssg: self.ssg.clone(),
        };
        if let Some(existing) = multi.sites.iter_mut().find(|s| s.name == self.site_name) {
            *existing = entry;
        } else {
            multi.sites.push(entry);
        }
        multi.active_site = self.site_name.clone();
        if self.editor.is_some() {
            multi.editor = self.editor.clone();
        }

        let content = serde_json::to_string_pretty(&multi)?;
        fs::write(path, content).context("Failed to write config file")?;
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

    // Astro: has astro.config.mjs or astro.config.ts
    if path.join("astro.config.mjs").exists() || path.join("astro.config.ts").exists() {
        return SsgType::Astro;
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
        SsgType::Astro => "src/content".to_string(),
    }
}

impl MultiSiteConfig {
    /// Return the active site as a `Config`, or an empty default if not found.
    pub fn active_config(&self) -> Result<Config> {
        if self.sites.is_empty() {
            return Ok(Config::default());
        }
        let entry = self
            .sites
            .iter()
            .find(|s| s.name == self.active_site)
            .or_else(|| self.sites.first())
            .unwrap(); // safe: not empty
        Ok(Config {
            site_name: entry.name.clone(),
            site_path: entry.path.clone(),
            content_dir: entry.content_dir.clone(),
            ssg: entry.ssg.clone(),
            editor: self.editor.clone(),
        })
    }

    /// Load multi-site config from the default location.
    pub fn load() -> Result<Self> {
        let path = Config::config_path()?;
        Self::load_from_file(&path)
    }

    /// Load multi-site config from a specific config directory.
    pub fn load_from(config_dir: &Path) -> Result<Self> {
        let path = config_dir.join("config.json");
        Self::load_from_file(&path)
    }

    fn load_from_file(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                sites: Vec::new(),
                active_site: String::new(),
                editor: None,
            });
        }
        let content = fs::read_to_string(path).context("Failed to read config file")?;
        let json: serde_json::Value =
            serde_json::from_str(&content).context("Failed to parse config")?;

        if json.get("sites").is_some() {
            let multi: MultiSiteConfig = serde_json::from_value(json)
                .context("Failed to parse multi-site config")?;
            return Ok(multi);
        }

        // Legacy flat format — wrap in multi
        let legacy: Config = serde_json::from_str(&content).context("Failed to parse config")?;
        if legacy.site_path.is_empty() {
            return Ok(Self {
                sites: Vec::new(),
                active_site: String::new(),
                editor: None,
            });
        }
        Ok(Self {
            active_site: legacy.site_name.clone(),
            editor: legacy.editor.clone(),
            sites: vec![SiteEntry {
                name: legacy.site_name,
                path: legacy.site_path,
                content_dir: legacy.content_dir,
                ssg: legacy.ssg,
            }],
        })
    }

    fn save_to_path(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).context("Failed to write config")?;
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let path = Config::config_path()?;
        self.save_to_path(&path)
    }

    fn save_to(&self, config_dir: &Path) -> Result<()> {
        fs::create_dir_all(config_dir)?;
        let path = config_dir.join("config.json");
        self.save_to_path(&path)
    }
}

// ---------------------------------------------------------------------------
// Site management API
// ---------------------------------------------------------------------------

/// Add a site by path. Optionally override the name (defaults to dir name).
/// Returns the final site name.
pub fn sites_add(site_path: &str, name_override: Option<&str>) -> Result<String> {
    sites_add_to(site_path, name_override, None)
}

pub fn sites_add_to(
    site_path: &str,
    name_override: Option<&str>,
    config_dir: Option<&Path>,
) -> Result<String> {
    let abs = fs::canonicalize(site_path).context("Could not resolve site path")?;
    let path_str = abs.to_string_lossy().to_string();

    let ssg = detect_ssg(&path_str);
    let content_dir = detect_content_dir(&path_str, &ssg);

    let auto_name = abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("site")
        .to_string();
    let name = name_override.unwrap_or(&auto_name).to_string();

    let mut multi = if let Some(dir) = config_dir {
        MultiSiteConfig::load_from(dir)?
    } else {
        MultiSiteConfig::load()?
    };

    if multi.sites.iter().any(|s| s.name == name) {
        anyhow::bail!("Site '{}' already exists. Use a different name with --name.", name);
    }

    multi.sites.push(SiteEntry {
        name: name.clone(),
        path: path_str,
        content_dir,
        ssg,
    });

    // If this is the first site, make it active
    if multi.sites.len() == 1 || multi.active_site.is_empty() {
        multi.active_site = name.clone();
    }

    if let Some(dir) = config_dir {
        multi.save_to(dir)?;
    } else {
        multi.save()?;
    }

    Ok(name)
}

/// List all registered sites. Returns (name, path, is_active).
pub fn sites_list() -> Result<Vec<(String, String, bool)>> {
    let multi = MultiSiteConfig::load()?;
    Ok(multi.sites.iter().map(|s| {
        (s.name.clone(), s.path.clone(), s.name == multi.active_site)
    }).collect())
}

/// Switch the active site by name.
pub fn sites_use(name: &str) -> Result<()> {
    sites_use_in(name, None)
}

pub fn sites_use_in(name: &str, config_dir: Option<&Path>) -> Result<()> {
    let mut multi = if let Some(dir) = config_dir {
        MultiSiteConfig::load_from(dir)?
    } else {
        MultiSiteConfig::load()?
    };

    if !multi.sites.iter().any(|s| s.name == name) {
        let names: Vec<&str> = multi.sites.iter().map(|s| s.name.as_str()).collect();
        anyhow::bail!(
            "Site '{}' not found. Available: {}",
            name,
            if names.is_empty() { "none".to_string() } else { names.join(", ") }
        );
    }

    multi.active_site = name.to_string();

    if let Some(dir) = config_dir {
        multi.save_to(dir)?;
    } else {
        multi.save()?;
    }
    Ok(())
}

/// Remove a site by name. Cannot remove the active site.
pub fn sites_remove(name: &str) -> Result<()> {
    let mut multi = MultiSiteConfig::load()?;

    if multi.active_site == name {
        anyhow::bail!("Cannot remove the active site '{}'. Switch to another site first with: textorium sites use <name>", name);
    }

    let before = multi.sites.len();
    multi.sites.retain(|s| s.name != name);

    if multi.sites.len() == before {
        anyhow::bail!("Site '{}' not found.", name);
    }

    multi.save()?;
    Ok(())
}

/// Configure textorium to use a site (existing single-site API, migrates to multi-site).
/// Adds the site if not already present, and makes it active.
pub fn configure_site(path: &str) -> Result<()> {
    let config = build_site_config(path)?;
    config.save()?;
    Ok(())
}

/// Configure textorium to use a site, saving config to a specific directory
pub fn configure_site_to(path: &str, config_dir: &Path) -> Result<()> {
    let config = build_site_config(path)?;
    config.save_to(config_dir)?;
    Ok(())
}

fn build_site_config(path: &str) -> Result<Config> {
    let path = fs::canonicalize(path).context("Could not resolve site path")?;
    let path_str = path.to_string_lossy().to_string();

    let ssg = detect_ssg(&path_str);
    let content_dir = detect_content_dir(&path_str, &ssg);

    let site_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("site")
        .to_string();

    Ok(Config {
        site_name,
        site_path: path_str,
        content_dir,
        ssg,
        editor: std::env::var("EDITOR").ok(),
    })
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

    #[test]
    fn test_detect_ssg_astro_mjs() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("astro.config.mjs"), "").unwrap();
        assert_eq!(detect_ssg(&dir.path().to_string_lossy()), SsgType::Astro);
    }

    #[test]
    fn test_detect_ssg_astro_ts() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("astro.config.ts"), "").unwrap();
        assert_eq!(detect_ssg(&dir.path().to_string_lossy()), SsgType::Astro);
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

    #[test]
    fn test_detect_content_dir_astro() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            detect_content_dir(&dir.path().to_string_lossy(), &SsgType::Astro),
            "src/content"
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
        assert_eq!(SsgType::Astro.dev_server_url(), "http://localhost:4321");
    }

    // --- multi-site ---

    fn make_site_dir(name: &str, config_file: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        // Write the named file so SSG detection picks up Hugo
        fs::write(dir.path().join(config_file), "").unwrap();
        let _ = name; // suppress unused warning
        dir
    }

    #[test]
    fn test_multi_site_add_and_list() {
        let config_dir = tempfile::tempdir().unwrap();
        let site_a = make_site_dir("blog", "hugo.toml");
        let site_b = make_site_dir("docs", "hugo.toml");

        sites_add_to(site_a.path().to_str().unwrap(), Some("blog"), Some(config_dir.path())).unwrap();
        sites_add_to(site_b.path().to_str().unwrap(), Some("docs"), Some(config_dir.path())).unwrap();

        let multi = MultiSiteConfig::load_from(config_dir.path()).unwrap();
        assert_eq!(multi.sites.len(), 2);
        assert_eq!(multi.active_site, "blog"); // first added is active
    }

    #[test]
    fn test_multi_site_use() {
        let config_dir = tempfile::tempdir().unwrap();
        let site_a = make_site_dir("blog", "hugo.toml");
        let site_b = make_site_dir("docs", "hugo.toml");

        sites_add_to(site_a.path().to_str().unwrap(), Some("blog"), Some(config_dir.path())).unwrap();
        sites_add_to(site_b.path().to_str().unwrap(), Some("docs"), Some(config_dir.path())).unwrap();

        sites_use_in("docs", Some(config_dir.path())).unwrap();

        let multi = MultiSiteConfig::load_from(config_dir.path()).unwrap();
        assert_eq!(multi.active_site, "docs");
    }

    #[test]
    fn test_multi_site_use_unknown_fails() {
        let config_dir = tempfile::tempdir().unwrap();
        let site_a = make_site_dir("blog", "hugo.toml");
        sites_add_to(site_a.path().to_str().unwrap(), Some("blog"), Some(config_dir.path())).unwrap();

        let result = sites_use_in("nonexistent", Some(config_dir.path()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_multi_site_migrate_legacy_format() {
        let config_dir = tempfile::tempdir().unwrap();
        // Write an old-style flat config
        let legacy = serde_json::json!({
            "site_name": "old-blog",
            "site_path": "/tmp/old-blog",
            "content_dir": "content",
            "ssg": "hugo"
        });
        fs::write(
            config_dir.path().join("config.json"),
            serde_json::to_string_pretty(&legacy).unwrap(),
        ).unwrap();

        let config = Config::load_from(config_dir.path()).unwrap();
        assert_eq!(config.site_name, "old-blog");
        assert_eq!(config.site_path, "/tmp/old-blog");
    }

    #[test]
    fn test_multi_site_active_config_reads_active() {
        let config_dir = tempfile::tempdir().unwrap();
        let site_a = make_site_dir("blog", "hugo.toml");
        let site_b = make_site_dir("docs", "hugo.toml");

        sites_add_to(site_a.path().to_str().unwrap(), Some("blog"), Some(config_dir.path())).unwrap();
        sites_add_to(site_b.path().to_str().unwrap(), Some("docs"), Some(config_dir.path())).unwrap();

        // Active is "blog" (first added)
        let config = Config::load_from(config_dir.path()).unwrap();
        assert_eq!(config.site_name, "blog");
    }

    #[test]
    fn test_multi_site_add_duplicate_fails() {
        let config_dir = tempfile::tempdir().unwrap();
        let site_a = make_site_dir("blog", "hugo.toml");

        sites_add_to(site_a.path().to_str().unwrap(), Some("blog"), Some(config_dir.path())).unwrap();
        let result = sites_add_to(site_a.path().to_str().unwrap(), Some("blog"), Some(config_dir.path()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }
}
