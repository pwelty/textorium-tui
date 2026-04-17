use anyhow::Result;
use crate::core::filters::{FilterOp, PropertyFilter};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
    Frame, Terminal,
};
use std::io;
use std::process::Command;

use crate::core::{
    config::{Config, MultiSiteConfig, SiteEntry},
    posts::{read_post, save_post, scan_posts, smartquotes, CreatePostOptions, Post, ScanResult},
    templates,
};

pub struct App {
    config: Config,
    posts: Vec<Post>,
    selected: usize,
    table_state: TableState,
    focused_pane: usize,      // 0=posts, 1=metadata, 2=content
    metadata_selected: usize, // Selected field in metadata pane
    content_scroll: usize,    // Scroll offset in content pane
    content_area_width: u16,  // Inner width of content pane (for wrapped line count)
    content_area_height: u16, // Inner height of content pane (for scroll max)
    search_query: String,
    search_mode: bool,
    sort_mode: SortMode,
    drafts_only: bool,
    edit_mode: bool,        // Whether we're editing a metadata field
    edit_buffer: String,    // Buffer for editing metadata values
    status_message: String, // Status bar message
    adding_field: bool,     // Whether we're adding a new field
    new_field_key: String,  // Key name for new field being added
    quit_pending: bool,     // True after first 'q' press with unsaved changes
    filtered_indices: Vec<usize>, // Cached indices into self.posts after filter+sort
    filter_dirty: bool,           // True when filtered_indices needs recomputation
    show_help: bool,              // Whether the help overlay is visible
    cached_dirty_count: usize,    // Cached count of posts with unsaved changes
    cached_visual_lines: usize,   // Cached visual line count for current post at current width
    visual_lines_post_idx: Option<usize>, // Which post index the cached visual lines are for
    visual_lines_width: u16,      // Width used for cached visual lines computation
    // Template picker overlay state
    show_template_picker: bool,
    template_names: Vec<String>,  // Available template names
    template_selected: usize,     // Currently highlighted template in picker
    new_post_title_mode: bool,    // True when prompting for new post title
    new_post_title: String,       // Buffer for new post title input
    // Site picker overlay state
    show_site_picker: bool,
    site_entries: Vec<SiteEntry>, // All registered sites for the picker
    site_picker_selected: usize,  // Currently highlighted site in picker
    // Property filter state
    property_filters: Vec<PropertyFilter>,  // Active AND filters
    show_filter_builder: bool,              // Whether filter builder overlay is open
    filter_builder_step: FilterBuilderStep, // Current step in builder
    filter_builder_field: String,           // Field name being built
    filter_builder_op: Option<FilterOp>,    // Operator being built
    filter_builder_value: String,           // Value input buffer
    filter_builder_op_idx: usize,           // Currently selected op in list
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FilterBuilderStep {
    Field,   // Enter the field name
    Op,      // Pick the operator
    Value,   // Enter the value (for contains/equals)
}

#[derive(Debug, Clone, Copy)]
enum SortMode {
    DateDesc,
    DateAsc,
    TitleAsc,
    TitleDesc,
}

impl App {
    pub fn new() -> Result<Self> {
        let config = Config::load()?;
        let ScanResult { posts, errors } = scan_posts(&config)?;

        let status_message = if errors.is_empty() {
            String::new()
        } else {
            format!(
                "Loaded {} posts ({} skipped due to parse errors)",
                posts.len(),
                errors.len()
            )
        };

        let mut table_state = TableState::default();
        table_state.select(Some(0));

        let template_names = templates::list_templates(&config).unwrap_or_default();
        let site_entries = MultiSiteConfig::load()
            .map(|m| m.sites)
            .unwrap_or_default();

        Ok(Self {
            config,
            posts,
            selected: 0,
            table_state,
            focused_pane: 0,
            metadata_selected: 0,
            content_scroll: 0,
            content_area_width: 0,
            content_area_height: 0,
            search_query: String::new(),
            search_mode: false,
            sort_mode: SortMode::DateDesc,
            drafts_only: false,
            edit_mode: false,
            edit_buffer: String::new(),
            status_message,
            adding_field: false,
            new_field_key: String::new(),
            quit_pending: false,
            filtered_indices: Vec::new(),
            filter_dirty: true,
            show_help: false,
            cached_dirty_count: 0,
            cached_visual_lines: 0,
            visual_lines_post_idx: None,
            visual_lines_width: 0,
            show_template_picker: false,
            template_names,
            template_selected: 0,
            new_post_title_mode: false,
            new_post_title: String::new(),
            show_site_picker: false,
            site_entries,
            site_picker_selected: 0,
            property_filters: Vec::new(),
            show_filter_builder: false,
            filter_builder_step: FilterBuilderStep::Field,
            filter_builder_field: String::new(),
            filter_builder_op: None,
            filter_builder_value: String::new(),
            filter_builder_op_idx: 0,
        })
    }

    fn set_selected(&mut self, index: usize) {
        self.selected = index;
        self.table_state.select(Some(index));
        self.content_scroll = 0;
        self.metadata_selected = 0;
        self.invalidate_visual_lines();
    }

    fn dirty_count(&mut self) -> usize {
        self.cached_dirty_count = self
            .posts
            .iter()
            .filter(|p| Self::is_dirty(p))
            .count();
        self.cached_dirty_count
    }

    fn is_dirty(post: &Post) -> bool {
        post.frontmatter != post.original_frontmatter || post.content != post.original_content
    }

    fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    /// Get the visual (wrapped) line count for the currently selected post.
    /// Cached — only recomputes when the post or width changes.
    fn visual_line_count(&mut self) -> usize {
        let post_idx = self.filtered_indices.get(self.selected).copied();
        let width = self.content_area_width;

        if self.visual_lines_post_idx == post_idx && self.visual_lines_width == width {
            return self.cached_visual_lines;
        }

        let lines = if let Some(idx) = post_idx {
            let content = &self.posts[idx].content;
            let w = width as usize;
            if w == 0 {
                content.lines().count()
            } else {
                content
                    .lines()
                    .map(|line| {
                        let len = line.len();
                        if len == 0 { 1 } else { (len + w - 1) / w }
                    })
                    .sum()
            }
        } else {
            0
        };

        self.cached_visual_lines = lines;
        self.visual_lines_post_idx = post_idx;
        self.visual_lines_width = width;
        lines
    }

    fn invalidate_visual_lines(&mut self) {
        self.visual_lines_post_idx = None;
    }

    fn ensure_filtered(&mut self) {
        if !self.filter_dirty {
            return;
        }

        let mut indices: Vec<usize> = (0..self.posts.len()).collect();

        // Filter drafts
        if self.drafts_only {
            indices.retain(|&i| self.posts[i].draft);
        }

        // Property filters (AND logic)
        if !self.property_filters.is_empty() {
            indices.retain(|&i| {
                self.property_filters.iter().all(|f| f.matches(&self.posts[i]))
            });
        }

        // Search
        if !self.search_query.is_empty() {
            let query = self.search_query.to_lowercase();
            indices.retain(|&i| {
                let p = &self.posts[i];
                p.title.to_lowercase().contains(&query)
                    || p.content.to_lowercase().contains(&query)
                    || p.categories
                        .iter()
                        .any(|c| c.to_lowercase().contains(&query))
                    || p.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query))
            });
        }

        // Sort
        match self.sort_mode {
            SortMode::DateDesc => indices.sort_by(|&a, &b| self.posts[b].date.cmp(&self.posts[a].date)),
            SortMode::DateAsc => indices.sort_by(|&a, &b| self.posts[a].date.cmp(&self.posts[b].date)),
            SortMode::TitleAsc => indices.sort_by(|&a, &b| self.posts[a].title.cmp(&self.posts[b].title)),
            SortMode::TitleDesc => indices.sort_by(|&a, &b| self.posts[b].title.cmp(&self.posts[a].title)),
        }

