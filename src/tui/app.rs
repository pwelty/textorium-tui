use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use std::process::Command;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame, Terminal,
};

use crate::core::{config::Config, posts::{save_post, scan_posts, Post}};

pub struct App {
    config: Config,
    posts: Vec<Post>,
    selected: usize,
    focused_pane: usize, // 0=posts, 1=metadata, 2=content
    metadata_selected: usize, // Selected field in metadata pane
    content_scroll: usize, // Scroll offset in content pane
    search_query: String,
    search_mode: bool,
    sort_mode: SortMode,
    drafts_only: bool,
    edit_mode: bool, // Whether we're editing a metadata field
    edit_buffer: String, // Buffer for editing metadata values
    status_message: String, // Status bar message
    adding_field: bool, // Whether we're adding a new field
    new_field_key: String, // Key name for new field being added
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
        let posts = scan_posts(&config)?;

        Ok(Self {
            config,
            posts,
            selected: 0,
            focused_pane: 0,
            metadata_selected: 0,
            content_scroll: 0,
            search_query: String::new(),
            search_mode: false,
            sort_mode: SortMode::DateDesc,
            drafts_only: false,
            edit_mode: false,
            edit_buffer: String::new(),
            status_message: String::new(),
            adding_field: false,
            new_field_key: String::new(),
        })
    }

    fn get_filtered_posts(&self) -> Vec<&Post> {
        let mut filtered: Vec<&Post> = self.posts.iter().collect();

        // Filter drafts
        if self.drafts_only {
            filtered.retain(|p| p.draft);
        }

        // Search: filters by title, content, and categories (case-insensitive substring match)
        // Note: Does not search tags or other frontmatter fields
        if !self.search_query.is_empty() {
            let query = self.search_query.to_lowercase();
            filtered.retain(|p| {
                p.title.to_lowercase().contains(&query)
                    || p.content.to_lowercase().contains(&query)
                    || p.categories.iter().any(|c| c.to_lowercase().contains(&query))
            });
        }

        // Sort
        match self.sort_mode {
            SortMode::DateDesc => filtered.sort_by(|a, b| b.date.cmp(&a.date)),
            SortMode::DateAsc => filtered.sort_by(|a, b| a.date.cmp(&b.date)),
            SortMode::TitleAsc => filtered.sort_by(|a, b| a.title.cmp(&b.title)),
            SortMode::TitleDesc => filtered.sort_by(|a, b| b.title.cmp(&a.title)),
        }

        filtered
    }

    fn select_next(&mut self) {
        let filtered = self.get_filtered_posts();
        if !filtered.is_empty() && self.selected < filtered.len() - 1 {
            self.selected += 1;
        }
    }

    fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn cycle_sort(&mut self) {
        self.sort_mode = match self.sort_mode {
            SortMode::DateDesc => SortMode::DateAsc,
            SortMode::DateAsc => SortMode::TitleAsc,
            SortMode::TitleAsc => SortMode::TitleDesc,
            SortMode::TitleDesc => SortMode::DateDesc,
        };
        self.selected = 0;
    }

    fn toggle_drafts(&mut self) {
        self.drafts_only = !self.drafts_only;
        self.selected = 0;
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
            let status = Command::new(&editor)
                .arg(&post.path)
                .status()?;

            // Re-enter TUI mode
            enable_raw_mode()?;
            execute!(
                io::stdout(),
                EnterAlternateScreen,
                EnableMouseCapture
            )?;

            if status.success() {
                return Ok(());
            } else {
                anyhow::bail!("Editor exited with error");
            }
        }
        Ok(())
    }
}

