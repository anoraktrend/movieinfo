use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use serde_json::Value;
use std::{env, io::stdout};
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};

#[derive(Deserialize)]
struct Movie {
    title: String,
    release_date: String,
    overview: String,
}

#[derive(Deserialize)]
struct MovieData {
    results: Vec<Movie>,
}

#[derive(Debug)]
enum MyError {
    Reqwest(reqwest::Error),
    Serde(serde_json::Error),
}

impl std::fmt::Display for MyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MyError::Reqwest(err) => write!(f, "Reqwest error: {}", err),
            MyError::Serde(err) => write!(f, "Serde error: {}", err),
        }
    }
}

impl std::error::Error for MyError {}

impl From<reqwest::Error> for MyError {
    fn from(err: reqwest::Error) -> MyError {
        MyError::Reqwest(err)
    }
}

impl From<serde_json::Error> for MyError {
    fn from(err: serde_json::Error) -> MyError {
        MyError::Serde(err)
    }
}

async fn search_movie(movie_name: &str) -> Result<MovieData, MyError> {
    let url = format!("https://api.themoviedb.org/3/search/movie?query={}", movie_name);

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_static("Bearer eyJhbGciOiJIUzI1NiJ9.eyJhdWQiOiJiNmM4Njc0MmE2NTIwNGJhMTQ1MjBlNDdkZDA5ODdmMCIsIm5iZiI6MTczOTI0NjcwNS4zODUsInN1YiI6IjY3YWFjYzcxZjcxYTM3ODNlMWJiMWE2YiIsInNjb3BlcyI6WyJhcGlfcmVhZCJdLCJ2ZXJzaW9uIjoxfQ.9e_cg8dEVyNjKUn7_wsnFnlXN-eyoDGn9Sxy7dRQfuk")
    );

    let client = reqwest::Client::new();
    let response = client.get(&url)
        .headers(headers)
        .send()
        .await?
        .json::<Value>()
        .await?;

    let movie_data: MovieData = serde_json::from_value(response)?;
    Ok(movie_data)
}

struct App {
    movies: Vec<Movie>,
    selected: usize,
    view: View,
    search_term: String,
    list_scroll: u16,
    detail_scroll: u16,
    list_state: ListState,  // Add this field
}

impl App {
    fn new(movies: Vec<Movie>, search_term: String) -> App {
        let mut list_state = ListState::default();
        list_state.select(Some(0));  // Initialize with first item selected
        
        App {
            movies,
            selected: 0,
            view: View::List,
            search_term,
            list_scroll: 0,
            detail_scroll: 0,
            list_state,
        }
    }

    fn next(&mut self) {
        self.selected = (self.selected + 1) % self.movies.len();
        self.list_state.select(Some(self.selected));  // Update list state
    }