        self.filtered_indices = indices;
        self.filter_dirty = false;
    }

    fn get_filtered_posts(&self) -> Vec<&Post> {
        self.filtered_indices
            .iter()
            .map(|&i| &self.posts[i])
            .collect()
    }

    fn select_next(&mut self) {
        let len = self.filtered_indices.len();
        if len > 0 && self.selected < len - 1 {
            self.set_selected(self.selected + 1);
        }
    }

    fn select_prev(&mut self) {
        if self.selected > 0 {
            self.set_selected(self.selected - 1);
        }
    }

    fn page_down(&mut self, page_size: usize) {
        let len = self.filtered_indices.len();
        if len == 0 {
            return;
        }
        let max = len - 1;
        let new = (self.selected + page_size).min(max);
        self.set_selected(new);
    }

    fn page_up(&mut self, page_size: usize) {
        let new = self.selected.saturating_sub(page_size);
        self.set_selected(new);
    }

    fn cycle_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::DateDesc => SortMode::DateAsc,
            SortMode::DateAsc => SortMode::TitleAsc,
            SortMode::TitleAsc => SortMode::TitleDesc,
            SortMode::TitleDesc => SortMode::DateDesc,
        };
        self.invalidate_filter();
        self.set_selected(0);
    }

    fn toggle_drafts(&mut self) {
        self.drafts_only = !self.drafts_only;
        self.invalidate_filter();
        self.set_selected(0);
    }

    fn empty_state_message(&self) -> Option<String> {
        if !self.posts.is_empty() {
            return None;
        }

        if self.config.site_path.is_empty() {
            return Some("No site configured. Run: textorium use <path>".to_string());
        }

        let site_path = std::path::Path::new(&self.config.site_path);
        if !site_path.exists() {
            return Some(format!(
                "Site path not found: {}",
                self.config.site_path
            ));
        }

        let content_path = self.config.content_path();
        if !content_path.exists() {
            return Some(format!(
                "Content directory not found: {}",
                content_path.display()
            ));
        }

        Some(format!(
            "No markdown files found in {}",
            content_path.display()
        ))
    }

    fn open_in_editor(&self) -> Result<()> {
        let filtered = self.get_filtered_posts();
        if let Some(post) = filtered.get(self.selected) {
            // Get editor from config or environment
            let editor = if let Some(ref e) = self.config.editor {
                e.clone()
            } else if let Ok(e) = std::env::var("EDITOR") {
                e
            } else {
                "nano".to_string()
            };

            // Completely restore terminal
            disable_raw_mode()?;
            execute!(
                io::stdout(),
                LeaveAlternateScreen,
                DisableMouseCapture,
                crossterm::cursor::Show
            )?;

            // Open editor with proper terminal control
            let status = Command::new(&editor).arg(&post.path).status()?;

            // Re-enter TUI mode
            enable_raw_mode()?;
            execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

            if status.success() {
                return Ok(());
            } else {
                anyhow::bail!("Editor exited with error");
            }
        }
        Ok(())
    }

    fn save_all(&mut self) {
        let dirty_paths: Vec<std::path::PathBuf> = self
            .posts
            .iter()
            .filter(|p| Self::is_dirty(p))
            .map(|p| p.path.clone())
            .collect();

        if dirty_paths.is_empty() {
            self.status_message = "No unsaved changes".to_string();
        } else {
            let mut saved = 0usize;
            let mut errors = 0usize;
            let mut first_error: Option<String> = None;
            for path in &dirty_paths {
                if let Some(post) = self.posts.iter_mut().find(|p| &p.path == path) {
                    match save_post(post) {
                        Ok(_) => {
                            if let Ok(reloaded) = read_post(&post.path) {
                                post.original_frontmatter = reloaded.original_frontmatter;
                                post.original_content = reloaded.original_content;
                                post.raw_frontmatter = reloaded.raw_frontmatter;
                            }
                            saved += 1;
                        }
                        Err(e) => {
                            if first_error.is_none() {
                                first_error = Some(format!("{}", e));
                            }
                            errors += 1;
                        }
                    }
                }
            }
            self.status_message = if errors > 0 {
                let err_detail = first_error.as_deref().unwrap_or("unknown error");
                if saved > 0 {
                    match errors {
                        1 => format!("✓ Saved {}, ✗ 1 error: {}", saved, err_detail),
                        n => format!("✓ Saved {}, ✗ {} errors (first: {})", saved, n, err_detail),
                    }
                } else {
                    match errors {
                        1 => format!("✗ 1 error: {}", err_detail),
                        n => format!("✗ {} errors (first: {})", n, err_detail),
                    }
                }
            } else if saved == 1 {
                format!("✓ Saved: {}", dirty_paths[0].display())
            } else {
                format!("✓ Saved {} posts", saved)
            };
        }
    }

    fn delete_metadata_field(&mut self) {
        if self.focused_pane != 1 {
            return;
        }

        let post_path = {
            let filtered = self.get_filtered_posts();
            filtered.get(self.selected).map(|p| p.path.clone())
        };

        if let Some(path) = post_path {
            if let Some(actual_post) = self.posts.iter_mut().find(|p| p.path == path) {
                let mut keys: Vec<String> = actual_post.frontmatter.keys().cloned().collect();
                keys.sort();

                if self.metadata_selected < keys.len() {
                    if let Some(key) = keys.get(self.metadata_selected) {
                        if key != "title" {
                            actual_post.frontmatter.remove(key);
                            self.status_message = format!("✓ Deleted field: {}", key);
                            if self.metadata_selected > 0
                                && self.metadata_selected >= actual_post.frontmatter.len()
                            {
                                self.metadata_selected -= 1;
                            }
                        } else {
                            self.status_message = "✗ Cannot delete title field".to_string();
                        }
                    }
                }
            }
        }
    }

    fn revert_selected(&mut self) {
        let filtered = self.get_filtered_posts();
        if let Some(post_ref) = filtered.get(self.selected) {
            let post_path = post_ref.path.clone();
            let is_dirty = Self::is_dirty(post_ref);

            if !is_dirty {
                self.status_message = "No unsaved changes".to_string();
            } else if let Some(post) = self.posts.iter_mut().find(|p| p.path == post_path) {
                post.frontmatter = post.original_frontmatter.clone();
                post.content = post.original_content.clone();
                post.sync_fields_from_frontmatter();

                let title = post.title.clone();
                self.invalidate_filter();
                self.status_message = format!("Reverted: {}", title);
            }
        }
    }

    /// Create a new post with optional template and reload the posts list.
    fn create_new_post(&mut self, title: &str, template_name: Option<&str>) {
        let template_fields = template_name.and_then(|name| {
            templates::load_template(&self.config, name).ok()
        });

        let options = CreatePostOptions {
            title: title.to_string(),
            category: None,
            tags: None,
            template_fields,
        };

        match crate::core::posts::create_post(&self.config, &options) {
            Ok(path) => {
                // Reload posts list and select the new post
                match scan_posts(&self.config) {
                    Ok(result) => {
                        self.posts = result.posts;
                        self.invalidate_filter();
                        self.ensure_filtered();
                        // Select the newly created post
                        let pos = self
                            .filtered_indices
                            .iter()
                            .position(|&i| self.posts[i].path == path)
                            .unwrap_or(0);
                        self.set_selected(pos);
                        self.status_message = format!("✓ Created: {}", path.display());
                    }
                    Err(e) => {
                        self.status_message = format!("✗ Reload failed: {}", e);
                    }
                }
            }
            Err(e) => {
                self.status_message = format!("✗ Could not create post: {}", e);
            }
        }
    }

    fn apply_smartquotes(&mut self) {
        if self.focused_pane != 2 {
            return;
        }

        if let Some(idx) = self.filtered_indices.get(self.selected).copied() {
            let post = &mut self.posts[idx];
            post.content = smartquotes(&post.content);
            self.invalidate_filter();
            self.status_message =
                "\u{2713} Smart quotes applied (Ctrl+S to save, u to revert)".to_string();
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    // Main layout with status bar at bottom
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_chunks[0]);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    // Posts table — build rows and title in a scoped borrow, then render statefully
    let (rows, posts_title, _filtered_count, is_empty, empty_message) = {
        let filtered_posts = app.get_filtered_posts();
        let filtered_count = filtered_posts.len();
        let is_empty = filtered_posts.is_empty();

        let empty_message = if is_empty {
            if !app.search_query.is_empty() {
                format!("No posts match \"{}\"", app.search_query)
            } else if app.drafts_only {
                "No draft posts found".to_string()
            } else {
                app.empty_state_message()
                    .unwrap_or_else(|| "No posts found".to_string())
            }
        } else {
            String::new()
        };

        let rows: Vec<Row> = filtered_posts
            .iter()
            .map(|post| {
                let date = post
                    .date
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "—".to_string());

                let status = if post.draft { "draft" } else { "" };
                let content_type = if post.content_type.is_empty() {
                    "—"
                } else {
                    &post.content_type
                };

                let title_display = if App::is_dirty(post) {
                    format!("{} *", post.title)
                } else {
                    post.title.clone()
                };

                Row::new(vec![
                    Cell::from(title_display),
                    Cell::from(date),
                    Cell::from(content_type.to_string()),
                    Cell::from(status.to_string()),
                ])
            })
            .collect();

        let focus = if app.focused_pane == 0 {
            " [FOCUSED]"
        } else {
            ""
        };
        let filter = if app.drafts_only {
            " [DRAFTS ONLY]"
        } else {
            ""
        };
        let search = if !app.search_query.is_empty() {
            format!(" [SEARCH: \"{}\"]", app.search_query)
        } else {
            String::new()
        };
        let prop_filters = if !app.property_filters.is_empty() {
            let labels: Vec<String> = app.property_filters.iter().map(|f| f.display()).collect();
            format!(" [FILTER: {}]", labels.join(" AND "))
        } else {
            String::new()
        };
        let count = format!(" ({}/{})", filtered_count, app.posts.len());
        let posts_title = format!("Posts{}{}{}{}{}", count, filter, prop_filters, search, focus);

        (rows, posts_title, filtered_count, is_empty, empty_message)
    };

    // Build header with sort indicators
    let (title_header, date_header) = match app.sort_mode {
        SortMode::DateDesc => ("Title", "Date ▼"),
        SortMode::DateAsc => ("Title", "Date ▲"),
        SortMode::TitleAsc => ("Title ▲", "Date"),
        SortMode::TitleDesc => ("Title ▼", "Date"),
    };

    let header = Row::new(vec![
        Cell::from(title_header),
        Cell::from(date_header),
        Cell::from("Type"),
        Cell::from("Status"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let posts_block = Block::default()
        .borders(Borders::ALL)
        .title(posts_title)
        .border_style(if app.focused_pane == 0 {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let widths = [
        Constraint::Percentage(50),
        Constraint::Length(12),
        Constraint::Length(15),
        Constraint::Length(8),
    ];

    // Show empty state message or posts table
    if is_empty {
        let empty_state = Paragraph::new(empty_message)
            .block(posts_block)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: false });
        f.render_widget(empty_state, chunks[0]);
    } else {
        let posts_table = Table::new(rows, widths)
            .header(header)
            .block(posts_block)
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        f.render_stateful_widget(posts_table, chunks[0], &mut app.table_state);
    }

    // Metadata pane
    let filtered_posts = app.get_filtered_posts();
    let selected_post = filtered_posts.get(app.selected);
    let mut metadata_text = if let Some(post) = selected_post {
        // Collect all frontmatter fields
        let mut keys: Vec<String> = post.frontmatter.keys().cloned().collect();
        keys.sort();

        keys.iter()
            .enumerate()
            .map(|(i, key)| {
                let marker = if app.focused_pane == 1 && i == app.metadata_selected {
                    "► "
                } else {
                    "  "
                };

                // Get value as string
                let value = match post.frontmatter.get(key) {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(serde_json::Value::Bool(b)) => b.to_string(),
                    Some(serde_json::Value::Number(n)) => n.to_string(),
                    Some(serde_json::Value::Array(arr)) => {
                        let items: Vec<String> = arr
                            .iter()
                            .map(|v| match v {
                                serde_json::Value::String(s) => s.clone(),
                                serde_json::Value::Object(obj) => {
                                    let fields: Vec<String> =
                                        obj.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
                                    format!("{{{}}}", fields.join(", "))
                                }
                                other => other.to_string(),
                            })
                            .collect();
                        format!("[{}]", items.join(", "))
                    }
                    Some(serde_json::Value::Object(obj)) => {
                        let fields: Vec<String> =
                            obj.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
                        format!("{{{}}}", fields.join(", "))
                    }
                    _ => "—".to_string(),
                };

                // If we're editing this field, show the edit buffer
                let display_value =
                    if app.edit_mode && app.focused_pane == 1 && i == app.metadata_selected {
                        format!("{}_", app.edit_buffer) // Add cursor
                    } else {
                        value
                    };

                // Color based on key
                let color = match key.as_str() {
                    "title" => Color::Cyan,
                    "draft" => Color::Yellow,
                    "content_type" | "type" => Color::Green,
                    "date" => Color::Blue,
                    "tags" | "categories" => Color::Magenta,
                    _ => Color::Reset,
                };

                Line::from(vec![
                    Span::raw(marker),
                    Span::raw(format!("{}: ", key)),
                    Span::styled(display_value, Style::default().fg(color)),
                ])
            })
            .collect()
    } else {
        vec![Line::from("No post selected")]
    };

    // Add "add new field" option at the bottom
    if app.focused_pane == 1 && selected_post.is_some() {
        let num_fields = selected_post.map(|p| p.frontmatter.len()).unwrap_or(0);
        let marker = if app.metadata_selected == num_fields {
            "► "
        } else {
            "  "
        };

        let add_line = if app.adding_field {
            if app.new_field_key.is_empty() {
                Line::from(vec![
                    Span::raw(marker),
                    Span::styled(
                        format!("key: {}_", app.edit_buffer),
                        Style::default().fg(Color::Gray),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::raw(marker),
                    Span::styled(
                        format!("{}: {}_", app.new_field_key, app.edit_buffer),
                        Style::default().fg(Color::Gray),
                    ),
                ])
            }
        } else {
            Line::from(vec![
                Span::raw(marker),
                Span::styled("+ Add field", Style::default().fg(Color::Gray)),
            ])
        };
        metadata_text.push(add_line);
    }

    let metadata_title = if app.focused_pane == 1 {
        "Metadata [FOCUSED]"
    } else {
        "Metadata"
    };

    let metadata_block = Block::default()
        .borders(Borders::ALL)
        .title(metadata_title)
        .border_style(if app.focused_pane == 1 {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let metadata = Paragraph::new(metadata_text)
        .block(metadata_block)
        .wrap(Wrap { trim: false });
    f.render_widget(metadata, right_chunks[0]);

    // Content pane
    let content_text = if let Some(post) = selected_post {
        post.content.clone()
    } else {
        "No post selected".to_string()
    };

    let content_title = if app.focused_pane == 2 {
        "Content [FOCUSED]"
    } else {
        "Content"
    };

    let content_block = Block::default()
        .borders(Borders::ALL)
        .title(content_title)
        .border_style(if app.focused_pane == 2 {
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    // Store content pane dimensions for scroll max calculation
    app.content_area_width = right_chunks[1].width.saturating_sub(2);
    app.content_area_height = right_chunks[1].height.saturating_sub(2);

    let content = Paragraph::new(content_text)
        .block(content_block)
        .wrap(Wrap { trim: true })
        .scroll((app.content_scroll as u16, 0));
    f.render_widget(content, right_chunks[1]);

    // Status bar
    let dirty = app.dirty_count();
    let dirty_suffix = if dirty > 0 {
        if dirty == 1 {
            " | 1 unsaved change".to_string()
        } else {
            format!(" | {} unsaved changes", dirty)
        }
    } else {
        String::new()
    };

    let status_text = if app.quit_pending {
        "Unsaved changes. Press q again to quit, or Ctrl+S to save.".to_string()
    } else if !app.status_message.is_empty() {
        format!("{}{}", app.status_message, dirty_suffix)
    } else if app.search_mode {
        format!(
            "Search mode - Type to filter | Enter/Esc: exit search | {} matches",
            app.filtered_indices.len()
        )
    } else if app.focused_pane == 1 {
        format!("q: quit | j/k: navigate | Enter: edit/add | d: delete | u: revert | Ctrl+S: save | Tab: panes | ?: help{}", dirty_suffix)
    } else {
        {
            let filter_hint = if !app.property_filters.is_empty() {
                format!(" | F: add filter | x: clear filters ({})", app.property_filters.len())
            } else {
                " | F: property filter".to_string()
            };
            format!("q: quit | j/k: navigate | Tab/h/l: panes | Ctrl+S: save | n: new | s: sort | f: drafts | /: search | o: preview | ?:help{}{}", dirty_suffix, filter_hint)
        }
    };
    let status_bar = Paragraph::new(status_text).style(Style::default().fg(Color::Gray));
    f.render_widget(status_bar, main_chunks[1]);

    // New post title prompt overlay
    if app.new_post_title_mode {
        let area = f.area();
        let overlay_width = 60u16.min(area.width.saturating_sub(4));
        let overlay_height = 5u16;
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = ratatui::layout::Rect::new(x, y, overlay_width, overlay_height);

        let prompt_text = vec![
            Line::from(""),
            Line::from(vec![
                Span::raw("  Title: "),
                Span::styled(
                    format!("{}_", app.new_post_title),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Enter: create   Esc: cancel",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let block = Block::default()
            .title(" New post — enter title ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let para = Paragraph::new(prompt_text).block(block);
        f.render_widget(Clear, overlay_area);
        f.render_widget(para, overlay_area);
    }

    // Template picker overlay
    if app.show_template_picker {
        let area = f.area();
        let item_count = (app.template_names.len() + 2) as u16; // +2: "No template" + padding
        let overlay_height = (item_count + 4).min(area.height.saturating_sub(4));
        let overlay_width = 50u16.min(area.width.saturating_sub(4));
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = ratatui::layout::Rect::new(x, y, overlay_width, overlay_height);

        // Build items: index 0 = "No template (minimal frontmatter)", then named templates
        let mut items: Vec<Line> = Vec::new();
        items.push(Line::from(""));

        let no_tmpl_marker = if app.template_selected == 0 { "► " } else { "  " };
        items.push(Line::from(vec![
            Span::raw(no_tmpl_marker),
            Span::styled("No template (minimal frontmatter)", Style::default().fg(
                if app.template_selected == 0 { Color::Yellow } else { Color::Reset }
            )),
        ]));

        for (i, name) in app.template_names.iter().enumerate() {
            let idx = i + 1;
            let marker = if app.template_selected == idx { "► " } else { "  " };
            items.push(Line::from(vec![
                Span::raw(marker),
                Span::styled(name.clone(), Style::default().fg(
                    if app.template_selected == idx { Color::Yellow } else { Color::Reset }
                )),
            ]));
        }

        items.push(Line::from(""));
        items.push(Line::from(Span::styled(
            "  j/k: navigate   Enter: select   Esc: cancel",
            Style::default().fg(Color::DarkGray),
        )));

        let block = Block::default()
            .title(" Choose template ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        let para = Paragraph::new(items).block(block);
        f.render_widget(Clear, overlay_area);
        f.render_widget(para, overlay_area);
    }

    // Filter builder overlay
    if app.show_filter_builder {
        let area = f.area();
        let overlay_width = 60u16.min(area.width.saturating_sub(4));
        let overlay_height = 12u16.min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = ratatui::layout::Rect::new(x, y, overlay_width, overlay_height);

        let ops = ["contains", "equals", "is_true", "is_false"];

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        match app.filter_builder_step {
            FilterBuilderStep::Field => {
                lines.push(Line::from(vec![
                    Span::raw("  Field name: "),
                    Span::styled(
                        format!("{}_", app.filter_builder_field),
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  e.g. draft, content_type, tags, categories",
                    Style::default().fg(Color::DarkGray),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  Enter: next step   Esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            FilterBuilderStep::Op => {
                lines.push(Line::from(Span::styled(
                    format!("  Field: {}", app.filter_builder_field),
                    Style::default().fg(Color::Cyan),
                )));
                lines.push(Line::from(""));
                for (i, op) in ops.iter().enumerate() {
                    let selected = i == app.filter_builder_op_idx;
                    let marker = if selected { "► " } else { "  " };
                    lines.push(Line::from(vec![
                        Span::raw(marker),
                        Span::styled(op.to_string(), Style::default().fg(
                            if selected { Color::Yellow } else { Color::Reset }
                        )),
                    ]));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  j/k: navigate   Enter: select   Esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            FilterBuilderStep::Value => {
                lines.push(Line::from(Span::styled(
                    format!("  Field: {}  Op: {}", app.filter_builder_field,
                        app.filter_builder_op.as_ref().map(|o| o.label()).unwrap_or("")),
                    Style::default().fg(Color::Cyan),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::raw("  Value: "),
                    Span::styled(
                        format!("{}_", app.filter_builder_value),
                        Style::default().fg(Color::Yellow),
                    ),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  Enter: apply filter   Esc: cancel",
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        let title = match app.filter_builder_step {
            FilterBuilderStep::Field => " Add filter — step 1: field ",
            FilterBuilderStep::Op => " Add filter — step 2: operator ",
            FilterBuilderStep::Value => " Add filter — step 3: value ",
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));

        let para = Paragraph::new(lines).block(block);
        f.render_widget(Clear, overlay_area);
        f.render_widget(para, overlay_area);
    }

    // Site picker overlay
    if app.show_site_picker {
        let area = f.area();
        let item_count = app.site_entries.len() as u16;
        let overlay_height = (item_count + 5).min(area.height.saturating_sub(4));
        let overlay_width = 60u16.min(area.width.saturating_sub(4));
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = ratatui::layout::Rect::new(x, y, overlay_width, overlay_height);

        let mut items: Vec<Line> = Vec::new();
        items.push(Line::from(""));

        if app.site_entries.is_empty() {
            items.push(Line::from(Span::styled(
                "  No sites registered. Use: textorium sites add <path>",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for (i, site) in app.site_entries.iter().enumerate() {
                let is_active = site.name == app.config.site_name;
                let is_selected = i == app.site_picker_selected;
                let marker = if is_selected { "► " } else { "  " };
                let active_tag = if is_active { " (active)" } else { "" };
                let color = if is_selected {
                    Color::Yellow
                } else if is_active {
                    Color::Green
                } else {
                    Color::Reset
                };
                items.push(Line::from(vec![
                    Span::raw(marker),
                    Span::styled(format!("{}{}", site.name, active_tag), Style::default().fg(color)),
                    Span::styled(
                        format!("  {}", site.path),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }

        items.push(Line::from(""));
        items.push(Line::from(Span::styled(
            "  j/k: navigate   Enter: switch   Esc: cancel",
            Style::default().fg(Color::DarkGray),
        )));

        let block = Block::default()
            .title(" Sites — S: switch ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let para = Paragraph::new(items).block(block);
        f.render_widget(Clear, overlay_area);
        f.render_widget(para, overlay_area);
    }

    // Help overlay
    if app.show_help {
        let area = f.area();
        // Center the overlay: 60 wide, 28 tall (or fit to terminal)
        let overlay_width = 56u16.min(area.width.saturating_sub(4));
        let overlay_height = 28u16.min(area.height.saturating_sub(2));
        let x = (area.width.saturating_sub(overlay_width)) / 2;
        let y = (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = ratatui::layout::Rect::new(x, y, overlay_width, overlay_height);

        let help_text = vec![
            Line::from(Span::styled("Global", Style::default().add_modifier(Modifier::BOLD))),
            Line::from("  ?             Toggle this help"),
            Line::from("  q             Quit (confirms if unsaved)"),
            Line::from("  Ctrl+S        Save all changes"),
            Line::from("  Ctrl+C        Quit (confirms if unsaved)"),
            Line::from("  Tab / h / l   Switch panes"),
            Line::from("  /             Search posts"),
            Line::from("  s             Cycle sort mode"),
            Line::from("  f             Toggle drafts-only filter"),
            Line::from("  r             Refresh from disk"),
            Line::from("  o             Open in browser"),
            Line::from("  n             New post (template picker if templates exist)"),
            Line::from("  F             Add property filter (field:op:value)"),
            Line::from("  x             Clear all property filters"),
            Line::from("  S             Site picker (switch active site)"),
            Line::from(""),
            Line::from(Span::styled("Posts pane", Style::default().add_modifier(Modifier::BOLD))),
            Line::from("  j / k         Navigate up/down"),
            Line::from("  Ctrl+D        Page down"),
            Line::from("  Ctrl+U        Page up"),
            Line::from(""),
            Line::from(Span::styled("Metadata pane", Style::default().add_modifier(Modifier::BOLD))),
            Line::from("  j / k         Navigate fields"),
            Line::from("  Enter         Edit field / add new field"),
            Line::from("  d             Delete field"),
            Line::from("  u             Revert post changes"),
            Line::from(""),
            Line::from(Span::styled("Content pane", Style::default().add_modifier(Modifier::BOLD))),
            Line::from("  j / k         Scroll up/down"),
            Line::from("  Enter         Open in $EDITOR"),
            Line::from("  Q             Apply smart quotes"),
        ];

        let help_block = Block::default()
            .title(" Help — press ? or Esc to close ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let help_paragraph = Paragraph::new(help_text)
            .block(help_block)
            .style(Style::default().fg(Color::White));

        f.render_widget(Clear, overlay_area);
        f.render_widget(help_paragraph, overlay_area);
    }
}

pub fn run() -> Result<()> {
    // Install panic hook to restore terminal on crash
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new()?;

    // Main loop
    loop {
        app.ensure_filtered();
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            // Help overlay intercepts all keys
            if app.show_help {
                match key.code {
                    KeyCode::Char('?') | KeyCode::Esc => app.show_help = false,
                    _ => {}
                }
                continue;
            }

            // Clear status message on any key press (except when saving)
            if !key.modifiers.contains(KeyModifiers::CONTROL) || key.code != KeyCode::Char('s') {
                app.status_message.clear();
            }

            // Clear quit_pending on any non-q key
            if key.code != KeyCode::Char('q') {
                app.quit_pending = false;
            }

            // New post title input mode
            if app.new_post_title_mode {
                match key.code {
                    KeyCode::Char(c) => {
                        app.new_post_title.push(c);
                    }
                    KeyCode::Backspace => {
                        app.new_post_title.pop();
                    }
                    KeyCode::Enter => {
                        if !app.new_post_title.is_empty() {
                            let title = std::mem::take(&mut app.new_post_title);
                            app.new_post_title_mode = false;
                            // template_selected: 0 = no template, 1..n = template index
                            let tmpl = if app.template_selected > 0 {
                                app.template_names.get(app.template_selected - 1).cloned()
                            } else {
                                None
                            };
                            app.create_new_post(&title, tmpl.as_deref());
                        }
                    }
                    KeyCode::Esc => {
                        app.new_post_title_mode = false;
                        app.new_post_title.clear();
                        app.status_message = "New post cancelled".to_string();
                    }
                    _ => {}
                }
                continue;
            }

            // Filter builder overlay
            if app.show_filter_builder {
                let ops = [FilterOp::Contains, FilterOp::Equals, FilterOp::IsTrue, FilterOp::IsFalse];
                match app.filter_builder_step {
                    FilterBuilderStep::Field => match key.code {
                        KeyCode::Char(c) => {
                            app.filter_builder_field.push(c);
                        }
                        KeyCode::Backspace => {
                            app.filter_builder_field.pop();
                        }
                        KeyCode::Enter => {
                            if !app.filter_builder_field.is_empty() {
                                app.filter_builder_step = FilterBuilderStep::Op;
                                app.filter_builder_op_idx = 0;
                            }
                        }
                        KeyCode::Esc => {
                            app.show_filter_builder = false;
                            app.filter_builder_field.clear();
                            app.filter_builder_value.clear();
                            app.filter_builder_op = None;
                        }
                        _ => {}
                    },
                    FilterBuilderStep::Op => match key.code {
                        KeyCode::Char('j') | KeyCode::Down => {
                            if app.filter_builder_op_idx < ops.len() - 1 {
                                app.filter_builder_op_idx += 1;
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if app.filter_builder_op_idx > 0 {
                                app.filter_builder_op_idx -= 1;
                            }
                        }
                        KeyCode::Enter => {
                            let selected_op = ops[app.filter_builder_op_idx].clone();
                            app.filter_builder_op = Some(selected_op.clone());
                            match selected_op {
                                FilterOp::IsTrue | FilterOp::IsFalse => {
                                    // No value needed — apply immediately
                                    let filter = PropertyFilter {
                                        field: app.filter_builder_field.clone(),
                                        op: selected_op,
                                        value: String::new(),
                                    };
                                    app.property_filters.push(filter);
                                    app.show_filter_builder = false;
                                    app.filter_builder_field.clear();
                                    app.filter_builder_op = None;
                                    app.filter_builder_step = FilterBuilderStep::Field;
                                    app.invalidate_filter();
                                    app.set_selected(0);
                                    app.status_message = format!("✓ Filter added ({} active)", app.property_filters.len());
                                }
                                _ => {
                                    app.filter_builder_step = FilterBuilderStep::Value;
                                    app.filter_builder_value.clear();
                                }
                            }
                        }
                        KeyCode::Esc => {
                            app.show_filter_builder = false;
                            app.filter_builder_field.clear();
                            app.filter_builder_value.clear();
                            app.filter_builder_op = None;
                            app.filter_builder_step = FilterBuilderStep::Field;
                        }
                        _ => {}
                    },
                    FilterBuilderStep::Value => match key.code {
                        KeyCode::Char(c) => {
                            app.filter_builder_value.push(c);
                        }
                        KeyCode::Backspace => {
                            app.filter_builder_value.pop();
                        }
                        KeyCode::Enter => {
                            if !app.filter_builder_value.is_empty() {
                                if let Some(op) = app.filter_builder_op.take() {
                                    let filter = PropertyFilter {
                                        field: app.filter_builder_field.clone(),
                                        op,
                                        value: app.filter_builder_value.clone(),
                                    };
                                    app.property_filters.push(filter);
                                    app.show_filter_builder = false;
                                    app.filter_builder_field.clear();
                                    app.filter_builder_value.clear();
                                    app.filter_builder_step = FilterBuilderStep::Field;
                                    app.invalidate_filter();
                                    app.set_selected(0);
                                    app.status_message = format!("✓ Filter added ({} active)", app.property_filters.len());
                                }
                            }
                        }
                        KeyCode::Esc => {
                            app.show_filter_builder = false;
                            app.filter_builder_field.clear();
                            app.filter_builder_value.clear();
                            app.filter_builder_op = None;
                            app.filter_builder_step = FilterBuilderStep::Field;
                        }
                        _ => {}
                    },
                }
                continue;
            }

            // Site picker overlay
            if app.show_site_picker {
                let total = app.site_entries.len();
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if total > 0 && app.site_picker_selected < total - 1 {
                            app.site_picker_selected += 1;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if app.site_picker_selected > 0 {
                            app.site_picker_selected -= 1;
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(site) = app.site_entries.get(app.site_picker_selected) {
                            let name = site.name.clone();
                            app.show_site_picker = false;
                            // Switch site and reload
                            match crate::core::config::sites_use(&name) {
                                Ok(()) => {
                                    match Config::load() {
                                        Ok(new_config) => {
                                            app.config = new_config;
                                            // Reload posts for new site
                                            match scan_posts(&app.config) {
                                                Ok(result) => {
                                                    app.posts = result.posts;
                                                    app.invalidate_filter();
                                                    app.set_selected(0);
                                                    app.template_names = templates::list_templates(&app.config).unwrap_or_default();
                                                    app.status_message = format!("✓ Switched to site '{}'", name);
                                                }
                                                Err(e) => {
                                                    app.status_message = format!("✗ Reload failed: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            app.status_message = format!("✗ Config reload failed: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    app.status_message = format!("✗ Could not switch site: {}", e);
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        app.show_site_picker = false;
                        app.status_message = "Site switch cancelled".to_string();
                    }
                    _ => {}
                }
                continue;
            }

            // Template picker overlay
            if app.show_template_picker {
                let total = app.template_names.len() + 1; // 0 = no template
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        if app.template_selected < total - 1 {
                            app.template_selected += 1;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if app.template_selected > 0 {
                            app.template_selected -= 1;
                        }
                    }
                    KeyCode::Enter => {
                        // template_selected is already stored; transition to title prompt
                        app.show_template_picker = false;
                        app.new_post_title_mode = true;
                        app.new_post_title.clear();
                        app.status_message = "Enter title for new post".to_string();
                    }
                    KeyCode::Esc => {
                        app.show_template_picker = false;
                        app.template_selected = 0;
                        app.status_message = "New post cancelled".to_string();
                    }
                    _ => {}
                }
                continue;
            }

            // Handle search mode input
            if app.search_mode {
                match key.code {
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                        app.invalidate_filter();
                        app.set_selected(0); // Reset selection when search changes
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                        app.invalidate_filter();
                        app.set_selected(0);
                    }
                    KeyCode::Esc | KeyCode::Enter => {
                        // Exit search mode
                        app.search_mode = false;
                    }
                    _ => {}
                }
            }
            // Handle edit mode input (including adding new fields)
            else if app.edit_mode || app.adding_field {
                match key.code {
                    KeyCode::Char(c) => {
                        app.edit_buffer.push(c);
                    }
                    KeyCode::Backspace => {
                        app.edit_buffer.pop();
                    }
                    KeyCode::Enter => {
                        if app.adding_field {
                            // Two-step process: first key, then value
                            if app.new_field_key.is_empty() {
                                // Just entered the key name
                                app.new_field_key = app.edit_buffer.clone();
                                app.edit_buffer.clear();
                            } else {
                                // Just entered the value, save it
                                let post_path = {
                                    let filtered = app.get_filtered_posts();
                                    filtered.get(app.selected).map(|p| p.path.clone())
                                };

                                if let Some(path) = post_path {
                                    if let Some(actual_post) =
                                        app.posts.iter_mut().find(|p| p.path == path)
                                    {
                                        actual_post.frontmatter.insert(
                                            app.new_field_key.clone(),
                                            serde_json::Value::String(app.edit_buffer.clone()),
                                        );
                                    }
                                }

                                app.adding_field = false;
                                app.new_field_key.clear();
                                app.edit_buffer.clear();
                            }
                        } else {
                            // Regular edit mode
                            // Save the edited value
                            let post_path = {
                                let filtered = app.get_filtered_posts();
                                filtered.get(app.selected).map(|p| p.path.clone())
                            };

                            if let Some(path) = post_path {
                                // Find the actual post in the posts vec
                                if let Some(actual_post) =
                                    app.posts.iter_mut().find(|p| p.path == path)
                                {
                                    // Get the field key being edited
                                    let mut keys: Vec<String> =
                                        actual_post.frontmatter.keys().cloned().collect();
                                    keys.sort();

                                    if let Some(key) = keys.get(app.metadata_selected) {
                                        // Update the value, preserving the original type
                                        let original = actual_post.frontmatter.get(key);
                                        let new_value = if key == "draft" {
                                            serde_json::Value::Bool(app.edit_buffer == "true")
                                        } else if matches!(
                                            original,
                                            Some(serde_json::Value::Array(_))
                                        ) {
                                            let items: Vec<serde_json::Value> = app
                                                .edit_buffer
                                                .split(',')
                                                .map(|s| {
                                                    serde_json::Value::String(s.trim().to_string())
                                                })
                                                .filter(|v| v.as_str() != Some(""))
                                                .collect();
                                            serde_json::Value::Array(items)
                                        } else {
                                            serde_json::Value::String(app.edit_buffer.clone())
                                        };

                                        actual_post.frontmatter.insert(key.clone(), new_value);

                                        // Sync struct fields from updated frontmatter
                                        actual_post.sync_fields_from_frontmatter();
                                        app.invalidate_filter();
                                    }
                                }
                            }
                            app.edit_mode = false;
                            app.edit_buffer.clear();
                        }
                    }
                    KeyCode::Esc => {
                        // Cancel edit or adding
                        app.edit_mode = false;
                        app.adding_field = false;
                        app.edit_buffer.clear();
                        app.new_field_key.clear();
                    }
                    _ => {}
                }
            } else {
                // Normal navigation mode
                match key.code {
                    KeyCode::Char('q') => {
                        if app.dirty_count() == 0 || app.quit_pending {
                            break;
                        }
                        app.quit_pending = true;
                        continue;
                    }
                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.save_all();
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if app.dirty_count() == 0 || app.quit_pending {
                            break;
                        }
                        app.quit_pending = true;
                        continue;
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if app.focused_pane == 0 {
                            let size = terminal.size()?;
                            let half_page = (size.height as usize).saturating_sub(4) / 2;
                            app.page_down(half_page);
                        }
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if app.focused_pane == 0 {
                            let size = terminal.size()?;
                            let half_page = (size.height as usize).saturating_sub(4) / 2;
                            app.page_up(half_page);
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        match app.focused_pane {
                            0 => app.select_next(), // Posts pane
                            1 => {
                                // Metadata pane - navigate fields (including "Add field")
                                if let Some(&idx) = app.filtered_indices.get(app.selected) {
                                    let max_index = app.posts[idx].frontmatter.len(); // +1 for "Add field", 0-indexed
                                    if app.metadata_selected < max_index {
                                        app.metadata_selected += 1;
                                    }
                                }
                            }
                            2 => {
                                // Content pane - scroll down (visual wrapped lines)
                                let visual_lines = app.visual_line_count();
                                let max_scroll = visual_lines.saturating_sub(app.content_area_height as usize);
                                if app.content_scroll < max_scroll {
                                    app.content_scroll += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        match app.focused_pane {
                            0 => app.select_prev(), // Posts pane
                            1 => {
                                // Metadata pane - navigate fields
                                if app.metadata_selected > 0 {
                                    app.metadata_selected -= 1;
                                }
                            }
                            2 => {
                                // Content pane - scroll up
                                if app.content_scroll > 0 {
                                    app.content_scroll -= 1;
                                }
                            }
                            _ => {}
                        }
                    }
                    KeyCode::Enter => {
                        // Enter edit mode if in metadata pane
                        if app.focused_pane == 1 {
                            let filtered = app.get_filtered_posts();
                            if let Some(post) = filtered.get(app.selected) {
                                // Check if we're on the "Add field" row
                                if app.metadata_selected == post.frontmatter.len() {
                                    // Start adding a new field
                                    app.adding_field = true;
                                    app.edit_buffer.clear();
                                    app.new_field_key.clear();
                                } else {
                                    // Edit existing field
                                    let mut keys: Vec<String> =
                                        post.frontmatter.keys().cloned().collect();
                                    keys.sort();

                                    if let Some(key) = keys.get(app.metadata_selected) {
                                        let value = post.frontmatter.get(key);

                                        // Check for complex types that can't be safely inline-edited
                                        let is_complex = match value {
                                            Some(serde_json::Value::Array(arr)) => {
                                                arr.iter().any(|v| !v.is_string())
                                            }
                                            Some(serde_json::Value::Object(_)) => true,
                                            _ => false,
                                        };

                                        if is_complex {
                                            app.status_message = format!("Complex field '{}' — edit in $EDITOR (Enter in content pane)", key);
                                        } else {
                                            app.edit_buffer = match value {
                                                Some(serde_json::Value::String(s)) => s.clone(),
                                                Some(serde_json::Value::Bool(b)) => b.to_string(),
                                                Some(serde_json::Value::Number(n)) => n.to_string(),
                                                Some(serde_json::Value::Array(arr)) => arr
                                                    .iter()
                                                    .filter_map(|v| v.as_str().map(String::from))
                                                    .collect::<Vec<_>>()
                                                    .join(", "),
                                                _ => String::new(),
                                            };
                                            app.edit_mode = true;
                                        }
                                    }
                                }
                            }
                        } else if app.focused_pane == 2 {
                            // Open in external editor if in content pane
                            // Capture path before opening editor so we can reload just this post
                            let edited_path = {
                                let filtered = app.get_filtered_posts();
                                filtered.get(app.selected).map(|p| p.path.clone())
                            };
                            if let Err(e) = app.open_in_editor() {
                                app.status_message = format!("✗ Error opening editor: {}", e);
                            } else if let Some(path) = edited_path {
                                // Reload only the edited post, preserving unsaved changes on others
                                match read_post(&path) {
                                    Ok(reloaded) => {
                                        if let Some(post) = app.posts.iter_mut().find(|p| p.path == path) {
                                            *post = reloaded;
                                        }
                                        app.invalidate_filter();
                                        app.status_message = "✓ Reloaded after edit".to_string();
                                    }
                                    Err(e) => {
                                        app.status_message = format!("✗ Error reloading post: {}", e);
                                    }
                                }
                            }
                            // Redraw after returning from editor
                            terminal.clear()?;
                        }
                    }
                    KeyCode::Tab | KeyCode::Char('l') | KeyCode::Right => {
                        app.focused_pane = (app.focused_pane + 1) % 3;
                        app.metadata_selected = 0;
                        app.content_scroll = 0;
                    }
                    KeyCode::BackTab | KeyCode::Char('h') | KeyCode::Left => {
                        app.focused_pane = if app.focused_pane == 0 {
                            2
                        } else {
                            app.focused_pane - 1
                        };
                        app.metadata_selected = 0;
                        app.content_scroll = 0;
                    }
                    KeyCode::Char('d') => {
                        app.delete_metadata_field();
                    }
                    KeyCode::Char('s') => app.cycle_sort(),
                    KeyCode::Char('f') => app.toggle_drafts(),
                    KeyCode::Char('r') => {
                        let result = scan_posts(&app.config)?;
                        let err_count = result.errors.len();
                        app.posts = result.posts;
                        app.invalidate_filter();
                        app.ensure_filtered();
                        let max = app.filtered_indices.len().saturating_sub(1);
                        if app.selected > max {
                            app.set_selected(max);
                        }
                        app.status_message = if err_count > 0 {
                            format!(
                                "✓ Refreshed ({} files skipped due to parse errors)",
                                err_count
                            )
                        } else {
                            "✓ Refreshed".to_string()
                        };
                    }
                    KeyCode::Char('u') => {
                        app.revert_selected();
                    }
                    KeyCode::Char('o') => {
                        // Open current post in browser
                        let filtered = app.get_filtered_posts();
                        if let Some(post) = filtered.get(app.selected) {
                            if let Some(url) = app.config.preview_url(&post.path) {
                                match open::that(&url) {
                                    Ok(_) => {
                                        app.status_message =
                                            format!("✓ Opening in browser: {}", url);
                                    }
                                    Err(e) => {
                                        app.status_message =
                                            format!("✗ Could not open browser ({}). URL: {}", e, url);
                                    }
                                }
                            } else {
                                app.status_message =
                                    "✗ Could not construct preview URL".to_string();
                            }
                        }
                    }
                    KeyCode::Char('?') => {
                        app.show_help = true;
                    }
                    KeyCode::Char('F') => {
                        // Open filter builder
                        app.show_filter_builder = true;
                        app.filter_builder_step = FilterBuilderStep::Field;
                        app.filter_builder_field.clear();
                        app.filter_builder_value.clear();
                        app.filter_builder_op = None;
                        app.filter_builder_op_idx = 0;
                    }
                    KeyCode::Char('x') => {
                        // Clear all property filters
                        if !app.property_filters.is_empty() {
                            let n = app.property_filters.len();
                            app.property_filters.clear();
                            app.invalidate_filter();
                            app.set_selected(0);
                            app.status_message = format!("✓ Cleared {} filter(s)", n);
                        }
                    }
                    KeyCode::Char('S') => {
                        // Open site picker
                        app.site_entries = MultiSiteConfig::load()
                            .map(|m| m.sites)
                            .unwrap_or_default();
                        // Pre-select the active site
                        app.site_picker_selected = app
                            .site_entries
                            .iter()
                            .position(|s| s.name == app.config.site_name)
                            .unwrap_or(0);
                        app.show_site_picker = true;
                    }
                    KeyCode::Char('n') => {
                        // Create new post — prompt for template if any exist
                        // Refresh template list first (user may have added one)
                        app.template_names = templates::list_templates(&app.config).unwrap_or_default();
                        if app.template_names.is_empty() {
                            // No templates: skip picker, go straight to title prompt
                            app.template_selected = 0;
                            app.new_post_title_mode = true;
                            app.new_post_title.clear();
                            app.status_message = "Enter title for new post".to_string();
                        } else {
                            // Show template picker
                            app.template_selected = 0;
                            app.show_template_picker = true;
                        }
                    }
                    KeyCode::Char('Q') => {
                        app.apply_smartquotes();
                    }
                    KeyCode::Char('/') => {
                        // Enter search mode
                        app.search_mode = true;
                        app.search_query.clear();
                        app.invalidate_filter();
                        app.set_selected(0);
                        app.status_message = "Search mode: type to filter posts".to_string();
                    }
                    KeyCode::Esc => {
                        // Clear search if active
                        if !app.search_query.is_empty() {
                            app.search_query.clear();
                            app.invalidate_filter();
                            app.set_selected(0);
                            app.status_message = "Search cleared".to_string();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use std::io::Write;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn make_post(title: &str, date: &str, draft: bool, categories: &[&str], tags: &[&str], content: &str) -> Post {
        let dt = chrono::DateTime::parse_from_rfc3339(date)
            .map(|d| d.with_timezone(&Utc))
            .ok();
        let fm: HashMap<String, serde_json::Value> = HashMap::new();
        Post {
            path: PathBuf::from(format!("/tmp/test/{}.md", title.replace(' ', "-").to_lowercase())),
            title: title.to_string(),
            date: dt,
            draft,
            content_type: String::new(),
            categories: categories.iter().map(|s| s.to_string()).collect(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            content: content.to_string(),
            frontmatter: fm.clone(),
            raw_frontmatter: String::new(),
            original_frontmatter: fm,
            original_content: content.to_string(),
            format: crate::core::posts::FrontmatterFormat::default(),
        }
    }

    fn make_app(posts: Vec<Post>) -> App {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        App {
            config: Config::default(),
            posts,
            selected: 0,
            table_state,
            focused_pane: 0,
            metadata_selected: 0,
            content_scroll: 0,
            content_area_width: 80,
            content_area_height: 24,
            search_query: String::new(),
            search_mode: false,
            sort_mode: SortMode::DateDesc,
            drafts_only: false,
            edit_mode: false,
            edit_buffer: String::new(),
            status_message: String::new(),
            adding_field: false,
            new_field_key: String::new(),
            quit_pending: false,
            filtered_indices: Vec::new(),
            filter_dirty: true,
            show_help: false,
            cached_dirty_count: 0,
            cached_visual_lines: 0,
            visual_lines_post_idx: None,
            visual_lines_width: 0,
            show_template_picker: false,
            template_names: Vec::new(),
            template_selected: 0,
            new_post_title_mode: false,
            new_post_title: String::new(),
            show_site_picker: false,
            site_entries: Vec::new(),
            site_picker_selected: 0,
            property_filters: Vec::new(),
            show_filter_builder: false,
            filter_builder_step: FilterBuilderStep::Field,
            filter_builder_field: String::new(),
            filter_builder_op: None,
            filter_builder_value: String::new(),
            filter_builder_op_idx: 0,
        }
    }

    fn sample_posts() -> Vec<Post> {
        vec![
            make_post("Alpha post", "2026-03-01T10:00:00Z", false, &["blog"], &["rust"], "Alpha content"),
            make_post("Beta draft", "2026-03-15T10:00:00Z", true, &["docs"], &["tui"], "Beta draft content"),
            make_post("Gamma post", "2026-02-01T10:00:00Z", false, &["blog", "tech"], &["rust", "cli"], "Gamma content about CLI tools"),
        ]
    }

    #[test]
    fn test_drafts_only_filter() {
        let mut app = make_app(sample_posts());
        app.drafts_only = true;
        app.invalidate_filter();
        app.ensure_filtered();

        let filtered = app.get_filtered_posts();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].title, "Beta draft");
    }

    #[test]
    fn test_search_matches_title_content_categories_tags() {
        let mut app = make_app(sample_posts());

        // Search by title
        app.search_query = "alpha".to_string();
        app.invalidate_filter();
        app.ensure_filtered();
        assert_eq!(app.get_filtered_posts().len(), 1);

        // Search by content
        app.search_query = "CLI tools".to_string();
        app.invalidate_filter();
        app.ensure_filtered();
        assert_eq!(app.get_filtered_posts().len(), 1);
        assert_eq!(app.get_filtered_posts()[0].title, "Gamma post");

        // Search by category
        app.search_query = "docs".to_string();
        app.invalidate_filter();
        app.ensure_filtered();
        assert_eq!(app.get_filtered_posts().len(), 1);
        assert_eq!(app.get_filtered_posts()[0].title, "Beta draft");

        // Search by tag
        app.search_query = "tui".to_string();
        app.invalidate_filter();
        app.ensure_filtered();
        assert_eq!(app.get_filtered_posts().len(), 1);
        assert_eq!(app.get_filtered_posts()[0].title, "Beta draft");
    }

    #[test]
    fn test_sort_modes() {
        let mut app = make_app(sample_posts());

        // DateDesc (default) — newest first
        app.sort_mode = SortMode::DateDesc;
        app.invalidate_filter();
        app.ensure_filtered();
        let posts = app.get_filtered_posts();
        assert_eq!(posts[0].title, "Beta draft");  // Mar 15
        assert_eq!(posts[1].title, "Alpha post");  // Mar 1
        assert_eq!(posts[2].title, "Gamma post");  // Feb 1

        // DateAsc — oldest first
        app.sort_mode = SortMode::DateAsc;
        app.invalidate_filter();
        app.ensure_filtered();
        let posts = app.get_filtered_posts();
        assert_eq!(posts[0].title, "Gamma post");
        assert_eq!(posts[2].title, "Beta draft");

        // TitleAsc — alphabetical
        app.sort_mode = SortMode::TitleAsc;
        app.invalidate_filter();
        app.ensure_filtered();
        let posts = app.get_filtered_posts();
        assert_eq!(posts[0].title, "Alpha post");
        assert_eq!(posts[1].title, "Beta draft");
        assert_eq!(posts[2].title, "Gamma post");

        // TitleDesc — reverse alphabetical
        app.sort_mode = SortMode::TitleDesc;
        app.invalidate_filter();
        app.ensure_filtered();
        let posts = app.get_filtered_posts();
        assert_eq!(posts[0].title, "Gamma post");
        assert_eq!(posts[2].title, "Alpha post");
    }

    #[test]
    fn test_dirty_count() {
        let mut app = make_app(sample_posts());
        assert_eq!(app.dirty_count(), 0);

        // Modify one post's frontmatter
        app.posts[0]
            .frontmatter
            .insert("draft".to_string(), serde_json::Value::Bool(true));
        assert_eq!(app.dirty_count(), 1);

        // Modify another
        app.posts[2]
            .frontmatter
            .insert("title".to_string(), serde_json::Value::String("Changed".to_string()));
        assert_eq!(app.dirty_count(), 2);
    }

    #[test]
    fn test_select_next_prev_bounds() {
        let mut app = make_app(sample_posts());
        app.ensure_filtered();

        assert_eq!(app.selected, 0);

        // select_prev at 0 stays at 0
        app.select_prev();
        assert_eq!(app.selected, 0);

        // select_next advances
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        assert_eq!(app.selected, 2);

        // select_next at end stays at end
        app.select_next();
        assert_eq!(app.selected, 2);

        // select_prev goes back
        app.select_prev();
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn test_property_filter_applied_in_ensure_filtered() {
        use crate::core::filters::{FilterOp, PropertyFilter};
        let mut posts = sample_posts();
        // Add content_type to one post
        posts[0].frontmatter.insert(
            "content_type".to_string(),
            serde_json::Value::String("tutorial".to_string()),
        );
        posts[0].sync_fields_from_frontmatter();

        let mut app = make_app(posts);
        app.property_filters.push(PropertyFilter {
            field: "content_type".to_string(),
            op: FilterOp::Equals,
            value: "tutorial".to_string(),
        });
        app.invalidate_filter();
        app.ensure_filtered();

        let filtered = app.get_filtered_posts();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].content_type, "tutorial");
    }

    #[test]
    fn test_property_filter_contains_tag() {
        use crate::core::filters::{FilterOp, PropertyFilter};
        let mut posts = sample_posts();
        // Inject tags into frontmatter (make_post doesn't populate frontmatter HashMap)
        posts[0].frontmatter.insert(
            "tags".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("rust".to_string())]),
        );
        posts[2].frontmatter.insert(
            "tags".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("rust".to_string())]),
        );

        let mut app = make_app(posts);
        app.property_filters.push(PropertyFilter {
            field: "tags".to_string(),
            op: FilterOp::Contains,
            value: "rust".to_string(),
        });
        app.invalidate_filter();
        app.ensure_filtered();

        // Alpha post (idx 0) and Gamma post (idx 2) have tag "rust"
        assert_eq!(app.get_filtered_posts().len(), 2);
    }

    #[test]
    fn test_draft_toggle_invalidates_filter() {
        let mut app = make_app(sample_posts());
        app.ensure_filtered();
        assert_eq!(app.get_filtered_posts().len(), 3);

        app.drafts_only = true;
        app.invalidate_filter();
        app.ensure_filtered();
        assert_eq!(app.get_filtered_posts().len(), 1);

        app.drafts_only = false;
        app.invalidate_filter();
        app.ensure_filtered();
        assert_eq!(app.get_filtered_posts().len(), 3);
    }

    #[test]
    fn test_visual_line_count() {
        let long_content = "a".repeat(200); // 200 chars in 80-wide pane = 3 visual lines
        let posts = vec![make_post("Long post", "2026-03-01T10:00:00Z", false, &[], &[], &long_content)];
        let mut app = make_app(posts);
        app.content_area_width = 80;
        app.ensure_filtered();

        let lines = app.visual_line_count();
        assert_eq!(lines, 3); // ceil(200/80) = 3
    }

    fn create_temp_markdown(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    fn make_post_from_file(f: &NamedTempFile) -> Post {
        crate::core::posts::read_post(f.path()).unwrap()
    }

    // --- save_all tests ---

    #[test]
    fn test_save_all_no_dirty_posts() {
        let mut app = make_app(sample_posts());
        app.ensure_filtered();
        app.save_all();
        assert_eq!(app.status_message, "No unsaved changes");
    }

    #[test]
    fn test_save_all_writes_to_disk() {
        let md = "---\ntitle: Test post\ndraft: false\n---\n\nOriginal content.\n";
        let f = create_temp_markdown(md);
        let mut post = make_post_from_file(&f);

        // Modify content — makes it dirty
        post.content = "Updated content.".to_string();

        let mut app = make_app(vec![post]);
        app.ensure_filtered();
        app.save_all();

        let saved = std::fs::read_to_string(f.path()).unwrap();
        assert!(saved.contains("Updated content."), "save_all should write content to disk");
        assert!(app.status_message.contains("✓ Saved"), "status should confirm save");
    }

    #[test]
    fn test_save_all_syncs_originals_after_save() {
        let md = "---\ntitle: Sync test\ndraft: false\n---\n\nBefore.\n";
        let f = create_temp_markdown(md);
        let mut post = make_post_from_file(&f);

        post.content = "After.".to_string();
        assert!(App::is_dirty(&post));

        let mut app = make_app(vec![post]);
        app.ensure_filtered();
        app.save_all();

        // Post should no longer be dirty — originals were synced from disk
        assert!(!App::is_dirty(&app.posts[0]), "post should be clean after save_all syncs originals");
    }

    // --- delete_metadata_field tests ---

    #[test]
    fn test_delete_metadata_field_removes_key() {
        let mut post = make_post("Delete test", "2026-03-01T10:00:00Z", false, &[], &[], "content");
        post.frontmatter.insert("author".to_string(), serde_json::Value::String("Paul".to_string()));
        post.frontmatter.insert("title".to_string(), serde_json::Value::String("Delete test".to_string()));

        let mut app = make_app(vec![post]);
        app.focused_pane = 1;
        app.ensure_filtered();

        // Keys sort alphabetically: "author" is index 0, "title" is index 1
        app.metadata_selected = 0; // "author"
        app.delete_metadata_field();

        assert!(!app.posts[0].frontmatter.contains_key("author"), "author field should be deleted");
        assert!(app.status_message.contains("✓ Deleted field: author"));
    }

    #[test]
    fn test_delete_metadata_field_blocks_title() {
        let mut post = make_post("Title test", "2026-03-01T10:00:00Z", false, &[], &[], "content");
        post.frontmatter.insert("title".to_string(), serde_json::Value::String("Title test".to_string()));

        let mut app = make_app(vec![post]);
        app.focused_pane = 1;
        app.ensure_filtered();

        // With only "title" in frontmatter, it's at index 0
        app.metadata_selected = 0;
        app.delete_metadata_field();

        assert!(app.posts[0].frontmatter.contains_key("title"), "title should not be deletable");
        assert_eq!(app.status_message, "✗ Cannot delete title field");
    }

    #[test]
    fn test_delete_metadata_field_wrong_pane() {
        let mut post = make_post("Pane test", "2026-03-01T10:00:00Z", false, &[], &[], "content");
        post.frontmatter.insert("author".to_string(), serde_json::Value::String("Paul".to_string()));

        let mut app = make_app(vec![post]);
        app.focused_pane = 0; // posts pane, not metadata
        app.ensure_filtered();
        app.metadata_selected = 0;
        app.delete_metadata_field();

        assert!(app.posts[0].frontmatter.contains_key("author"), "field should not be deleted when focused_pane != 1");
        assert_eq!(app.status_message, "", "no status message when wrong pane");
    }

    // --- revert_selected tests ---

    #[test]
    fn test_revert_selected_restores_content() {
        let mut post = make_post("Revert test", "2026-03-01T10:00:00Z", false, &[], &[], "Original content");
        let original_content = post.content.clone();
        post.content = "Modified content".to_string();

        let mut app = make_app(vec![post]);
        app.ensure_filtered();
        app.revert_selected();

        assert_eq!(app.posts[0].content, original_content, "content should be reverted");
        assert!(!App::is_dirty(&app.posts[0]), "post should be clean after revert");
        assert!(app.status_message.starts_with("Reverted:"));
    }

    #[test]
    fn test_revert_selected_clean_post() {
        let mut app = make_app(sample_posts());
        app.ensure_filtered();
        app.revert_selected(); // nothing dirty
        assert_eq!(app.status_message, "No unsaved changes");
    }

    // --- apply_smartquotes tests ---

    #[test]
    fn test_apply_smartquotes_transforms_content() {
        let posts = vec![make_post("Quote test", "2026-03-01T10:00:00Z", false, &[], &[], "He said \"hello world\".")];
        let mut app = make_app(posts);
        app.focused_pane = 2;
        app.ensure_filtered();
        app.apply_smartquotes();

        assert!(
            app.posts[0].content.contains('\u{201C}') || app.posts[0].content.contains('\u{201D}'),
            "straight quotes should be converted to curly quotes"
        );
        assert!(app.status_message.contains("Smart quotes applied"));
    }

    #[test]
    fn test_apply_smartquotes_wrong_pane() {
        let original = "He said \"hello\".".to_string();
        let posts = vec![make_post("Quote test", "2026-03-01T10:00:00Z", false, &[], &[], &original)];
        let mut app = make_app(posts);
        app.focused_pane = 0; // posts pane, not content
        app.ensure_filtered();
        app.apply_smartquotes();

        assert_eq!(app.posts[0].content, original, "content should not change when focused_pane != 2");
        assert_eq!(app.status_message, "", "no status message when wrong pane");
    }

    // --- pane focus ---

    #[test]
    fn test_pane_focus_cycles() {
        let mut app = make_app(sample_posts());
        assert_eq!(app.focused_pane, 0);
        app.focused_pane = (app.focused_pane + 1) % 3;
        assert_eq!(app.focused_pane, 1);
        app.focused_pane = (app.focused_pane + 1) % 3;
        assert_eq!(app.focused_pane, 2);
        app.focused_pane = (app.focused_pane + 1) % 3;
        assert_eq!(app.focused_pane, 0);
    }
}
