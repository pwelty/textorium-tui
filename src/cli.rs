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

        /// Skip opening in editor
        #[arg(long)]
        no_edit: bool,
    },

    /// List posts
    List {
        /// Show only drafts
        #[arg(short, long)]
        drafts: bool,

        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Publish a draft post
    Publish {
        /// Post slug or path
        slug: String,
    },

    /// Capture an idea to Notion
    Idea {
        /// Idea title
        title: String,

        /// Category
        #[arg(short, long)]
        category: Option<String>,

        /// Additional notes
        #[arg(short, long)]
        notes: Option<String>,

        /// Tags (comma-separated)
        #[arg(short, long)]
        tags: Option<String>,
    },

    /// Start development server
    Serve {
        /// Port number
        #[arg(short, long, default_value = "1313")]
        port: u16,

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

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        None => {
            // No subcommand = launch TUI
            crate::tui::app::run().await?;
        }
        Some(Commands::Use { path }) => {
            crate::core::config::configure_site(&path)?;
            println!("✓ Configured textorium to use: {}", path);
        }
        Some(Commands::New {
            title,
            category,
            tags,
            no_edit,
        }) => {
            let config = crate::core::config::Config::load()?;
            if config.site_path.is_empty() {
                anyhow::bail!("No site configured. Run: textorium use <path>");
            }

            let tag_list = tags.map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

            let options = crate::core::posts::CreatePostOptions {
                title,
                category,
                tags: tag_list,
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
        Some(Commands::List {
            drafts,
            category,
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
                        let title = if p.title.len() > 48 {
                            format!("{}…", &p.title[..47])
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
        Some(Commands::Idea {
            title,
            category: _,
            notes: _,
            tags: _,
        }) => {
            println!("Capturing idea: {}", title);
            // TODO: Implement
        }
        Some(Commands::Serve { port, no_drafts: _ }) => {
            println!("Starting server on port {}...", port);
            // TODO: Implement
        }
        Some(Commands::Build { minify: _ }) => {
            println!("Building site...");
            // TODO: Implement
        }
    }

    Ok(())
}
