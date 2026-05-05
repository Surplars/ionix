//! Ionix - Rust-native kernel configuration tool.
//!
//! An interactive TUI for configuring kernel options via TOML schemas,
//! with type-safe output and dependency resolution.
//!
//! ## Library Usage
//!
//! ### Generate from defaults (build.rs)
//! ```rust,no_run
//! fn main() {
//!     ionix::generate("kernel.kconfig", "generated_config.rs")
//!         .expect("failed to generate config");
//! }
//! ```
//!
//! ### Generate from existing config
//! ```rust,no_run
//! fn main() {
//!     ionix::generate_with_config("kernel.kconfig", "generated_config.rs", Some("config.toml"))
//!         .expect("failed to generate config");
//! }
//! ```
//!
//! ## CLI Usage
//! ```bash
//! ionix --schema <file> --batch              # Generate from defaults
//! ionix --schema <file> --batch -c config.toml  # Generate from config
//! ionix --schema <file> --config config.toml   # Interactive TUI
//! ```

pub mod config;
pub mod schema;

use crate::schema::ConfigSchema;
use anyhow::{bail, Context, Result};
pub use config::ConfigLoader;
use schema::codegen::ConfigValues;
pub use schema::ConfigType;
use std::path::{Path, PathBuf};

/// Options for preparing a complete config file and generated Rust output.
#[derive(Debug, Clone)]
pub struct PrepareOptions {
    pub schema_path: PathBuf,
    pub config_path: PathBuf,
    pub output_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub overrides: ConfigValues,
}

impl PrepareOptions {
    pub fn new(
        schema_path: impl Into<PathBuf>,
        config_path: impl Into<PathBuf>,
        output_path: impl Into<PathBuf>,
    ) -> Self {
        Self {
            schema_path: schema_path.into(),
            config_path: config_path.into(),
            output_path: output_path.into(),
            backup_path: None,
            overrides: ConfigValues::new(),
        }
    }

    pub fn with_backup_path(mut self, backup_path: impl Into<PathBuf>) -> Self {
        self.backup_path = Some(backup_path.into());
        self
    }

    pub fn with_overrides(mut self, overrides: ConfigValues) -> Self {
        self.overrides = overrides;
        self
    }
}

/// Result of preparing configuration.
#[derive(Debug, Clone)]
pub struct PrepareResult {
    pub values: ConfigValues,
    pub config_written: bool,
}

/// Load schema/config, merge defaults, apply optional overrides, validate, save
/// the full config file, then generate Rust constants.
pub fn prepare(options: PrepareOptions) -> Result<PrepareResult> {
    let schema = ConfigSchema::from_path(&options.schema_path)
        .with_context(|| format!("Failed to load schema: {}", options.schema_path.display()))?;
    let loader = ConfigLoader::new(&options.schema_path, &options.output_path);
    let loaded = loader.load(Some(&options.config_path))?;
    let mut values = ConfigLoader::merge_with_defaults(&loaded.values, &schema);

    for (key, value) in options.overrides {
        values.insert(key, value);
    }

    validate_values(&values, &schema)?;

    let config_written = loader.save_with_schema_if_changed(
        &values,
        &schema,
        &options.config_path,
        options.backup_path.as_deref(),
    )?;
    loader.generate(&schema, &values)?;

    Ok(PrepareResult {
        values,
        config_written,
    })
}

/// Generate Rust config code from a schema file.
///
/// This is the main entry point for build.rs scripts.
///
/// # Arguments
/// * `schema_path` - Path to the TOML schema file
/// * `output_path` - Path where the generated Rust code will be written
///
/// # Example
///
/// ```rust,no_run
/// fn main() -> anyhow::Result<()> {
///     ionix::generate("kernel.kconfig", "generated_config.rs")?;
///     Ok(())
/// }
/// ```
pub fn generate(schema_path: impl AsRef<Path>, output_path: impl AsRef<Path>) -> Result<()> {
    let schema_path = schema_path.as_ref();
    let output_path = output_path.as_ref();

    let schema = ConfigSchema::from_path(schema_path)
        .with_context(|| format!("Failed to load schema: {}", schema_path.display()))?;

    // Load config if it exists (use defaults otherwise)
    let config_loader = ConfigLoader::new(schema_path, output_path);
    let result = config_loader.load(None)?;

    let values = ConfigLoader::merge_with_defaults(&result.values, &schema);
    let errors = ConfigLoader::validate(&values, &schema);
    if !errors.is_empty() {
        bail!("config validation failed:\n{}", errors.join("\n"));
    }

    // Ensure output directory exists
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
    }

    let generator = schema::codegen::CodeGenerator::new(&schema, &values);
    generator.write_to_file(output_path)?;

    Ok(())
}

/// Load a config file and merge with schema defaults.
///
/// Returns the merged configuration values ready for code generation.
pub fn load_config(
    schema_path: impl AsRef<Path>,
    config_path: Option<impl AsRef<Path>>,
) -> Result<config::loader::LoadResult> {
    let schema_path = schema_path.as_ref();
    let loader = ConfigLoader::new(schema_path, std::path::PathBuf::from("dummy.rs"));
    match config_path {
        Some(p) => loader.load(Some(p.as_ref())),
        None => loader.load(None),
    }
}

