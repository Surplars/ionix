//! Ionix - Interactive kernel configuration TUI.
//!
//! Usage:
//! ionix --schema <file> # Interactive mode
//! ionix --schema <file> --batch # Batch mode (generate to generated_config.rs)
//! ionix --schema <file> --diff --config <file> # Diff mode

#![allow(unused)]

mod config;
mod schema;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use config::ConfigLoader;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use schema::ConfigSchema;
use std::io;
use ui::app::AppState;
use ui::events::{handle_key_event, AppEvent, EventHandler, KeyAction};
use ui::widgets::{ConfigList, HelpPanel, StatusBar};

#[derive(Parser, Debug)]
#[command(name = "ionix")]
#[command(about = "Interactive kernel configuration tool")]
struct Args {
    /// Path to TOML schema file (required)
    #[arg(long = "schema", short = 's', required = true)]
    schema_path: std::path::PathBuf,

    /// Path to config file (optional - uses defaults if not specified or doesn't exist)
    #[arg(long = "config", short = 'c')]
    config_path: Option<std::path::PathBuf>,

    /// Export generated Rust config to file
    #[arg(long = "export", short = 'e')]
    export_path: Option<std::path::PathBuf>,

    /// Non-interactive export mode
    #[arg(long = "batch")]
    batch: bool,

    /// Diff mode: compare config with schema and show differences
    #[arg(long = "diff", short = 'd')]
    diff: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let schema_path = &args.schema_path;

    if !schema_path.exists() {
        anyhow::bail!("Schema file not found: {}", schema_path.display());
    }

    let schema = ConfigSchema::from_path(schema_path)
        .with_context(|| format!("Failed to load schema: {}", schema_path.display()))?;

    if schema.is_empty() {
        anyhow::bail!("Schema file is empty: {}", schema_path.display());
    }

    // Handle diff mode FIRST, before any validation
    if args.diff {
        if let Some(ref config_path) = args.config_path {
            if !config_path.exists() {
                anyhow::bail!("Diff mode requires an existing config file");
            }
            ionix::diff(schema_path, config_path)?;
            println!("Diff complete: {}", config_path.display());
            return Ok(());
        } else {
            anyhow::bail!("Diff mode requires -c/--config option");
        }
    }

    // If config_path is specified but doesn't exist, warn and continue with defaults
    if let Some(ref config_path) = args.config_path {
        if !config_path.exists() {
            eprintln!(
                "Warning: Config file '{}' does not exist, using defaults.",
                config_path.display()
            );
        } else {
            // Config exists - validate it matches schema strictly
            let loader = ConfigLoader::new(
                schema_path.clone(),
                std::path::PathBuf::from("generated_config.rs"),
            );
            match loader.load(Some(config_path)) {
                Ok(result) => {
                    // Strict validation: check for missing keys and type mismatches
                    let errors = ConfigLoader::validate(&result.values, &schema);
                    if !errors.is_empty() {
                        for err in &errors {
                            eprintln!("Error: {}", err);
                        }
                        eprintln!("Config file validation failed. Exiting.");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Error: Config file '{}' is invalid: {}",
                        config_path.display(),
                        e
                    );
                    std::process::exit(1);
                }
            }
        }
    }

    if args.batch {
        run_batch(&schema, &args)?;
    } else {
        // Track if user provided an external config file
        let has_external_config = args
            .config_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false);
        run_tui(&schema, &args, has_external_config)?;
    }

    Ok(())
}

