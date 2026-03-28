use anyhow::Result;
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
    config::Config,
    posts::{read_post, save_post, scan_posts, smartquotes, Post, ScanResult},
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
            .filter(|p| p.frontmatter != p.original_frontmatter)
            .count();
        self.cached_dirty_count
    }

    fn is_dirty(&self, post: &Post) -> bool {
        post.frontmatter != post.original_frontmatter
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

                let title_display = if app.is_dirty(post) {
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
        let count = format!(" ({}/{})", filtered_count, app.posts.len());
        let posts_title = format!("Posts{}{}{}{}", count, filter, search, focus);

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
        .wrap(Wrap { trim: false })
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
        format!("q: quit | j/k: navigate | Tab/h/l: panes | Ctrl+S: save | s: sort | f: filter | /: search | o: preview | ?: help{}", dirty_suffix)
    };
    let status_bar = Paragraph::new(status_text).style(Style::default().fg(Color::Gray));
    f.render_widget(status_bar, main_chunks[1]);

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
                        // Save all dirty posts to disk
                        let dirty_paths: Vec<std::path::PathBuf> = app
                            .posts
                            .iter()
                            .filter(|p| p.frontmatter != p.original_frontmatter)
                            .map(|p| p.path.clone())
                            .collect();

                        if dirty_paths.is_empty() {
                            app.status_message = "No unsaved changes".to_string();
                        } else {
                            let mut saved = 0usize;
                            let mut errors = 0usize;
                            let mut first_error: Option<String> = None;
                            for path in &dirty_paths {
                                if let Some(post) =
                                    app.posts.iter_mut().find(|p| &p.path == path)
                                {
                                    match save_post(post) {
                                        Ok(_) => {
                                            if let Ok(reloaded) = read_post(&post.path) {
                                                post.original_frontmatter =
                                                    reloaded.original_frontmatter;
                                                post.raw_frontmatter =
                                                    reloaded.raw_frontmatter;
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
                            app.status_message = if errors > 0 {
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
                            if let Err(e) = app.open_in_editor() {
                                app.status_message = format!("✗ Error opening editor: {}", e);
                            } else {
                                // Reload posts after editing
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
                                        "✓ Reloaded after edit ({} files skipped)",
                                        err_count
                                    )
                                } else {
                                    "✓ Reloaded after edit".to_string()
                                };
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
                        // Delete metadata field when in metadata pane
                        if app.focused_pane == 1 {
                            let post_path = {
                                let filtered = app.get_filtered_posts();
                                filtered.get(app.selected).map(|p| p.path.clone())
                            };

                            if let Some(path) = post_path {
                                if let Some(actual_post) =
                                    app.posts.iter_mut().find(|p| p.path == path)
                                {
                                    let mut keys: Vec<String> =
                                        actual_post.frontmatter.keys().cloned().collect();
                                    keys.sort();

                                    // Don't allow deleting if on "Add field" row
                                    if app.metadata_selected < keys.len() {
                                        if let Some(key) = keys.get(app.metadata_selected) {
                                            // Don't allow deleting critical fields
                                            if key != "title" {
                                                actual_post.frontmatter.remove(key);
                                                app.status_message =
                                                    format!("✓ Deleted field: {}", key);
                                                // Move selection up if we were at the last field
                                                if app.metadata_selected > 0
                                                    && app.metadata_selected
                                                        >= actual_post.frontmatter.len()
                                                {
                                                    app.metadata_selected -= 1;
                                                }
                                            } else {
                                                app.status_message =
                                                    "✗ Cannot delete title field".to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }
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
                        // Revert selected post to last-saved state
                        let filtered = app.get_filtered_posts();
                        if let Some(post_ref) = filtered.get(app.selected) {
                            let post_path = post_ref.path.clone();
                            let is_dirty = post_ref.frontmatter != post_ref.original_frontmatter;

                            if !is_dirty {
                                app.status_message = "No unsaved changes".to_string();
                            } else if let Some(post) = app.posts.iter_mut().find(|p| p.path == post_path) {
                                post.frontmatter = post.original_frontmatter.clone();
                                post.sync_fields_from_frontmatter();

                                let title = post.title.clone();
                                app.invalidate_filter();
                                app.status_message = format!("Reverted: {}", title);
                            }
                        }
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
                                            format!("✗ Could not open browser — URL: {}", url);
                                        let _ = e; // Log suppressed; URL shown for manual copy
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
                    KeyCode::Char('Q') => {
                        if app.focused_pane == 2 {
                            if let Some(idx) = app.filtered_indices.get(app.selected).copied() {
                                let post = &mut app.posts[idx];
                                post.content = smartquotes(&post.content);
                                app.invalidate_filter();
                                app.status_message = "\u{2713} Smart quotes applied (Ctrl+S to save, u to revert)".to_string();
                            }
                        }
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
    use std::path::PathBuf;

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
}
