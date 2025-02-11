use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::io::stdout;
use crossterm::event::{read, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{enable_raw_mode, disable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use tui::backend::CrosstermBackend;
use tui::Terminal;
use tui::widgets::{Block, Borders, Paragraph, Wrap};
use tui::layout::{Layout, Constraint, Direction};
use tui::text::{Spans, Span};

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

fn display_movie<B: tui::backend::Backend>(terminal: &mut Terminal<B>, movie: &Movie) {
    let block = Block::default()
        .title("Movie Info")
        .borders(Borders::ALL);

    let text = vec![
        Spans::from(vec![Span::raw(format!("Title: {}", movie.title))]),
        Spans::from(vec![Span::raw(format!("Release Date: {}", movie.release_date))]),
        Spans::from(vec![Span::raw(format!("Overview: {}", movie.overview))]),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: true });

    terminal.draw(|f| {
        let size = f.size();
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(
                [Constraint::Percentage(100)]
                    .as_ref(),
            )
            .split(size);

        f.render_widget(paragraph, layout[0]);
    }).unwrap();
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

    let mut index = 0;
    let total = movie_data.results.len();

    enable_raw_mode()?;
    let mut stdout = stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(crossterm::cursor::Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        display_movie(&mut terminal, &movie_data.results[index]);

        if let Event::Key(event) = read()? {
            match event.code {
                KeyCode::Up => {
                    if index > 0 {
                        index -= 1;
                    }
                }
                KeyCode::Down => {
                    if index < total - 1 {
                        index += 1;
                    }
                }
                KeyCode::Esc | KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                    break;
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.backend_mut().execute(crossterm::cursor::Show)?;

    Ok(())
}