fn run_tui(schema: &ConfigSchema, args: &Args, has_external_config: bool) -> Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(io::stdout(), crossterm::cursor::Hide)?;
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = AppState::new(schema.clone());

    let loader = ConfigLoader::new(
        args.schema_path.clone(),
        args.export_path
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("generated_config.rs")),
    );

    // Try to load config if specified and exists
    if let Some(ref config_path) = args.config_path {
        if config_path.exists() {
            if let Ok(result) = loader.load(Some(config_path)) {
                app.load_values(result.values);
                app.config_path = Some(result.path);
                // Warnings already handled in main()
            }
        }
    }

    let mut events = EventHandler::new();
    let mut running = true;

    while running {
        terminal.draw(|f| {
            let size = f.area();
            render_ui(f, &mut app, size);
        })?;

        match events.next()? {
            AppEvent::Key(key) => {
                match handle_key_event(&mut app, key) {
                    Some(KeyAction::Quit) => {
                        // Auto-save on quit:
                        // - No external config (-c not specified): always save
                        // - External config exists: only save if modified
                        let should_save = !has_external_config || !app.modified.is_empty();
                        if should_save {
                            let values = app.effective_values();
                            match loader.save_with_schema_if_changed(
                                &values,
                                &app.schema,
                                std::path::Path::new(".config.toml"),
                                Some(std::path::Path::new(".config.old.toml")),
                            ) {
                                Ok(true) => {
                                    let _ = loader.generate(&app.schema, &values);
                                    println!("Saved config to .config.toml");
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    eprintln!("Warning: Save failed: {}", e);
                                }
                            }
                        }
                        running = false;
                    }
                    Some(KeyAction::QuitWithSavePrompt) => {
                        // Same logic as Quit
                        let should_save = !has_external_config || !app.modified.is_empty();
                        if should_save {
                            let values = app.effective_values();
                            match loader.save_with_schema_if_changed(
                                &values,
                                &app.schema,
                                std::path::Path::new(".config.toml"),
                                Some(std::path::Path::new(".config.old.toml")),
                            ) {
                                Ok(true) => {
                                    let _ = loader.generate(&app.schema, &values);
                                    println!("Saved config to .config.toml");
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    eprintln!("Warning: Save failed: {}", e);
                                }
                            }
                        }
                        running = false;
                    }
                    Some(KeyAction::Save) => {
                        // Manual save (for explicit save before exit)
                        let values = app.effective_values();
                        match loader.save_with_schema_if_changed(
                            &values,
                            &app.schema,
                            std::path::Path::new(".config.toml"),
                            Some(std::path::Path::new(".config.old.toml")),
                        ) {
                            Ok(true) => {
                                let _ = loader.generate(&app.schema, &values);
                                app.set_status("Saved -> .config.toml".to_string());
                            }
                            Ok(false) => {
                                app.set_status("No changes to save".to_string());
                            }
                            Err(e) => {
                                app.set_error(format!("Save failed: {}", e));
                            }
                        }
                    }
                    Some(KeyAction::SaveAndQuit) => {
                        // Save and quit
                        let values = app.effective_values();
                        match loader.save_with_schema_if_changed(
                            &values,
                            &app.schema,
                            std::path::Path::new(".config.toml"),
                            Some(std::path::Path::new(".config.old.toml")),
                        ) {
                            Ok(true) => {
                                let _ = loader.generate(&app.schema, &values);
                                println!("Saved config to .config.toml");
                            }
                            Ok(false) => {}
                            Err(e) => {
                                eprintln!("Save failed: {}", e);
                            }
                        }
                        running = false;
                    }
                    None => {}
                }
            }
            AppEvent::Resize(_, _) => {}
            AppEvent::Refresh => {}
        }
    }

    restore_terminal()?;
    println!("Goodbye!");

    Ok(())
}

fn restore_terminal() -> Result<()> {
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::cursor::MoveTo(0, 0),
        crossterm::cursor::Show
    )?;
    Ok(())
}

fn run_batch(schema: &ConfigSchema, args: &Args) -> Result<()> {
    let _ = schema;
    let config_path = args
        .config_path
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(".config.toml"));
    let output_path = args
        .export_path
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("generated_config.rs"));
    ionix::prepare(
        ionix::PrepareOptions::new(&args.schema_path, &config_path, &output_path)
            .with_backup_path(std::path::PathBuf::from(".config.old.toml")),
    )?;
    println!("Generated: {}", output_path.display());

    Ok(())
}

fn render_ui(f: &mut ratatui::Frame, app: &mut AppState, size: ratatui::layout::Rect) {
    use ratatui::layout::{Constraint, Direction, Layout};

    if size.height < 10 || size.width < 48 {
        let area = size;
        app.set_visible_height(area.height.saturating_sub(2) as usize);
        let list = ConfigList::new();
        f.render_stateful_widget(list, area, app);
        if app.save_dialog {
            render_save_dialog(f, size);
        }
        return;
    }

    let help_height = if app.show_help {
        size.height.saturating_sub(10).min(10).max(5)
    } else {
        3
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(6),
            Constraint::Length(help_height),
            Constraint::Length(3),
        ])
        .split(size);

    app.set_visible_height(chunks[0].height.saturating_sub(2) as usize);

    let list = ConfigList::new();
    f.render_stateful_widget(list, chunks[0], app);

    let help = HelpPanel::new();
    f.render_stateful_widget(help, chunks[1], app);

    let status = StatusBar::new();
    f.render_stateful_widget(status, chunks[2], app);

    // Overlay save dialog on top of everything
    if app.save_dialog {
        render_save_dialog(f, size);
    }
}

fn render_save_dialog(f: &mut ratatui::Frame, size: ratatui::layout::Rect) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Style};
    use ratatui::text::Line;
    use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};

    // Clear the area behind the dialog
    let dialog_area = ratatui::layout::Rect {
        x: size.width.saturating_sub(50) / 2,
        y: size.height.saturating_sub(7) / 2,
        width: 50.min(size.width),
        height: 7.min(size.height),
    };
    f.render_widget(Clear, dialog_area);

    let block = Block::bordered()
        .title(" Unsaved Changes ")
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(dialog_area);
    block.render(dialog_area, f.buffer_mut());

    let lines = vec![
        Line::styled(
            " You have unsaved changes.",
            Style::default().fg(Color::White),
        ),
        Line::styled("", Style::default()),
        Line::styled(
            " [Y] Save and quit [N] Quit without save",
            Style::default().fg(Color::Cyan),
        ),
        Line::styled(" [Esc] Cancel", Style::default().fg(Color::DarkGray)),
    ];

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);
}
