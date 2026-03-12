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
            drafts: _,
            category: _,
            json: _,
        }) => {
            println!("Listing posts...");
            // TODO: Implement
        }
        Some(Commands::Publish { slug }) => {
            println!("Publishing: {}", slug);
            // TODO: Implement
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