    fn previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        } else {
            self.selected = self.movies.len() - 1;
        }
        self.list_state.select(Some(self.selected));  // Update list state
    }

    fn toggle_view(&mut self) {
        self.view = match self.view {
            View::List => View::Detail,
            View::Detail => View::List,
        }
    }

    fn format_movie_details(&self, width: u16) -> Vec<Spans<'_>> {
        let movie = &self.movies[self.selected];
        let content_width = width.saturating_sub(4) as usize;
        
        let mut spans = vec![
            Spans::from(vec![
                Span::raw("Release Date: "),
                Span::styled(&movie.release_date, Style::default().add_modifier(Modifier::ITALIC))
            ]),
            Spans::from(""),
            Spans::from(vec![
                Span::styled("Overview:", Style::default().add_modifier(Modifier::UNDERLINED))
            ]),
            Spans::from(""),
        ];

        // Split overview into wrapped lines based on width
        let overview_words = movie.overview.split_whitespace().collect::<Vec<_>>();
        let mut current_line = String::new();

        for word in overview_words {
            if current_line.len() + word.len() + 1 <= content_width {
                if !current_line.is_empty() {
                    current_line.push(' ');
                }
                current_line.push_str(word);
            } else {
                if !current_line.is_empty() {
                    spans.push(Spans::from(current_line));
                }
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            spans.push(Spans::from(current_line));
        }

        spans
    }

    fn calculate_content_height(&self, width: u16) -> usize {
        // Base height starts with padding:
        // 1 line for top padding
        // 1 line for release date
        // 1 line for spacing after release date
        // 1 line for overview title
        // 1 line for spacing after title
        // 1 line for bottom padding
        let base_height: usize = 5;
        
        // Calculate wrapped overview lines
        let wrapped_lines = self.format_movie_details(width).len();
        
        base_height + wrapped_lines.saturating_sub(4) // Subtract the header lines we already counted
    }

    fn needs_scroll(&self, view_height: u16, view_width: u16) -> bool {
        match self.view {
            View::List => self.movies.len() > view_height.saturating_sub(2) as usize,
            View::Detail => {
                let content_height = self.calculate_content_height(view_width);
                let visible_height = view_height.saturating_sub(2) as usize;
                content_height > visible_height
            }
        }
    }

    fn scroll_up(&mut self, view_height: u16, view_width: u16) {
        if !self.needs_scroll(view_height, view_width) {
            self.reset_scroll();
            return;
        }

        match self.view {
            View::List if self.list_scroll > 0 => self.list_scroll -= 1,
            View::Detail if self.detail_scroll > 0 => self.detail_scroll -= 1,
            _ => {}
        }
    }

    fn scroll_down(&mut self, view_height: u16, view_width: u16) {
        if !self.needs_scroll(view_height, view_width) {
            self.reset_scroll();
            return;
        }

        let max_scroll = self.get_max_scroll(view_height, view_width);
        match self.view {
            View::List if self.list_scroll < max_scroll => self.list_scroll += 1,
            View::Detail if self.detail_scroll < max_scroll => self.detail_scroll += 1,
            _ => {}
        }
    }

    fn reset_scroll(&mut self) {
        match self.view {
            View::List => self.list_scroll = 0,
            View::Detail => self.detail_scroll = 0,
        }
    }

    fn ensure_selected_visible(&mut self, height: u16) {
        if matches!(self.view, View::List) {
            let visible_items = height.saturating_sub(2) as usize; // Account for borders
            let top_item = self.list_scroll as usize;
            let bottom_item = top_item + visible_items;

            if self.selected >= bottom_item {
                self.list_scroll = (self.selected - visible_items) as u16;
            } else if self.selected < top_item {
                self.list_scroll = self.selected as u16;
            }
            
            // Ensure list_state stays in sync
            self.list_state.select(Some(self.selected));
        }
    }

    fn get_max_scroll(&self, view_height: u16, view_width: u16) -> u16 {
        match self.view {
            View::List => {
                let visible_items = view_height.saturating_sub(2) as usize; // Account for borders
                self.movies.len().saturating_sub(visible_items) as u16
            }
            View::Detail => {
                let content_height = self.calculate_content_height(view_width);
                let visible_height = view_height.saturating_sub(2) as usize; // Border padding
                content_height.saturating_sub(visible_height).saturating_add(0) as u16 // Add 1 for partial lines
            }
        }
    }

    fn scroll_list(&mut self, direction: isize, height: u16) {
        let visible_items = height.saturating_sub(2) as usize; // Account for borders
        let total_items = self.movies.len();
        
        if total_items <= visible_items {
            self.list_scroll = 0;
            return;
        }

        let current_scroll = self.list_scroll as isize;
        let new_scroll = (current_scroll + direction)
            .max(0)
            .min((total_items.saturating_sub(visible_items)) as isize);
        
        self.list_scroll = new_scroll as u16;

        // Ensure selected item stays visible
        if self.selected < self.list_scroll as usize {
            self.selected = self.list_scroll as usize;
        } else if self.selected >= (self.list_scroll as usize + visible_items) {
            self.selected = (self.list_scroll as usize + visible_items).min(self.movies.len());
        }
    }

}

