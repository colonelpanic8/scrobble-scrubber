use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use lastfm_edit::Track;
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
    Frame, Terminal,
};
use scrobble_scrubber::rewrite::{create_no_op_edit, RewriteRule, SdRule};
use std::io;
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct RuleEditor {
    track_find: String,
    track_replace: String,
    artist_find: String,
    artist_replace: String,
    album_find: String,
    album_replace: String,
    current_field: usize, // 0=track_find, 1=track_replace, 2=artist_find, 3=artist_replace, 4=album_find, 5=album_replace
    editing: bool,        // true when actively editing a field
}

pub struct TuiApp<S, P>
where
    S: scrobble_scrubber::persistence::StateStorage,
    P: scrobble_scrubber::scrub_action_provider::ScrubActionProvider,
{
    scrubber: std::sync::Arc<Mutex<scrobble_scrubber::scrubber::ScrobbleScrubber<S, P>>>,
    current_tab: usize, // 0=tracks, 1=rule_editor, 2=preview
    tracks: Vec<Track>,
    track_list_state: ListState,
    tracks_limit: u32,
    rule_editor: RuleEditor,
    preview_text: String,
    loading: bool,
    error_message: Option<String>,
}

impl<S, P> TuiApp<S, P>
where
    S: scrobble_scrubber::persistence::StateStorage + Send + 'static,
    P: scrobble_scrubber::scrub_action_provider::ScrubActionProvider + Send + 'static,
{
    pub fn new(
        scrubber: std::sync::Arc<Mutex<scrobble_scrubber::scrubber::ScrobbleScrubber<S, P>>>,
    ) -> Self {
        let mut track_list_state = ListState::default();
        track_list_state.select(Some(0));

        Self {
            scrubber,
            current_tab: 0,
            tracks: Vec::new(),
            track_list_state,
            tracks_limit: 50, // Start with 50 tracks
            rule_editor: RuleEditor::default(),
            preview_text: String::new(),
            loading: false,
            error_message: None,
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Just update preview with welcome message - don't try to load anything yet
        self.update_preview();

        let result = self.run_app(&mut terminal).await;

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    async fn run_app<B: Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            terminal.draw(|f| self.ui(f))?;

            if event::poll(std::time::Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') => break,
                            KeyCode::Tab => {
                                if self.current_tab == 1 {
                                    // Navigate within rule editor fields
                                    self.rule_editor.current_field =
                                        (self.rule_editor.current_field + 1) % 6;
                                    self.rule_editor.editing = false;
                                    self.update_preview();
                                } else {
                                    self.current_tab = (self.current_tab + 1) % 3;
                                }
                            }
                            KeyCode::BackTab => {
                                if self.current_tab == 1 {
                                    // Navigate within rule editor fields (backwards)
                                    self.rule_editor.current_field =
                                        (self.rule_editor.current_field + 5) % 6; // +5 for backwards in mod 6
                                    self.rule_editor.editing = false;
                                    self.update_preview();
                                } else {
                                    self.current_tab = (self.current_tab + 2) % 3;
                                    // +2 to go backwards in mod 3
                                }
                            }
                            KeyCode::Down => {
                                if self.current_tab == 1 && !self.rule_editor.editing {
                                    // Navigate rule editor fields
                                    self.rule_editor.current_field =
                                        (self.rule_editor.current_field + 1) % 6;
                                    self.update_preview();
                                } else if self.current_tab == 0 {
                                    // Navigate tracks
                                    self.next_item();
                                    self.update_preview();
                                }
                            }
                            KeyCode::Up => {
                                if self.current_tab == 1 && !self.rule_editor.editing {
                                    // Navigate rule editor fields
                                    self.rule_editor.current_field =
                                        (self.rule_editor.current_field + 5) % 6; // backwards
                                    self.update_preview();
                                } else if self.current_tab == 0 {
                                    // Navigate tracks
                                    self.previous_item();
                                    self.update_preview();
                                }
                            }
                            KeyCode::Left => {
                                self.current_tab = (self.current_tab + 2) % 3;
                            }
                            KeyCode::Right => {
                                self.current_tab = (self.current_tab + 1) % 3;
                            }
                            KeyCode::Enter => {
                                if self.current_tab == 1 {
                                    // Toggle editing mode for rule editor
                                    self.rule_editor.editing = !self.rule_editor.editing;
                                    self.update_preview();
                                }
                            }
                            // Commands when not editing
                            KeyCode::Char('r') if !self.rule_editor.editing => {
                                self.load_recent_tracks().await?;
                                self.update_preview();
                            }
                            KeyCode::Char('a') if !self.rule_editor.editing => {
                                self.load_artist_tracks().await?;
                                self.update_preview();
                            }
                            KeyCode::Char('s') if !self.rule_editor.editing => {
                                self.save_rule().await?;
                                self.update_preview();
                            }
                            KeyCode::Char('c') if !self.rule_editor.editing => {
                                self.clear_rule();
                                self.update_preview();
                            }
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                self.tracks_limit += 25;
                                if !self.tracks.is_empty() {
                                    self.load_recent_tracks().await?;
                                }
                                self.update_preview();
                            }
                            KeyCode::Char('-') if !self.rule_editor.editing => {
                                if self.tracks_limit > 10 {
                                    self.tracks_limit -= 25;
                                    if self.tracks_limit < 10 {
                                        self.tracks_limit = 10;
                                    }
                                    if !self.tracks.is_empty() {
                                        self.load_recent_tracks().await?;
                                    }
                                    self.update_preview();
                                }
                            }
                            // Text input when editing
                            KeyCode::Char(c) if self.rule_editor.editing => {
                                self.handle_text_input(c);
                                self.update_preview();
                            }
                            KeyCode::Backspace if self.rule_editor.editing => {
                                self.handle_backspace();
                                self.update_preview();
                            }
                            KeyCode::Esc => {
                                // Dismiss error message
                                self.error_message = None;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn ui(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .split(f.area());

        let titles = vec!["Tracks", "Rule Editor", "Preview"];
        let tabs = Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Rule Workshop (tracks: {})", self.tracks_limit)),
            )
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Yellow))
            .select(self.current_tab);
        f.render_widget(tabs, chunks[0]);

        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage(40),
                    Constraint::Percentage(30),
                    Constraint::Percentage(30),
                ]
                .as_ref(),
            )
            .split(chunks[1]);

        self.render_tracks_panel(f, main_chunks[0]);
        self.render_rule_editor_panel(f, main_chunks[1]);
        self.render_preview_panel(f, main_chunks[2]);

        if self.loading {
            let popup_area = centered_rect(30, 10, f.area());
            f.render_widget(Clear, popup_area);
            f.render_widget(
                Block::default().title("Loading...").borders(Borders::ALL),
                popup_area,
            );
        }

        if let Some(error) = &self.error_message {
            let popup_area = centered_rect(70, 25, f.area());
            f.render_widget(Clear, popup_area);
            f.render_widget(
                Paragraph::new(format!("{error}\n\nPress ESC or Enter to dismiss"))
                    .block(Block::default().title("Error").borders(Borders::ALL))
                    .wrap(ratatui::widgets::Wrap { trim: true }),
                popup_area,
            );
        }
    }

    fn render_tracks_panel(&mut self, f: &mut Frame, area: Rect) {
        let border_style = if self.current_tab == 0 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let items: Vec<ListItem> = self
            .tracks
            .iter()
            .map(|track| {
                ListItem::new(vec![
                    Line::from(vec![Span::styled(
                        &track.name,
                        Style::default().fg(Color::Green),
                    )]),
                    Line::from(vec![
                        Span::styled("by ", Style::default().fg(Color::Gray)),
                        Span::styled(&track.artist, Style::default().fg(Color::Cyan)),
                    ]),
                    Line::from(vec![
                        Span::styled("album: ", Style::default().fg(Color::Gray)),
                        Span::styled(
                            track.album.as_deref().unwrap_or("Unknown"),
                            Style::default(),
                        ),
                    ]),
                ])
            })
            .collect();

        let tracks_list = List::new(items)
            .block(
                Block::default()
                    .title(format!(
                        "Tracks (r: recent, a: artist, +/-: limit {}, arrows/jk: navigate)",
                        self.tracks_limit
                    ))
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol(">> ");

        f.render_stateful_widget(tracks_list, area, &mut self.track_list_state);
    }

    fn render_rule_editor_panel(&self, f: &mut Frame, area: Rect) {
        let border_style = if self.current_tab == 1 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let fields = [
            ("Track Find", &self.rule_editor.track_find),
            ("Track Replace", &self.rule_editor.track_replace),
            ("Artist Find", &self.rule_editor.artist_find),
            ("Artist Replace", &self.rule_editor.artist_replace),
            ("Album Find", &self.rule_editor.album_find),
            ("Album Replace", &self.rule_editor.album_replace),
        ];

        let mut editor_text = vec![Line::from("Rule Editor:"), Line::from("")];

        for (i, (label, value)) in fields.iter().enumerate() {
            let is_current = i == self.rule_editor.current_field;
            let is_editing = self.rule_editor.editing && is_current;

            let label_style = Style::default().fg(Color::Gray);
            let value_style = if is_editing {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };

            let display_value = if value.is_empty() { "<empty>" } else { value };

            let prefix = if is_current { "→ " } else { "  " };
            let suffix = if is_editing { " [EDITING]" } else { "" };

            editor_text.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Yellow)),
                Span::styled(format!("{label}: "), label_style),
                Span::styled(display_value, value_style),
                Span::styled(suffix, Style::default().fg(Color::Green)),
            ]));
        }

        editor_text.push(Line::from(""));
        editor_text.push(Line::from("Controls:"));
        editor_text.push(Line::from("↑/↓: Navigate fields"));
        editor_text.push(Line::from("Enter: Edit field"));
        editor_text.push(Line::from("s: Save rule, c: Clear all"));

        let rule_editor = Paragraph::new(editor_text)
            .block(
                Block::default()
                    .title("Rule Editor")
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(rule_editor, area);
    }

    fn render_preview_panel(&mut self, f: &mut Frame, area: Rect) {
        let border_style = if self.current_tab == 2 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let preview = Paragraph::new(Text::from(self.preview_text.as_str()))
            .block(
                Block::default()
                    .title("Rule Preview")
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .wrap(ratatui::widgets::Wrap { trim: true });

        f.render_widget(preview, area);
    }

    fn next_item(&mut self) {
        if self.current_tab == 0 && !self.tracks.is_empty() {
            let i = match self.track_list_state.selected() {
                Some(i) => (i + 1) % self.tracks.len(),
                None => 0,
            };
            self.track_list_state.select(Some(i));
        }
    }

    fn previous_item(&mut self) {
        if self.current_tab == 0 && !self.tracks.is_empty() {
            let i = match self.track_list_state.selected() {
                Some(i) => (i + self.tracks.len() - 1) % self.tracks.len(),
                None => 0,
            };
            self.track_list_state.select(Some(i));
        }
    }

    fn update_preview(&mut self) {
        if self.tracks.is_empty() {
            self.preview_text = format!("Welcome to Rule Workshop!\n\nSimple Rule Editor:\n• Load tracks with 'r'\n• Edit rule fields with Enter\n• Save rule with 's'\n• Clear rule with 'c'\n\nNavigation:\n• ←/→: switch panels\n• ↑/↓: navigate items/fields\n• Enter: edit field\n\nTracks: {} limit, +/- to adjust", self.tracks_limit);
            return;
        }

        if let Some(track_index) = self.track_list_state.selected() {
            if let Some(track) = self.tracks.get(track_index) {
                let mut preview = "Original Track:\n".to_string();
                preview.push_str(&format!("  Name: {}\n", track.name));
                preview.push_str(&format!("  Artist: {}\n", track.artist));
                preview.push_str(&format!(
                    "  Album: {}\n",
                    track.album.as_deref().unwrap_or("Unknown")
                ));
                preview.push('\n');

                // Create a rule from the current editor state
                let mut rule = RewriteRule::new();
                let mut has_rules = false;

                // Track rule
                if !self.rule_editor.track_find.is_empty() {
                    let sd_rule = SdRule::new(
                        &self.rule_editor.track_find,
                        &self.rule_editor.track_replace,
                    );
                    rule = rule.with_track_name(sd_rule);
                    has_rules = true;
                }

                // Artist rule
                if !self.rule_editor.artist_find.is_empty() {
                    let sd_rule = SdRule::new(
                        &self.rule_editor.artist_find,
                        &self.rule_editor.artist_replace,
                    );
                    rule = rule.with_artist_name(sd_rule);
                    has_rules = true;
                }

                // Album rule
                if !self.rule_editor.album_find.is_empty() {
                    let sd_rule = SdRule::new(
                        &self.rule_editor.album_find,
                        &self.rule_editor.album_replace,
                    );
                    rule = rule.with_album_name(sd_rule);
                    has_rules = true;
                }

                if has_rules {
                    let mut edit = create_no_op_edit(track);
                    match rule.apply(&mut edit) {
                        Ok(changed) => {
                            if changed {
                                preview.push_str("After Rule Application:\n");
                                preview.push_str(&format!("  Name: {}\n", edit.track_name));
                                preview.push_str(&format!("  Artist: {}\n", edit.artist_name));
                                preview.push_str(&format!("  Album: {}\n", edit.album_name));
                                preview.push_str("\n✓ Rule would make changes");
                            } else {
                                preview.push_str("- Rule would not match this track");
                            }
                        }
                        Err(e) => {
                            preview.push_str(&format!("✗ Rule error: {e}"));
                        }
                    }
                } else {
                    preview.push_str("Enter find patterns to test the rule");
                }

                self.preview_text = preview;
            }
        } else {
            self.preview_text = "Select a track to see rule preview".to_string();
        }
    }

    async fn load_recent_tracks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.loading = true;
        self.error_message = None;

        // Use a timeout to avoid hanging
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let mut scrubber = self.scrubber.lock().await;
            scrubber.fetch_recent_tracks(self.tracks_limit).await
        })
        .await;

        match result {
            Ok(Ok(tracks)) => {
                if !tracks.is_empty() {
                    self.tracks = tracks;
                    self.track_list_state.select(Some(0));
                } else {
                    self.error_message = Some("No recent tracks found".to_string());
                }
            }
            Ok(Err(e)) => {
                self.error_message = Some(format!("Failed to load recent tracks: {e}"));
            }
            Err(_) => {
                self.error_message =
                    Some("Timeout loading recent tracks - check your connection".to_string());
            }
        }

        self.loading = false;
        Ok(())
    }

    async fn load_artist_tracks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Use a default artist for now - could be made interactive later
        let artist = if !self.tracks.is_empty() {
            // Use the current selected track's artist
            if let Some(selected) = self.track_list_state.selected() {
                if let Some(track) = self.tracks.get(selected) {
                    &track.artist
                } else {
                    "The Beatles" // fallback
                }
            } else {
                "The Beatles" // fallback
            }
        } else {
            "The Beatles" // fallback when no tracks loaded
        };

        self.loading = true;
        self.error_message = None;

        let result = tokio::time::timeout(std::time::Duration::from_secs(30), async {
            let mut scrubber = self.scrubber.lock().await;
            scrubber.fetch_artist_tracks(artist, 20).await
        })
        .await;

        match result {
            Ok(Ok(tracks)) => {
                if tracks.is_empty() {
                    self.error_message = Some(format!("No tracks found for artist '{artist}'"));
                } else {
                    self.tracks = tracks;
                    if !self.tracks.is_empty() {
                        self.track_list_state.select(Some(0));
                    }
                }
            }
            Ok(Err(e)) => {
                self.error_message = Some(format!("Failed to load tracks for '{artist}': {e}"));
            }
            Err(_) => {
                self.error_message = Some(format!(
                    "Timeout loading tracks for '{artist}' - check your connection"
                ));
            }
        }

        self.loading = false;
        Ok(())
    }

    fn handle_text_input(&mut self, c: char) {
        if c.is_ascii_graphic() || c == ' ' {
            match self.rule_editor.current_field {
                0 => self.rule_editor.track_find.push(c),
                1 => self.rule_editor.track_replace.push(c),
                2 => self.rule_editor.artist_find.push(c),
                3 => self.rule_editor.artist_replace.push(c),
                4 => self.rule_editor.album_find.push(c),
                5 => self.rule_editor.album_replace.push(c),
                _ => {}
            }
        }
    }

    fn handle_backspace(&mut self) {
        match self.rule_editor.current_field {
            0 => {
                self.rule_editor.track_find.pop();
            }
            1 => {
                self.rule_editor.track_replace.pop();
            }
            2 => {
                self.rule_editor.artist_find.pop();
            }
            3 => {
                self.rule_editor.artist_replace.pop();
            }
            4 => {
                self.rule_editor.album_find.pop();
            }
            5 => {
                self.rule_editor.album_replace.pop();
            }
            _ => {}
        }
    }

    fn clear_rule(&mut self) {
        self.rule_editor = RuleEditor::default();
    }

    async fn save_rule(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Create rule from current editor state
        let mut rule = RewriteRule::new();
        let mut has_rules = false;

        if !self.rule_editor.track_find.is_empty() {
            let sd_rule = SdRule::new(
                &self.rule_editor.track_find,
                &self.rule_editor.track_replace,
            );
            rule = rule.with_track_name(sd_rule);
            has_rules = true;
        }

        if !self.rule_editor.artist_find.is_empty() {
            let sd_rule = SdRule::new(
                &self.rule_editor.artist_find,
                &self.rule_editor.artist_replace,
            );
            rule = rule.with_artist_name(sd_rule);
            has_rules = true;
        }

        if !self.rule_editor.album_find.is_empty() {
            let sd_rule = SdRule::new(
                &self.rule_editor.album_find,
                &self.rule_editor.album_replace,
            );
            rule = rule.with_album_name(sd_rule);
            has_rules = true;
        }

        if !has_rules {
            self.error_message = Some("No find patterns entered - nothing to save".to_string());
            return Ok(());
        }

        // Save to storage
        self.loading = true;
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let scrubber = self.scrubber.lock().await;
            let storage = scrubber.storage();
            let mut storage_guard = storage.lock().await;

            let mut rules_state = storage_guard.load_rewrite_rules_state().await?;
            rules_state.rewrite_rules.push(rule);
            storage_guard.save_rewrite_rules_state(&rules_state).await?;

            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        })
        .await;

        match result {
            Ok(Ok(_)) => {
                // Success - clear the editor
                self.clear_rule();
                self.error_message = Some("Rule saved successfully!".to_string());
            }
            Ok(Err(e)) => {
                self.error_message = Some(format!("Failed to save rule: {e}"));
            }
            Err(_) => {
                self.error_message = Some("Timeout saving rule".to_string());
            }
        }

        self.loading = false;
        Ok(())
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
