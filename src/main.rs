use clap::{Parser, Subcommand};
use colored::Colorize;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "todo")]
#[command(about = "A simple todo list manager")]
#[command(long_about = "A simple todo list manager\n\nYou can also pipe text directly to add todos to the active list:\n  echo \"New todo\" | todo\n  printf \"Todo 1\\nTodo 2\" | todo")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all todo files
    Lists,
    /// List todos from the active list or a specified list
    List {
        /// Optional list to display (defaults to active list)
        #[arg(short, long)]
        list: Option<String>,
    },
    /// Show interactive TUI to manage todos
    Show {
        /// Optional list to display (defaults to active list)
        #[arg(short, long)]
        list: Option<String>,
    },
    /// Switch to a different todo list
    Use { list_name: String },
    /// Add a todo to the active list or specified list
    Add {
        /// The todo text to add
        todo: String,
        /// Optional list to add the todo to (defaults to active list)
        #[arg(short, long)]
        list: Option<String>,
    },
    /// Open the active list in the configured editor
    Edit,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    todo: TodoConfig,
    editor: EditorConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct TodoConfig {
    active_list: String,
    list_extension: String,
    path: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct EditorConfig {
    command: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            todo: TodoConfig {
                active_list: "default".to_string(),
                list_extension: "adoc".to_string(),
                path: "~/todos".to_string(),
            },
            editor: EditorConfig {
                command: "nvim".to_string(),
            },
        }
    }
}

// TUI structures
#[derive(Debug, Clone)]
struct TodoItem {
    text: String,
    completed: bool,
    line_type: LineType,
}

#[derive(Debug, Clone)]
enum LineType {
    Todo,
    Header1,
    Header2,
    Header3,
    Bullet,
    Text,
    Empty,
}

struct App {
    items: Vec<TodoItem>,
    selected: usize,
    list_path: PathBuf,
    list_name: String,
}

impl App {
    fn new(list_path: PathBuf, list_name: String) -> io::Result<Self> {
        let items = Self::load_todos(&list_path)?;
        let selected = items.iter().position(|item| matches!(item.line_type, LineType::Todo)).unwrap_or(0);
        Ok(App {
            items,
            selected,
            list_path,
            list_name,
        })
    }

    fn load_todos(path: &PathBuf) -> io::Result<Vec<TodoItem>> {
        let mut items = Vec::new();

        if !path.exists() {
            return Ok(items);
        }

        let file = fs::File::open(path)?;
        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();

            if trimmed.starts_with("* [ ]") {
                let text = trimmed.strip_prefix("* [ ]").unwrap_or("").trim().to_string();
                items.push(TodoItem {
                    text,
                    completed: false,
                    line_type: LineType::Todo,
                });
            } else if trimmed.starts_with("* [x]") || trimmed.starts_with("* [X]") {
                let text = trimmed
                    .strip_prefix("* [x]")
                    .or_else(|| trimmed.strip_prefix("* [X]"))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                items.push(TodoItem {
                    text,
                    completed: true,
                    line_type: LineType::Todo,
                });
            } else if trimmed.starts_with("= ") {
                let text = trimmed.strip_prefix("= ").unwrap_or(trimmed).to_string();
                items.push(TodoItem {
                    text,
                    completed: false,
                    line_type: LineType::Header1,
                });
            } else if trimmed.starts_with("== ") {
                let text = trimmed.strip_prefix("== ").unwrap_or(trimmed).to_string();
                items.push(TodoItem {
                    text,
                    completed: false,
                    line_type: LineType::Header2,
                });
            } else if trimmed.starts_with("=== ") {
                let text = trimmed.strip_prefix("=== ").unwrap_or(trimmed).to_string();
                items.push(TodoItem {
                    text,
                    completed: false,
                    line_type: LineType::Header3,
                });
            } else if trimmed.starts_with("* ") && !trimmed.starts_with("* [") {
                let text = trimmed.strip_prefix("* ").unwrap_or(trimmed).to_string();
                items.push(TodoItem {
                    text,
                    completed: false,
                    line_type: LineType::Bullet,
                });
            } else if trimmed.is_empty() {
                items.push(TodoItem {
                    text: String::new(),
                    completed: false,
                    line_type: LineType::Empty,
                });
            } else {
                items.push(TodoItem {
                    text: trimmed.to_string(),
                    completed: false,
                    line_type: LineType::Text,
                });
            }
        }

