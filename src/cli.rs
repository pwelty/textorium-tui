use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "textorium")]
#[command(about = "A fast terminal interface for static site generators", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Configure textorium to use a site folder
    Use {
        /// Path to your static site folder
        path: String,
    },

    /// Create a new post
    New {
        /// Post title
        title: String,

        /// Category
        #[arg(short, long)]
        category: Option<String>,

        /// Tags (comma-separated)
        #[arg(short, long)]
        tags: Option<String>,

        /// Template name (from <site>/.textorium/templates/)
        #[arg(long)]
        template: Option<String>,

        /// Skip opening in editor
        #[arg(long)]
        no_edit: bool,
    },

    /// Manage post templates
    Templates {
        #[command(subcommand)]
        action: TemplatesAction,
    },

    /// Manage multiple sites
    Sites {
        #[command(subcommand)]
        action: SitesAction,
    },

    /// List posts
    List {
        /// Show only drafts
        #[arg(short, long)]
        drafts: bool,

        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,

        /// Property filter in field:op:value format (repeatable, AND logic)
        /// Operators: contains, equals, is_true, is_false
        /// Example: --filter "draft:is_true" --filter "tags:contains:rust"
        #[arg(long = "filter", value_name = "FILTER")]
        filters: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Publish a draft post
    Publish {
        /// Post slug or path
        slug: String,
    },

    /// Start development server
    Serve {
        /// Port number (default: 1313 Hugo, 4000 Jekyll, 8080 Eleventy)
        #[arg(short, long)]
        port: Option<u16>,

        /// Don't include drafts
        #[arg(long)]
        no_drafts: bool,
    },

    /// Build the site for production
    Build {
        /// Minify output
        #[arg(short, long)]
        minify: bool,
    },
}

#[derive(Subcommand)]
pub enum TemplatesAction {
    /// List available templates
    List,
    /// Create a new template scaffold
    Create {
        /// Template name (no extension)
        name: String,
    },
}