fn ui(f: &mut Frame, app: &App) {
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

    // Posts table
    let filtered_posts = app.get_filtered_posts();

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

    let rows: Vec<Row> = filtered_posts
        .iter()
        .enumerate()
        .map(|(i, post)| {
            let date = post.date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "—".to_string());

            let status = if post.draft { "draft" } else { "" };
            let content_type = if post.content_type.is_empty() {
                "—"
            } else {
                &post.content_type
            };

            let style = if i == app.selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(post.title.as_str()),
                Cell::from(date),
                Cell::from(content_type),
                Cell::from(status),
            ])
            .style(style)
        })
        .collect();

    let posts_title = {
        let focus = if app.focused_pane == 0 { " [FOCUSED]" } else { "" };
        let filter = if app.drafts_only { " [DRAFTS ONLY]" } else { "" };
        let search = if !app.search_query.is_empty() {
            format!(" [SEARCH: \"{}\"]", app.search_query)
        } else {
            String::new()
        };
        let count = format!(" ({}/{})", filtered_posts.len(), app.posts.len());
        format!("Posts{}{}{}{}", count, filter, search, focus)
    };

    let posts_block = Block::default()
        .borders(Borders::ALL)
        .title(posts_title)
        .border_style(if app.focused_pane == 0 {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let widths = [
        Constraint::Percentage(50),
        Constraint::Length(12),
        Constraint::Length(15),
        Constraint::Length(8),
    ];

    let posts_table = Table::new(rows, widths)
        .header(header)
        .block(posts_block);

    f.render_widget(posts_table, chunks[0]);

    // Metadata pane
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
                        // Format arrays nicely
                        let items: Vec<String> = arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        format!("[{}]", items.join(", "))
                    }
                    _ => "—".to_string(),
                };

                // If we're editing this field, show the edit buffer
                let display_value = if app.edit_mode && app.focused_pane == 1 && i == app.metadata_selected {
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
                    _ => Color::White,
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
                    Span::styled(format!("key: {}_", app.edit_buffer), Style::default().fg(Color::Gray)),
                ])
            } else {
                Line::from(vec![
                    Span::raw(marker),
                    Span::styled(format!("{}: {}_", app.new_field_key, app.edit_buffer), Style::default().fg(Color::Gray)),
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
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let metadata = Paragraph::new(metadata_text).block(metadata_block);
    f.render_widget(metadata, right_chunks[0]);

    // Content pane
    let content_text = if let Some(post) = selected_post {
        let lines: Vec<&str> = post.content.lines().collect();
        let visible_start = app.content_scroll;
        let visible_end = (visible_start + 30).min(lines.len());
        lines[visible_start..visible_end].join("\n")
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
            Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        });

    let content = Paragraph::new(content_text).block(content_block);
    f.render_widget(content, right_chunks[1]);

    // Status bar
    let status_text = if !app.status_message.is_empty() {
        app.status_message.clone()
    } else if app.search_mode {
        format!("Search mode - Type to filter | Enter/Esc: exit search | {} matches", app.get_filtered_posts().len())
    } else if app.focused_pane == 1 {
        "q: quit | j/k: navigate | Enter: edit/add | d: delete field | Ctrl+S: save | Tab: switch panes | s: sort | f: filter | /: search | o: preview | r: refresh".to_string()
    } else {
        "q: quit | j/k: navigate | Tab/h/l: switch panes | Enter: edit (meta) or open editor (content) | Ctrl+S: save | s: sort | f: filter | /: search | o: preview | r: refresh".to_string()
    };
    let status_bar = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Gray));
    f.render_widget(status_bar, main_chunks[1]);
}