        Ok(items)
    }

    fn save_todos(&self) -> io::Result<()> {
        let mut content = String::new();

        for item in &self.items {
            let line = match item.line_type {
                LineType::Todo => {
                    if item.completed {
                        format!("* [x] {}\n", item.text)
                    } else {
                        format!("* [ ] {}\n", item.text)
                    }
                }
                LineType::Header1 => format!("= {}\n", item.text),
                LineType::Header2 => format!("== {}\n", item.text),
                LineType::Header3 => format!("=== {}\n", item.text),
                LineType::Bullet => format!("* {}\n", item.text),
                LineType::Text => format!("{}\n", item.text),
                LineType::Empty => "\n".to_string(),
            };
            content.push_str(&line);
        }

        fs::write(&self.list_path, content)?;
        Ok(())
    }

    fn next(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let start = self.selected;
        loop {
            self.selected = (self.selected + 1) % self.items.len();
            if matches!(self.items[self.selected].line_type, LineType::Todo) || self.selected == start {
                break;
            }
        }
    }

    fn previous(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let start = self.selected;
        loop {
            self.selected = if self.selected == 0 {
                self.items.len() - 1
            } else {
                self.selected - 1
            };
            if matches!(self.items[self.selected].line_type, LineType::Todo) || self.selected == start {
                break;
            }
        }
    }

    fn goto_top(&mut self) {
        self.selected = self.items
            .iter()
            .position(|item| matches!(item.line_type, LineType::Todo))
            .unwrap_or(0);
    }

    fn goto_bottom(&mut self) {
        self.selected = self.items
            .iter()
            .rposition(|item| matches!(item.line_type, LineType::Todo))
            .unwrap_or(self.items.len().saturating_sub(1));
    }

    fn toggle_current(&mut self) {
        if self.selected < self.items.len() {
            if matches!(self.items[self.selected].line_type, LineType::Todo) {
                self.items[self.selected].completed = !self.items[self.selected].completed;
            }
        }
    }

    fn count_todos(&self) -> (usize, usize) {
        let incomplete = self.items.iter()
            .filter(|item| matches!(item.line_type, LineType::Todo) && !item.completed)
            .count();
        let complete = self.items.iter()
            .filter(|item| matches!(item.line_type, LineType::Todo) && item.completed)
            .count();
        (incomplete, complete)
    }
}

fn get_config_path() -> PathBuf {
    let home = dirs::home_dir().expect("Could not find home directory");
    home.join(".config").join("todo").join("config.toml")
}

fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        let home = dirs::home_dir().expect("Could not find home directory");
        home.join(&path[2..])
    } else {
        PathBuf::from(path)
    }
}

fn ensure_config_exists() -> Config {
    let config_path = get_config_path();

    if !config_path.exists() {
        // Create the directory if it doesn't exist
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create config directory");
        }

        // Create default config
        let config = Config::default();
        let toml_string = toml::to_string(&config).expect("Failed to serialize config");
        fs::write(&config_path, toml_string).expect("Failed to write config file");

        config
    } else {
        // Load existing config
        let config_str = fs::read_to_string(&config_path).expect("Failed to read config file");
        toml::from_str(&config_str).expect("Failed to parse config file")
    }
}

fn save_config(config: &Config) {
    let config_path = get_config_path();
    let toml_string = toml::to_string(config).expect("Failed to serialize config");
    fs::write(&config_path, toml_string).expect("Failed to write config file");
}

fn ensure_todo_directory_exists(config: &Config) -> PathBuf {
    let todo_path = expand_tilde(&config.todo.path);

    if !todo_path.exists() {
        fs::create_dir_all(&todo_path).expect("Failed to create todo directory");
    }

    todo_path
}