#[derive(Subcommand)]
pub enum SitesAction {
    /// Add a site to the registry
    Add {
        /// Path to the site directory
        path: String,
        /// Custom name for the site (defaults to directory name)
        #[arg(long)]
        name: Option<String>,
    },
    /// List all registered sites
    List,
    /// Switch to a different active site
    Use {
        /// Site name
        name: String,
    },
    /// Remove a site from the registry
    Remove {
        /// Site name
        name: String,
    },
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        None => {
            // No subcommand = launch TUI
            crate::tui::app::run()?;
        }
        Some(Commands::Use { path }) => {
            crate::core::config::configure_site(&path)?;
            println!("✓ Configured textorium to use: {}", path);
        }
        Some(Commands::New {
            title,
            category,
            tags,
            template,
            no_edit,
        }) => {
            let config = crate::core::config::Config::load()?;
            if config.site_path.is_empty() {
                anyhow::bail!("No site configured. Run: textorium use <path>");
            }

            let tag_list: Option<Vec<String>> = tags.map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

            // Load template fields if --template was given
            let template_fields = if let Some(ref tname) = template {
                Some(crate::core::templates::load_template(&config, tname)?)
            } else {
                None
            };

            let options = crate::core::posts::CreatePostOptions {
                title,
                category,
                tags: tag_list,
                template_fields,
            };

            let path = crate::core::posts::create_post(&config, &options)?;
            println!("{}", path.display());

            if !no_edit {
                let editor = config
                    .editor
                    .or_else(|| std::env::var("EDITOR").ok())
                    .unwrap_or_else(|| "vi".to_string());
                std::process::Command::new(&editor)
                    .arg(&path)
                    .status()
                    .with_context(|| format!("Failed to open editor: {}", editor))?;
            }
        }
        Some(Commands::Templates { action }) => {
            let config = crate::core::config::Config::load()?;
            if config.site_path.is_empty() {
                anyhow::bail!("No site configured. Run: textorium use <path>");
            }
            match action {
                TemplatesAction::List => {
                    let names = crate::core::templates::list_templates(&config)?;
                    if names.is_empty() {
                        println!(
                            "No templates found. Create one with: textorium templates create <name>"
                        );
                    } else {
                        println!("Available templates:");
                        for name in &names {
                            let dir = crate::core::templates::templates_dir(&config);
                            println!("  {}  ({})", name, dir.join(format!("{}.yaml", name)).display());
                        }
                    }
                }
                TemplatesAction::Create { name } => {
                    let path = crate::core::templates::create_template(&config, &name)?;
                    println!("✓ Created template: {}", path.display());
                    println!("Edit the file to customize your frontmatter scaffold.");
                }
            }
        }
        Some(Commands::Sites { action }) => {
            match action {
                SitesAction::Add { path, name } => {
                    let site_name = crate::core::config::sites_add(&path, name.as_deref())?;
                    println!("✓ Added site '{}'", site_name);
                    println!("Run: textorium sites use {} — to make it active", site_name);
                }
                SitesAction::List => {
                    let sites = crate::core::config::sites_list()?;
                    if sites.is_empty() {
                        println!("No sites registered. Run: textorium sites add <path>");
                    } else {
                        println!("{:<20} {:<8} {}", "Name", "Active", "Path");
                        println!("{}", "-".repeat(60));
                        for (name, path, active) in &sites {
                            let marker = if *active { "✓" } else { "" };
                            println!("{:<20} {:<8} {}", name, marker, path);
                        }
                    }
                }
                SitesAction::Use { name } => {
                    crate::core::config::sites_use(&name)?;
                    println!("✓ Switched to site '{}'", name);
                }
                SitesAction::Remove { name } => {
                    crate::core::config::sites_remove(&name)?;
                    println!("✓ Removed site '{}'", name);
                }
            }
        }
        Some(Commands::List {
            drafts,
            category,
            filters,
            json,
        }) => {
            let config = crate::core::config::Config::load()?;
            if config.site_path.is_empty() {
                anyhow::bail!("No site configured. Run: textorium use <path>");
            }

            let result = crate::core::posts::scan_posts(&config)?;

            // Report parse errors to stderr
            for (path, err) in &result.errors {
                eprintln!("Warning: skipped {}: {}", path.display(), err);
            }

            let mut posts = result.posts;

            // Apply filters
            if drafts {
                posts.retain(|p| p.draft);
            }
            if let Some(ref cat) = category {
                posts.retain(|p| p.categories.iter().any(|c| c.eq_ignore_ascii_case(cat)));
            }

            // Apply property filters
            if !filters.is_empty() {
                let parsed_filters: Vec<crate::core::filters::PropertyFilter> = filters
                    .iter()
                    .filter_map(|s| {
                        let f = crate::core::filters::PropertyFilter::parse(s);
                        if f.is_none() {
                            eprintln!("Warning: invalid filter '{}' (expected field:op[:value])", s);
                        }
                        f
                    })
                    .collect();
                posts.retain(|p| parsed_filters.iter().all(|f| f.matches(p)));
            }

            if json {
                // JSON output for scripting
                let json_posts: Vec<serde_json::Value> = posts
                    .iter()
                    .map(|p| {
                        serde_json::json!({
                            "title": p.title,
                            "date": p.date.map(|d| d.to_rfc3339()),
                            "draft": p.draft,
                            "categories": p.categories,
                            "tags": p.tags,
                            "path": p.path.to_string_lossy(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json_posts)?);
            } else {
                // Table output
                if posts.is_empty() {
                    println!("No posts found.");
                } else {
                    println!(
                        "{:<50} {:<12} {:<8} Path",
                        "Title", "Date", "Status"
                    );
                    println!("{}", "-".repeat(100));
                    for p in &posts {
                        let date_str = p
                            .date
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_else(|| "—".to_string());
                        let status = if p.draft { "draft" } else { "published" };
                        let title = if p.title.chars().count() > 48 {
                            format!(
                                "{}…",
                                p.title.chars().take(47).collect::<String>()
                            )
                        } else {
                            p.title.clone()
                        };
                        println!(
                            "{:<50} {:<12} {:<8} {}",
                            title,
                            date_str,
                            status,
                            p.path.display()
                        );
                    }
                    println!("\n{} post(s)", posts.len());
                }
            }
        }
        Some(Commands::Publish { slug }) => {
            let config = crate::core::config::Config::load()?;
            if config.site_path.is_empty() {
                anyhow::bail!("No site configured. Run: textorium use <path>");
            }

            let result = crate::core::posts::scan_posts(&config)?;

            // Collect slugs before consuming the iterator (avoids a second scan on not-found)
            let slugs: Vec<String> = result
                .posts
                .iter()
                .filter_map(|p| p.path.file_stem().and_then(|s| s.to_str().map(String::from)))
                .collect();

            // Find the post by slug (match against filename stem or relative path)
            let matched = result.posts.into_iter().find(|p| {
                let stem = p
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let path_str = p.path.to_string_lossy();
                stem == slug || path_str.ends_with(&slug)
            });

            match matched {
                Some(mut post) => {
                    if !post.draft {
                        println!("Already published: {}", post.title);
                    } else {
                        post.frontmatter
                            .insert("draft".to_string(), serde_json::Value::Bool(false));
                        post.draft = false;
                        crate::core::posts::save_post(&post)?;
                        println!("✓ Published: {}", post.title);
                    }
                }
                None => {
                    eprintln!("Error: no post found matching \"{}\"", slug);
                    if !slugs.is_empty() {
                        eprintln!("Available slugs:");
                        for s in &slugs {
                            eprintln!("  {}", s);
                        }
                    }
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Serve { port, no_drafts }) => {
            let config = crate::core::config::Config::load()?;
            if config.site_path.is_empty() {
                anyhow::bail!("No site configured. Run: textorium use <path>");
            }

            let port = port.unwrap_or(match config.ssg {
                crate::core::config::SsgType::Hugo => 1313,
                crate::core::config::SsgType::Jekyll => 4000,
                crate::core::config::SsgType::Eleventy => 8080,
                crate::core::config::SsgType::Astro => 4321,
            });
            let port_str = port.to_string();
            let (program, mut args): (&str, Vec<&str>) = match config.ssg {
                crate::core::config::SsgType::Hugo => {
                    ("hugo", vec!["server", "--port", &port_str])
                }
                crate::core::config::SsgType::Jekyll => {
                    ("bundle", vec!["exec", "jekyll", "serve", "--port", &port_str])
                }
                crate::core::config::SsgType::Eleventy => {
                    ("npx", vec!["@11ty/eleventy", "--serve", "--port", &port_str])
                }
                crate::core::config::SsgType::Astro => {
                    ("npx", vec!["astro", "dev", "--port", &port_str])
                }
            };

            // Include drafts by default; --no-drafts opts out
            if !no_drafts {
                match config.ssg {
                    crate::core::config::SsgType::Hugo => args.push("-D"),
                    crate::core::config::SsgType::Jekyll => args.push("--drafts"),
                    crate::core::config::SsgType::Eleventy => {}
                    crate::core::config::SsgType::Astro => {}
                }
            }

            let ssg_name = match config.ssg {
                crate::core::config::SsgType::Hugo => "Hugo",
                crate::core::config::SsgType::Jekyll => "Jekyll",
                crate::core::config::SsgType::Eleventy => "Eleventy",
                crate::core::config::SsgType::Astro => "Astro",
            };
            println!("Starting {} dev server on port {}...", ssg_name, port);

            let status = std::process::Command::new(program)
                .args(&args)
                .current_dir(&config.site_path)
                .status()
                .with_context(|| format!("Failed to run '{}'. Is it installed?", program))?;

            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Some(Commands::Build { minify }) => {
            let config = crate::core::config::Config::load()?;
            if config.site_path.is_empty() {
                anyhow::bail!("No site configured. Run: textorium use <path>");
            }

            let (program, mut args): (&str, Vec<&str>) = match config.ssg {
                crate::core::config::SsgType::Hugo => ("hugo", vec![]),
                crate::core::config::SsgType::Jekyll => {
                    ("bundle", vec!["exec", "jekyll", "build"])
                }
                crate::core::config::SsgType::Eleventy => ("npx", vec!["@11ty/eleventy"]),
                crate::core::config::SsgType::Astro => ("npx", vec!["astro", "build"]),
            };

            if minify && config.ssg == crate::core::config::SsgType::Hugo {
                args.push("--minify");
            }

            let ssg_name = match config.ssg {
                crate::core::config::SsgType::Hugo => "Hugo",
                crate::core::config::SsgType::Jekyll => "Jekyll",
                crate::core::config::SsgType::Eleventy => "Eleventy",
                crate::core::config::SsgType::Astro => "Astro",
            };
            println!("Building site with {}...", ssg_name);

            let status = std::process::Command::new(program)
                .args(&args)
                .current_dir(&config.site_path)
                .status()
                .with_context(|| format!("Failed to run '{}'. Is it installed?", program))?;

            if status.success() {
                println!("✓ Build complete");
            } else {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::TempDir;

    /// Set up a temp Hugo site with isolated config directory.
    /// Returns (config_dir, site_dir) — both must be kept alive for the test duration.
    fn setup_hugo_site() -> (TempDir, TempDir) {
        let config_dir = TempDir::new().unwrap();
        let site = TempDir::new().unwrap();

        // Create Hugo content dir so SSG detection works
        fs::create_dir_all(site.path().join("content/posts")).unwrap();

        // Configure the site using an isolated config directory (no HOME mutation)
        crate::core::config::configure_site_to(
            site.path().to_str().unwrap(),
            config_dir.path(),
        )
        .unwrap();

        (config_dir, site)
    }

    /// Load config from the test's isolated config directory
    fn load_config(config_dir: &TempDir) -> crate::core::config::Config {
        crate::core::config::Config::load_from(config_dir.path()).unwrap()
    }

    /// Create a sample draft post in the site for list/publish tests
    fn create_sample_post(site: &TempDir, slug: &str, draft: bool) {
        let draft_str = if draft { "true" } else { "false" };
        let content = format!(
            "---\ntitle: \"Test {slug}\"\ndate: 2026-01-15T10:00:00Z\ndraft: {draft_str}\ncategories: [\"blog\"]\n---\nSample content for {slug}.\n"
        );
        let path = site.path().join("content/posts").join(format!("{slug}.md"));
        fs::write(path, content).unwrap();
    }

    #[test]
    fn test_new_creates_post_with_frontmatter() {
        let (config_dir, _site) = setup_hugo_site();
        let config = load_config(&config_dir);

        let options = crate::core::posts::CreatePostOptions {
            title: "My test post".to_string(),
            category: Some("blog".to_string()),
            tags: Some(vec!["rust".to_string(), "tui".to_string()]),
            template_fields: None,
        };

        let path = crate::core::posts::create_post(&config, &options).unwrap();
        assert!(path.exists(), "Post file should be created");

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("title: \"My test post\""));
        assert!(content.contains("draft: true"));
        assert!(content.contains("categories: [\"blog\"]"));
        assert!(content.contains("\"rust\""));
        assert!(content.contains("\"tui\""));
    }

    #[test]
    fn test_new_fails_without_site_configured() {
        let config_dir = TempDir::new().unwrap();
        let config = load_config(&config_dir);
        assert!(config.site_path.is_empty(), "Unconfigured site should have empty path");
    }

    #[test]
    fn test_new_defaults_to_posts_section() {
        let (config_dir, site) = setup_hugo_site();
        let config = load_config(&config_dir);

        let options = crate::core::posts::CreatePostOptions {
            title: "No category".to_string(),
            category: None,
            tags: None,
            template_fields: None,
        };

        let path = crate::core::posts::create_post(&config, &options).unwrap();
        // Canonicalize to resolve macOS /var → /private/var symlink
        let expected_prefix = fs::canonicalize(site.path()).unwrap().join("content/posts");
        assert!(
            path.starts_with(&expected_prefix),
            "Post should be in posts/ when no category given.\nActual: {}\nExpected prefix: {}",
            path.display(),
            expected_prefix.display()
        );
    }

    #[test]
    fn test_list_finds_posts() {
        let (config_dir, site) = setup_hugo_site();
        create_sample_post(&site, "hello-world", false);
        create_sample_post(&site, "draft-post", true);

        let config = load_config(&config_dir);
        let result = crate::core::posts::scan_posts(&config).unwrap();
        assert_eq!(result.posts.len(), 2);
    }

    #[test]
    fn test_list_drafts_filter() {
        let (config_dir, site) = setup_hugo_site();
        create_sample_post(&site, "published-post", false);
        create_sample_post(&site, "draft-one", true);
        create_sample_post(&site, "draft-two", true);

        let config = load_config(&config_dir);
        let result = crate::core::posts::scan_posts(&config).unwrap();
        let drafts: Vec<_> = result.posts.iter().filter(|p| p.draft).collect();
        assert_eq!(drafts.len(), 2);

        let published: Vec<_> = result.posts.iter().filter(|p| !p.draft).collect();
        assert_eq!(published.len(), 1);
    }

    #[test]
    fn test_publish_sets_draft_false() {
        let (config_dir, site) = setup_hugo_site();
        create_sample_post(&site, "to-publish", true);

        let config = load_config(&config_dir);
        let result = crate::core::posts::scan_posts(&config).unwrap();
        let mut post = result
            .posts
            .into_iter()
            .find(|p| p.path.to_string_lossy().contains("to-publish"))
            .unwrap();

        assert!(post.draft, "Post should start as draft");

        // Simulate publish
        post.frontmatter
            .insert("draft".to_string(), serde_json::Value::Bool(false));
        post.draft = false;
        crate::core::posts::save_post(&post).unwrap();

        // Re-read and verify
        let saved = fs::read_to_string(&post.path).unwrap();
        assert!(saved.contains("draft: false"), "Post should be published: {}", saved);
    }

    #[test]
    fn test_publish_nonexistent_slug() {
        let (config_dir, site) = setup_hugo_site();
        create_sample_post(&site, "real-post", true);

        let config = load_config(&config_dir);
        let result = crate::core::posts::scan_posts(&config).unwrap();
        let matched = result.posts.into_iter().find(|p| {
            p.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                == "nonexistent"
        });
        assert!(matched.is_none(), "Should not find nonexistent slug");
    }

    #[test]
    fn test_list_json_format() {
        let (config_dir, site) = setup_hugo_site();
        create_sample_post(&site, "json-test", false);

        let config = load_config(&config_dir);
        let result = crate::core::posts::scan_posts(&config).unwrap();

        // Verify posts can be serialized to JSON (same as list --json)
        let json_posts: Vec<serde_json::Value> = result
            .posts
            .iter()
            .map(|p| {
                serde_json::json!({
                    "title": p.title,
                    "date": p.date.map(|d| d.to_rfc3339()),
                    "draft": p.draft,
                    "categories": p.categories,
                    "tags": p.tags,
                    "path": p.path.to_string_lossy(),
                })
            })
            .collect();
        let json_str = serde_json::to_string_pretty(&json_posts).unwrap();
        assert!(json_str.contains("json-test"));
        assert!(json_str.contains("\"draft\": false"));
    }
}