enum View {
    List,
    Detail,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        println!("Usage: cargo run <movie_name>");
        return Ok(());
    }

    let movie_name = &args[1];
    let movie_data = search_movie(movie_name).await?;

    if movie_data.results.is_empty() {
        println!("No results found for '{}'", movie_name);
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(crossterm::cursor::Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(movie_data.results, movie_name.to_string());

    loop {
        terminal.draw(|f| {
            let size = f.size();
            
            // Create three-section layout for both views
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(1),
                ].as_ref())
                .split(size);

            match app.view {
                View::List => {
                    // Title remains same
                    let title_text = Paragraph::new(format!("Searching for: {}", app.search_term))
                        .block(Block::default()
                            .title("Movie Search")
                            .borders(Borders::ALL))
                        .alignment(tui::layout::Alignment::Center)
                        .style(Style::default().fg(Color::White));
                    f.render_widget(title_text, chunks[0]);

                    // List remains same but in middle chunk
                    app.ensure_selected_visible(chunks[1].height);
                    let items: Vec<ListItem> = app.movies
                        .iter()
                        .map(|m| ListItem::new(m.title.as_str()).style(Style::default()))
                        .collect();

                    let movies_list = List::new(items)
                        .block(Block::default().title("Movies").borders(Borders::ALL))
                        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                        .highlight_symbol("> ");

                    f.render_stateful_widget(movies_list, chunks[1], &mut app.list_state);

                    // Help message at bottom
                    let help = Paragraph::new("Press 'q' to quit, Enter to view details")
                        .style(Style::default().fg(Color::Gray));
                    f.render_widget(help, chunks[2]);
                }
                View::Detail => {
                    // Title remains same
                    let movie = &app.movies[app.selected];
                    let title_text = Paragraph::new(movie.title.as_str())
                        .block(Block::default()
                            .title("Movie Info")
                            .borders(Borders::ALL))
                        .alignment(tui::layout::Alignment::Center)
                        .style(Style::default().add_modifier(Modifier::BOLD));
                    f.render_widget(title_text, chunks[0]);

                    // Details in middle chunk
                    let text = app.format_movie_details(chunks[1].width);
                    let details = Paragraph::new(text)
                        .block(Block::default().borders(Borders::ALL).title("Details"))
                        .wrap(Wrap { trim: true })
                        .scroll((app.detail_scroll, 0));
                    f.render_widget(details, chunks[1]);

                    // Help message at bottom
                    let help = Paragraph::new("Press ESC to return to list")
                        .style(Style::default().fg(Color::Gray));
                    f.render_widget(help, chunks[2]);
                }
            }
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Down => {
                    let size = terminal.size()?;
                    let height = size.height.saturating_sub(3);
                    if matches!(app.view, View::List) { 
                        if event::KeyModifiers::SHIFT == key.modifiers {
                            app.scroll_list(1, height);
                        } else {
                            app.next();
                            app.ensure_selected_visible(height);
                        }
                    } else {
                        app.scroll_down(height, size.width);
                    }
                },
                KeyCode::Up => {
                    let size = terminal.size()?;
                    let height = size.height.saturating_sub(3);
                    if matches!(app.view, View::List) {
                        if event::KeyModifiers::SHIFT == key.modifiers {
                            app.scroll_list(-1, height);
                        } else {
                            app.previous();
                            app.ensure_selected_visible(height);
                        }
                    } else {
                        app.scroll_up(height, size.width);
                    }
                },
                KeyCode::Enter => {
                    app.toggle_view();
                    app.reset_scroll();
                },
                KeyCode::Esc => {
                    app.view = View::List;
                    app.reset_scroll();
                },
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.backend_mut().execute(crossterm::cursor::Show)?;

    Ok(())
}
