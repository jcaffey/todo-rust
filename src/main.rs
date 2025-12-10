use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "todo")]
#[command(about = "A simple todo list manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List all todo files
    Lists,
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

fn main() {
    // Ensure config exists and load it
    let mut config = ensure_config_exists();

    // Ensure todo directory exists
    let todo_path = ensure_todo_directory_exists(&config);

    // Get active list path
    let active_list_path = get_active_list_path(&config, &todo_path);

    // Ensure active list file exists
    ensure_active_list_exists(&active_list_path);

    // Parse CLI arguments
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Lists) => {
            list_todos(&config);
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