pub async fn run() -> Result<()> {
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
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            // Clear status message on any key press (except when saving)
            if !key.modifiers.contains(KeyModifiers::CONTROL) || key.code != KeyCode::Char('s') {
                app.status_message.clear();
            }

            // Handle search mode input
            if app.search_mode {
                match key.code {
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                        app.selected = 0; // Reset selection when search changes
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                        app.selected = 0;
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
                                    if let Some(actual_post) = app.posts.iter_mut().find(|p| p.path == path) {
                                        actual_post.frontmatter.insert(
                                            app.new_field_key.clone(),
                                            serde_json::Value::String(app.edit_buffer.clone())
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
                                if let Some(actual_post) = app.posts.iter_mut().find(|p| p.path == path) {
                                    // Get the field key being edited
                                    let mut keys: Vec<String> = actual_post.frontmatter.keys().cloned().collect();
                                    keys.sort();

                                    if let Some(key) = keys.get(app.metadata_selected) {
                                        // Update the value
                                        let new_value = if key == "draft" {
                                            serde_json::Value::Bool(app.edit_buffer == "true")
                                        } else {
                                            serde_json::Value::String(app.edit_buffer.clone())
                                        };

                                        actual_post.frontmatter.insert(key.clone(), new_value);

                                        // Update struct fields for special keys
                                        match key.as_str() {
                                            "title" => actual_post.title = app.edit_buffer.clone(),
                                            "draft" => actual_post.draft = app.edit_buffer == "true",
                                            "content_type" | "type" => actual_post.content_type = app.edit_buffer.clone(),
                                            _ => {}
                                        }
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
                    KeyCode::Char('q') => break,
                    KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Save current post to disk
                        let filtered = app.get_filtered_posts();
                        if let Some(post) = filtered.get(app.selected) {
                            // Find the actual post in posts vec
                            if let Some(actual_post) = app.posts.iter().find(|p| p.path == post.path) {
                                match save_post(actual_post) {
                                    Ok(_) => {
                                        app.status_message = format!("✓ Saved: {}", actual_post.path.display());
                                    }
                                    Err(e) => {
                                        app.status_message = format!("✗ Error saving: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        match app.focused_pane {
                            0 => app.select_next(), // Posts pane
                            1 => {
                                // Metadata pane - navigate fields (including "Add field")
                                let filtered = app.get_filtered_posts();
                                if let Some(post) = filtered.get(app.selected) {
                                    let max_index = post.frontmatter.len(); // +1 for "Add field", 0-indexed
                                    if app.metadata_selected < max_index {
                                        app.metadata_selected += 1;
                                    }
                                }
                            }
                            2 => {
                                // Content pane - scroll down
                                app.content_scroll += 1;
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
                                    let mut keys: Vec<String> = post.frontmatter.keys().cloned().collect();
                                    keys.sort();

                                    if let Some(key) = keys.get(app.metadata_selected) {
                                        app.edit_buffer = match post.frontmatter.get(key) {
                                            Some(serde_json::Value::String(s)) => s.clone(),
                                            Some(serde_json::Value::Bool(b)) => b.to_string(),
                                            Some(serde_json::Value::Number(n)) => n.to_string(),
                                            _ => String::new(),
                                        };
                                        app.edit_mode = true;
                                    }
                                }
                            }
                        } else if app.focused_pane == 2 {
                            // Open in external editor if in content pane
                            if let Err(e) = app.open_in_editor() {
                                app.status_message = format!("✗ Error opening editor: {}", e);
                            } else {
                                // Reload posts after editing
                                app.posts = scan_posts(&app.config)?;
                                app.status_message = "✓ Reloaded after edit".to_string();
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
                        app.focused_pane = if app.focused_pane == 0 { 2 } else { app.focused_pane - 1 };
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
                                if let Some(actual_post) = app.posts.iter_mut().find(|p| p.path == path) {
                                    let mut keys: Vec<String> = actual_post.frontmatter.keys().cloned().collect();
                                    keys.sort();

                                    // Don't allow deleting if on "Add field" row
                                    if app.metadata_selected < keys.len() {
                                        if let Some(key) = keys.get(app.metadata_selected) {
                                            // Don't allow deleting critical fields
                                            if key != "title" {
                                                actual_post.frontmatter.remove(key);
                                                app.status_message = format!("✓ Deleted field: {}", key);
                                                // Move selection up if we were at the last field
                                                if app.metadata_selected > 0 && app.metadata_selected >= actual_post.frontmatter.len() {
                                                    app.metadata_selected -= 1;
                                                }
                                            } else {
                                                app.status_message = "✗ Cannot delete title field".to_string();
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
                        app.posts = scan_posts(&app.config)?;
                    }
                    KeyCode::Char('o') => {
                        // Open current post in browser
                        let filtered = app.get_filtered_posts();
                        if let Some(post) = filtered.get(app.selected) {
                            if let Some(url) = app.config.preview_url(&post.path) {
                                match Command::new("open").arg(&url).spawn() {
                                    Ok(_) => {
                                        app.status_message = format!("✓ Opening in browser: {}", url);
                                    }
                                    Err(e) => {
                                        app.status_message = format!("✗ Could not open browser: {}", e);
                                    }
                                }
                            } else {
                                app.status_message = "✗ Could not construct preview URL".to_string();
                            }
                        }
                    }
                    KeyCode::Char('/') => {
                        // Enter search mode
                        app.search_mode = true;
                        app.search_query.clear();
                        app.selected = 0;
                        app.status_message = "Search mode: type to filter posts".to_string();
                    }
                    KeyCode::Esc => {
                        // Clear search if active
                        if !app.search_query.is_empty() {
                            app.search_query.clear();
                            app.selected = 0;
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