fn get_active_list_path(config: &Config, todo_path: &PathBuf) -> PathBuf {
    let file_name = format!("{}.{}", config.todo.active_list, config.todo.list_extension);
    todo_path.join(file_name)
}

fn ensure_active_list_exists(list_path: &PathBuf) {
    if !list_path.exists() {
        fs::write(list_path, "").expect("Failed to create todo list file");
    }
}

fn list_todos(config: &Config) {
    let todo_path = expand_tilde(&config.todo.path);

    if !todo_path.exists() {
        println!("No todo lists found.");
        return;
    }

    match fs::read_dir(&todo_path) {
        Ok(entries) => {
            let mut files: Vec<String> = entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_file())
                .filter_map(|entry| {
                    entry.file_name().to_str().map(|s| s.to_string())
                })
                .collect();

            if files.is_empty() {
                println!("No todo lists found.");
            } else {
                files.sort();
                for file in files {
                    if file == format!("{}.{}", config.todo.active_list, config.todo.list_extension) {
                        println!("* {} (active)", file);
                    } else {
                        println!("  {}", file);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading todo directory: {}", e);
        }
    }
}

fn use_list(config: &mut Config, list_name: String) {
    // Extract just the name without extension if provided
    let list_name = if list_name.contains('.') {
        list_name.split('.').next().unwrap().to_string()
    } else {
        list_name
    };

    config.todo.active_list = list_name.clone();
    save_config(config);

    println!("Switched to list: {}.{}", list_name, config.todo.list_extension);

    // Ensure the new list file exists
    let todo_path = expand_tilde(&config.todo.path);
    let list_path = get_active_list_path(config, &todo_path);
    ensure_active_list_exists(&list_path);
}

fn add_todo(config: &Config, todo_text: String, target_list: Option<String>) {
    let todo_path = expand_tilde(&config.todo.path);

    // Determine which list to add to
    let list_path = if let Some(list_name) = target_list {
        // Extract just the name without extension if provided
        let list_name = if list_name.contains('.') {
            list_name.split('.').next().unwrap().to_string()
        } else {
            list_name
        };
        let file_name = format!("{}.{}", list_name, config.todo.list_extension);
        let path = todo_path.join(file_name);

        // Ensure the target list exists
        ensure_active_list_exists(&path);
        path
    } else {
        // Use active list
        get_active_list_path(config, &todo_path)
    };

    // Format the todo item
    let todo_line = format!("* [ ] {}\n", todo_text);

    // Append to the file
    match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&list_path)
    {
        Ok(mut file) => {
            if let Err(e) = file.write_all(todo_line.as_bytes()) {
                eprintln!("Error writing to todo list: {}", e);
            } else {
                let list_name = list_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                println!("Added todo to {}: {}", list_name, todo_text);
            }
        }
        Err(e) => {
            eprintln!("Error opening todo list: {}", e);
        }
    }
}

fn edit_list(config: &Config) {
    let todo_path = expand_tilde(&config.todo.path);
    let list_path = get_active_list_path(config, &todo_path);

    // Ensure the list exists
    ensure_active_list_exists(&list_path);

    // Open in editor
    let editor = &config.editor.command;

    match Command::new(editor)
        .arg(&list_path)
        .status()
    {
        Ok(status) => {
            if !status.success() {
                eprintln!("Editor exited with status: {}", status);
            }
        }
        Err(e) => {
            eprintln!("Failed to open editor '{}': {}", editor, e);
            eprintln!("Make sure the editor command is correct in your config.");
        }
    }
}

fn display_todo_list(config: &Config, target_list: Option<String>) {
    let todo_path = expand_tilde(&config.todo.path);

    // Determine which list to display
    let (list_path, list_name) = if let Some(list_name) = target_list {
        // Extract just the name without extension if provided
        let list_name = if list_name.contains('.') {
            list_name.split('.').next().unwrap().to_string()
        } else {
            list_name
        };
        let file_name = format!("{}.{}", list_name, config.todo.list_extension);
        let path = todo_path.join(&file_name);
        (path, file_name)
    } else {
        // Use active list
        let path = get_active_list_path(config, &todo_path);
        let file_name = format!("{}.{}", config.todo.active_list, config.todo.list_extension);
        (path, file_name)
    };

    // Check if the list exists
    if !list_path.exists() {
        eprintln!("List '{}' does not exist", list_name);
        return;
    }

    // Display header
    println!("{}", format!("=== {} ===", list_name).bold().cyan());
    println!();

    // Read and parse the file
    match fs::File::open(&list_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut incomplete_count = 0;
            let mut complete_count = 0;
            let mut has_todos = false;

            for line in reader.lines() {
                if let Ok(line) = line {
                    let trimmed = line.trim();

                    // Parse incomplete todos: * [ ] text
                    if trimmed.starts_with("* [ ]") {
                        has_todos = true;
                        incomplete_count += 1;
                        let todo_text = trimmed.strip_prefix("* [ ]").unwrap_or("").trim();
                        println!("{} {}", "☐".bright_yellow(), todo_text);
                    }
                    // Parse complete todos: * [x] text
                    else if trimmed.starts_with("* [x]") || trimmed.starts_with("* [X]") {
                        has_todos = true;
                        complete_count += 1;
                        let todo_text = trimmed.strip_prefix("* [x]")
                            .or_else(|| trimmed.strip_prefix("* [X]"))
                            .unwrap_or("")
                            .trim();
                        println!("{} {}", "☑".green(), todo_text.strikethrough().dimmed());
                    }
                    // Handle other adoc formatting
                    else if trimmed.starts_with("= ") {
                        // Level 1 header
                        let header = trimmed.strip_prefix("= ").unwrap_or(trimmed);
                        println!("{}", header.bold().bright_cyan());
                    }
                    else if trimmed.starts_with("== ") {
                        // Level 2 header
                        let header = trimmed.strip_prefix("== ").unwrap_or(trimmed);
                        println!("{}", header.bold().cyan());
                    }
                    else if trimmed.starts_with("=== ") {
                        // Level 3 header
                        let header = trimmed.strip_prefix("=== ").unwrap_or(trimmed);
                        println!("{}", header.bold().blue());
                    }
                    else if trimmed.starts_with("* ") && !trimmed.starts_with("* [") {
                        // Regular bullet point
                        let text = trimmed.strip_prefix("* ").unwrap_or(trimmed);
                        println!("  {} {}", "•".bright_white(), text);
                    }
                    else if !trimmed.is_empty() {
                        // Regular text
                        println!("{}", trimmed);
                    }
                    else {
                        // Empty line
                        println!();
                    }
                }
            }

            if !has_todos {
                println!("{}", "No todos found.".dimmed());
            } else {
                println!();
                println!(
                    "{} {} incomplete, {} complete",
                    "Summary:".bold(),
                    incomplete_count.to_string().bright_yellow(),
                    complete_count.to_string().green()
                );
            }
        }
        Err(e) => {
            eprintln!("Error reading todo list: {}", e);
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Status bar
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new(format!("  {} ", app.list_name))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
        );
    f.render_widget(title, chunks[0]);

    // Todo list
    let items: Vec<ListItem> = app
        .items
        .iter()
        .enumerate()
        .map(|(i, todo_item)| {
            let content = match todo_item.line_type {
                LineType::Todo => {
                    if todo_item.completed {
                        Line::from(vec![
                            Span::styled("☑ ", Style::default().fg(Color::Green)),
                            Span::styled(
                                &todo_item.text,
                                Style::default()
                                    .fg(Color::DarkGray)
                                    .add_modifier(Modifier::CROSSED_OUT),
                            ),
                        ])
                    } else {
                        Line::from(vec![
                            Span::styled("☐ ", Style::default().fg(Color::Yellow)),
                            Span::styled(&todo_item.text, Style::default().fg(Color::White)),
                        ])
                    }
                }
                LineType::Header1 => {
                    Line::from(Span::styled(
                        &todo_item.text,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ))
                }
                LineType::Header2 => {
                    Line::from(Span::styled(
                        &todo_item.text,
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::BOLD),
                    ))
                }
                LineType::Header3 => {
                    Line::from(Span::styled(
                        &todo_item.text,
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ))
                }
                LineType::Bullet => {
                    Line::from(vec![
                        Span::raw("  • "),
                        Span::styled(&todo_item.text, Style::default().fg(Color::White)),
                    ])
                }
                LineType::Text => {
                    Line::from(Span::styled(&todo_item.text, Style::default().fg(Color::Gray)))
                }
                LineType::Empty => Line::from(""),
            };

            let style = if i == app.selected {
                Style::default()
                    .bg(Color::Rgb(60, 60, 80))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White))
        );

    f.render_widget(list, chunks[1]);

    // Status bar
    let (incomplete, complete) = app.count_todos();
    let status_text = format!(
        " {} incomplete  {} complete  │  [j/k] move  [Space/Enter] toggle  [g/G] top/bottom  [q] quit ",
        incomplete, complete
    );

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::White).bg(Color::Rgb(40, 40, 60)))
        .block(Block::default());

    f.render_widget(status, chunks[2]);
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => {
                        app.save_todos()?;
                        return Ok(());
                    }
                    KeyCode::Char('j') | KeyCode::Down => app.next(),
                    KeyCode::Char('k') | KeyCode::Up => app.previous(),
                    KeyCode::Char('g') => app.goto_top(),
                    KeyCode::Char('G') => app.goto_bottom(),
                    KeyCode::Char(' ') | KeyCode::Enter => app.toggle_current(),
                    _ => {}
                }
            }
        }
    }
}

