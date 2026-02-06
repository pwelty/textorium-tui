use anyhow::Result;
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
        Some(Commands::New { title, category, tags, no_edit }) => {
            println!("Creating new post: {}", title);
            // TODO: Implement
        }
        Some(Commands::List { drafts, category, json }) => {
            println!("Listing posts...");
            // TODO: Implement
        }
        Some(Commands::Publish { slug }) => {
            println!("Publishing: {}", slug);
            // TODO: Implement
        }
        Some(Commands::Idea { title, category, notes, tags }) => {
            println!("Capturing idea: {}", title);
            // TODO: Implement
        }
        Some(Commands::Serve { port, no_drafts }) => {
            println!("Starting server on port {}...", port);
            // TODO: Implement
        }
        Some(Commands::Build { minify }) => {
            println!("Building site...");
            // TODO: Implement
        }
    }

    Ok(())
}