/// Generate Rust config code with explicit values.
pub fn generate_with_config(
    schema_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
    config_path: Option<impl AsRef<Path>>,
) -> Result<()> {
    let schema_path = schema_path.as_ref();
    let output_path = output_path.as_ref();

    let schema = ConfigSchema::from_path(schema_path)
        .with_context(|| format!("Failed to load schema: {}", schema_path.display()))?;

    let loader = ConfigLoader::new(schema_path, output_path);
    let result = match config_path {
        Some(p) => loader.load(Some(p.as_ref()))?,
        None => loader.load(None)?,
    };

    let values = ConfigLoader::merge_with_defaults(&result.values, &schema);
    validate_values(&values, &schema)?;

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
    }

    let generator = schema::codegen::CodeGenerator::new(&schema, &values);
    generator.write_to_file(output_path)?;

    Ok(())
}

/// Compare a config file with a schema and write annotated differences.
///
/// # Arguments
/// * `schema_path` - Path to the TOML schema file
/// * `config_path` - Path to the config file to annotate
pub fn diff(schema_path: impl AsRef<Path>, config_path: impl AsRef<Path>) -> Result<()> {
    let schema_path = schema_path.as_ref();
    let config_path = config_path.as_ref();

    let schema = ConfigSchema::from_path(schema_path)
        .with_context(|| format!("Failed to load schema: {}", schema_path.display()))?;

    // Read the config file
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read config: {}", config_path.display()))?;

    // Remove existing ionix annotations to avoid duplicates
    let clean_content = remove_existing_annotations(&content);

    // Parse config values (skip comments)
    let values: ConfigValues = toml::from_str(&clean_content)
        .with_context(|| format!("Failed to parse config: {}", config_path.display()))?;

    // Missing keys (in schema but not in config)
    let mut missing = Vec::new();
    for item in schema.items() {
        if !values.contains_key(&item.key) {
            missing.push((item.key.clone(), item.default.clone()));
        }
    }

    // Unknown keys (in config but not in schema)
    let mut unknown: Vec<String> = Vec::new();
    for key in values.keys() {
        if schema.get(key).is_none() {
            unknown.push(key.clone());
        }
    }

    // Sort unknown keys by length descending to match longer keys first
    unknown.sort_by(|a, b| b.len().cmp(&a.len()));

    // Calculate max line length for alignment
    let max_line_len = clean_content
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim().starts_with('#'))
        .map(|l| l.len())
        .max()
        .unwrap_or(40)
        .max(40);

    // Build the annotated content
    let mut output = String::new();

    // Header
    output.push_str("# Generated by ionix - DO NOT EDIT\n");
    output.push_str("# ==============================\n\n");

    // Original content with Unknown annotations
    for line in clean_content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            output.push('\n');
            continue;
        }

        // Skip lines that are already annotations
        if trimmed.starts_with('#') {
            continue;
        }

        // Extract the key from this line (key is at the start, before =)
        let line_key = trimmed
            .split(|c| c == '=' || c == ' ')
            .next()
            .unwrap_or("")
            .trim();

        let is_unknown = unknown.iter().any(|k| k == line_key);
        if is_unknown {
            // Pad line to max length for alignment, then add annotation
            let padded = format!("{:<width$} # Unknown", line, width = max_line_len);
            output.push_str(&padded);
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }

    // Separator and missing entries
    output.push_str("\n# ==============================\n");
    output.push_str("# Missing entries (using schema defaults):\n");
    for (key, default_val) in &missing {
        output.push_str(&format!("# {} = {}\n", key, default_val));
    }

    // Summary
    if missing.is_empty() && unknown.is_empty() {
        output.push_str("\n# No differences found - config matches schema.\n");
    } else {
        output.push_str(&format!(
            "\n# Summary: {} missing, {} unknown\n",
            missing.len(),
            unknown.len()
        ));
    }

    // Write back to config file
    std::fs::write(config_path, &output)
        .with_context(|| format!("Failed to write config: {}", config_path.display()))?;

    Ok(())
}

pub fn validate_values(values: &ConfigValues, schema: &ConfigSchema) -> Result<()> {
    let errors = ConfigLoader::validate(values, schema);
    if !errors.is_empty() {
        bail!("config validation failed:\n{}", errors.join("\n"));
    }
    Ok(())
}

/// Remove existing ionix annotations from config content to avoid duplicates.
fn remove_existing_annotations(content: &str) -> String {
    let mut result = String::new();
    let mut in_annotation_section = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip header
        if trimmed == "# Generated by ionix - DO NOT EDIT" {
            continue;
        }

        // Detect annotation section start
        if trimmed == "# ==============================" {
            in_annotation_section = true;
            continue;
        }

        // If in annotation section, skip until we exit
        if in_annotation_section {
            if trimmed.is_empty() {
                in_annotation_section = false;
            }
            continue;
        }

        // Remove "# Unknown" suffix from config lines
        let clean_line = if trimmed.starts_with('#') {
            trimmed.to_string()
        } else {
            trimmed.trim_end_matches(" # Unknown").to_string()
        };

        result.push_str(&clean_line);
        result.push('\n');
    }

    result
}