fn show_tui(config: &Config, target_list: Option<String>) -> io::Result<()> {
    let todo_path = expand_tilde(&config.todo.path);

    // Determine which list to display
    let (list_path, list_name) = if let Some(list_name) = target_list {
        let list_name = if list_name.contains('.') {
            list_name.split('.').next().unwrap().to_string()
        } else {
            list_name
        };
        let file_name = format!("{}.{}", list_name, config.todo.list_extension);
        let path = todo_path.join(&file_name);
        (path, file_name)
    } else {
        let path = get_active_list_path(config, &todo_path);
        let file_name = format!("{}.{}", config.todo.active_list, config.todo.list_extension);
        (path, file_name)
    };

    // Ensure the list exists
    ensure_active_list_exists(&list_path);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let app = App::new(list_path, list_name)?;
    let res = run_app(&mut terminal, app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

fn main() {
    // Ensure config exists and load it
    let mut config = ensure_config_exists();

    // Ensure todo directory exists
    let todo_path = ensure_todo_directory_exists(&config);

    // Get active list path
    let active_list_path = get_active_list_path(&config, &todo_path);

    // Ensure active list file exists
    ensure_active_list_exists(&active_list_path);

    // Parse CLI arguments first to check if user provided any commands
    let cli = Cli::parse();

    // Check if there's piped input AND no subcommand was provided
    let stdin = io::stdin();
    if cli.command.is_none() && !stdin.is_terminal() {
        // Read from stdin
        let reader = BufReader::new(stdin);
        for line in reader.lines() {
            if let Ok(todo_text) = line {
                let trimmed = todo_text.trim();
                if !trimmed.is_empty() {
                    add_todo(&config, trimmed.to_string(), None);
                }
            }
        }
        return;
    }

    match &cli.command {
        Some(Commands::Lists) => {
            list_todos(&config);
        }
        Some(Commands::List { list }) => {
            display_todo_list(&config, list.clone());
        }
        Some(Commands::Show { list }) => {
            if let Err(e) = show_tui(&config, list.clone()) {
                eprintln!("Error running TUI: {}", e);
            }
        }
        Some(Commands::Use { list_name }) => {
            use_list(&mut config, list_name.clone());
        }
        Some(Commands::Add { todo, list }) => {
            add_todo(&config, todo.clone(), list.clone());
        }
        Some(Commands::Edit) => {
            edit_list(&config);
        }
        None => {
            println!("Active list: {}.{}", config.todo.active_list, config.todo.list_extension);
            println!("Use --help to see available commands");
        }
    }
}
