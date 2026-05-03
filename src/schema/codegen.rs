//! Rust code generation from configuration state.
//!
//! Generates `generated_config.rs` containing const definitions
//! that can be included via `include!`.

use super::kconfig::{ConfigSchema, ConfigType};
use std::fmt;
use std::io::Write;
use std::path::Path;

/// Configuration values as parsed from user's config file.
pub type ConfigValues = std::collections::HashMap<String, toml::Value>;

/// Generates Rust code from config schema and user values.
pub struct CodeGenerator<'a> {
    schema: &'a ConfigSchema,
    values: &'a ConfigValues,
    include_help: bool,
}

impl<'a> CodeGenerator<'a> {
    pub fn new(schema: &'a ConfigSchema, values: &'a ConfigValues) -> Self {
        Self {
            schema,
            values,
            include_help: true,
        }
    }

    /// Set whether to include doc comments with help text.
    pub fn with_help(mut self, include: bool) -> Self {
        self.include_help = include;
        self
    }

    /// Generate the complete config file content.
    pub fn generate(&self) -> String {
        let mut out = String::new();
        self.write_header(&mut out);
        self.write_items(&mut out);
        out
    }

    /// Write generated code directly to a file.
    pub fn write_to_file(&self, path: &Path) -> std::io::Result<()> {
        let content = self.generate();
        let mut file = std::fs::File::create(path)?;
        file.write_all(content.as_bytes())?;
        Ok(())
    }

    fn write_header(&self, out: &mut String) {
        out.push_str("//! Auto-generated configuration file.\n");
        out.push_str("//! Do not edit manually - use ionix TUI instead.\n\n");
        out.push_str("#![allow(unused)]\n\n");
        out.push_str("use std::sync::OnceLock;\n\n");
        out.push_str("static CONFIG_LOCK: OnceLock<()> = OnceLock::new();\n\n");
    }

    fn write_items(&self, out: &mut String) {
        for item in self.schema.items() {
            self.write_config_item(out, item);
        }
    }

    fn write_config_item(&self, out: &mut String, item: &super::kconfig::ConfigItem) {
        // Write help as doc comment
        if self.include_help && !item.help.is_empty() {
            for line in item.help.lines() {
                out.push_str("/// ");
                out.push_str(line.trim());
                out.push('\n');
            }
        }

        // Generate const definition
        match item.config_type {
            ConfigType::Bool => {
                let val = self.get_bool(item.key.as_str());
                out.push_str(&format!("pub const {}: bool = {};\n", item.key, val));
            }
            ConfigType::String => {
                let val = self.get_string(item.key.as_str());
                out.push_str(&format!(
                    "pub const {}: &str = {};\n",
                    item.key,
                    Self::escape_string(&val)
                ));
            }
            ConfigType::U64 => {
                // Check if the default is a hex/binary string literal (e.g., "0xFFFF_8000_0000_0000")
                let default = self.schema.get(&item.key).map(|i| &i.default);
                let val_str = if let Some(toml::Value::String(s)) = default {
                    if s.starts_with("0x") || s.starts_with("0b") {
                        s.clone()
                    } else {
                        self.get_int(&item.key).to_string()
                    }
                } else {
                    self.get_int(&item.key).to_string()
                };
                out.push_str(&format!("pub const {}: u64 = {};\n", item.key, val_str));
            }
            ConfigType::U8 | ConfigType::U16 | ConfigType::U32 | ConfigType::Usize => {
                let val = self.get_int(&item.key);
                let rust_type = item.config_type.rust_type();
                out.push_str(&format!("pub const {}: {} = {};\n", item.key, rust_type, val));
            }
        }
        out.push('\n');
    }

    fn get_bool(&self, key: &str) -> bool {
        self.values
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                self.schema
                    .get(key)
                    .and_then(|i| i.default.as_bool())
                    .unwrap_or(false)
            })
    }

    fn get_int(&self, key: &str) -> i64 {
        self.values
            .get(key)
            .and_then(|v| v.as_integer())
            .unwrap_or_else(|| {
                self.schema
                    .get(key)
                    .and_then(|i| i.default.as_integer())
                    .unwrap_or(0)
            })
    }

    fn get_string(&self, key: &str) -> String {
        self.values
            .get(key)
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                self.schema
                    .get(key)
                    .and_then(|i| i.default.as_str())
                    .map(String::from)
                    .unwrap_or_default()
            })
    }

    fn escape_string(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if !c.is_control() => out.push(c),
                c => out.push_str(&format!("\\u{:04x}", c as u32)),
            }
        }
        out.push('"');
        out
    }
}

impl fmt::Display for CodeGenerator<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.generate())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_generate_bool() {
        let toml = r#"
[[items]]
name = "Test"
key = "TEST_BOOL"
type = "bool"
default = false
"#;
        let schema = ConfigSchema::from_str(toml).unwrap();
        let values: ConfigValues = HashMap::new();

        let gen = CodeGenerator::new(&schema, &values);
        let out = gen.generate();

        assert!(out.contains("pub const TEST_BOOL: bool = false;"));
    }

    #[test]
    fn test_generate_string() {
        let toml = r#"
[[items]]
name = "Hostname"
key = "HOSTNAME"
type = "string"
default = "localhost"
"#;
        let schema = ConfigSchema::from_str(toml).unwrap();
        let values: ConfigValues = HashMap::new();

        let gen = CodeGenerator::new(&schema, &values);
        let out = gen.generate();

        assert!(out.contains("pub const HOSTNAME: &str = \"localhost\";"));
    }

    #[test]
    fn test_string_escape() {
        let toml = r#"
[[items]]
name = "Test"
key = "TEST"
type = "string"
default = "hello \"world\"\nnext line"
"#;
        let schema = ConfigSchema::from_str(toml).unwrap();
        let values: ConfigValues = HashMap::new();

        let gen = CodeGenerator::new(&schema, &values);
        let out = gen.generate();

        assert!(out.contains("\\\"world\\\""));
        assert!(out.contains("\\n"));
    }
}