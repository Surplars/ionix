//! TOML schema definitions and parsing for kernel configuration.
//!
//! Configuration schema is defined via TOML files, enabling type-safe
//! config items with dependency checking and validation, and nested menus.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::Path;

/// Represents a menu/category that can contain child items (like Linux menuconfig).
/// Defined via `[[menus]]` in TOML schema.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigMenu {
    /// Display name (e.g., "Network Options")
    pub name: String,
    /// Unique identifier (e.g., "NETWORK")
    pub key: String,
    /// Description shown when menu is selected
    #[serde(default)]
    pub help: String,
    /// Keys this menu depends on (AND relationship) - menu only visible when deps are met
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Parent menu key for UI nesting. This is independent from config dependencies.
    #[serde(default)]
    pub parent: Option<String>,
    /// Child config items in this menu (inline in TOML)
    #[serde(default)]
    pub items: Vec<ConfigItem>,
    /// If true, this is a visible menu entry (not just a grouping)
    #[serde(default)]
    pub visible: bool,
}

/// Built-in config types supported by Ionix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    Usize,
    String,
}

impl ConfigType {
    pub fn is_unsigned(&self) -> bool {
        matches!(
            self,
            ConfigType::U8
                | ConfigType::U16
                | ConfigType::U32
                | ConfigType::U64
                | ConfigType::Usize
        )
    }

    pub fn rust_type(&self) -> &'static str {
        match self {
            ConfigType::Bool => "bool",
            ConfigType::U8 => "u8",
            ConfigType::U16 => "u16",
            ConfigType::U32 => "u32",
            ConfigType::U64 => "u64",
            ConfigType::Usize => "usize",
            ConfigType::String => "&'static str",
        }
    }
}

/// A single configuration item in the schema.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigItem {
    /// Display name (e.g., "Network Stack")
    pub name: String,

    /// Type identifier (e.g., "NET_STACK")
    pub key: String,

    /// Data type for this config item.
    #[serde(rename = "type")]
    pub config_type: ConfigType,

    /// Default value (serialized as string in TOML).
    pub default: toml::Value,

    /// Help/description text shown in UI.
    #[serde(default)]
    pub help: String,

    /// Keys this item depends on (AND relationship).
    #[serde(default)]
    pub depends_on: Vec<String>,

    /// Keys that cannot be enabled together with this item.
    #[serde(default)]
    pub conflicts_with: Vec<String>,

    /// If true, this option is visible only in expert mode.
    #[serde(default)]
    pub expert: bool,

    /// If true, changes require kernel rebuild.
    #[serde(default)]
    pub rebuild: bool,

    /// Deprecation info, if any.
    #[serde(default)]
    pub deprecated: Option<Deprecation>,

    /// Menu/category this item belongs to (for nested menu support).
    /// Items with the same menu key will be grouped together in the UI.
    #[serde(default)]
    pub menu: Option<String>,
}

/// Deprecation metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct Deprecation {
    pub message: String,
    #[serde(default)]
    pub replaced_by: Option<String>,
}

/// The parsed configuration schema containing all config items and menus.
#[derive(Debug, Clone, Default)]
pub struct ConfigSchema {
    items: Vec<ConfigItem>,
    menus: Vec<ConfigMenu>,
    lookup: HashMap<String, usize>,
    menu_lookup: HashMap<String, usize>,
}

/// TOML wrapper for array-of-tables parsing.
#[derive(Deserialize)]
struct SchemaToml {
    #[serde(default)]
    items: Vec<ConfigItem>,
    #[serde(default)]
    menus: Vec<ConfigMenu>,
}

impl ConfigSchema {
    /// Parse a TOML schema file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read schema file: {}", path.as_ref().display()))?;

        Self::from_str(&content)
    }

    /// Parse TOML schema from string content.
    pub fn from_str(content: &str) -> Result<Self> {
        let toml: SchemaToml = toml::from_str(content).context("Failed to parse TOML schema")?;

        let mut schema = ConfigSchema {
            lookup: HashMap::with_capacity(toml.items.len()),
            menu_lookup: HashMap::new(),
            items: Vec::new(),
            menus: Vec::new(),
        };

        // First pass: collect all top-level items
        for (idx, item) in toml.items.into_iter().enumerate() {
            schema.lookup.insert(item.key.clone(), idx);
            schema.items.push(item);
        }

        // Second pass: process menus and their items
        for menu in toml.menus {
            let menu_key = menu.key.clone();
            let menu_items = menu.items.clone(); // Clone items for iteration
            let base_idx = schema.items.len();

            // Set menu key for all items in this menu
            for (i, mut item) in menu_items.into_iter().enumerate() {
                // If item doesn't have a menu set, assign this one
                if item.menu.is_none() {
                    item.menu = Some(menu_key.clone());
                }
                let full_idx = base_idx + i;
                schema.lookup.insert(item.key.clone(), full_idx);
                schema.items.push(item);
            }

            // Store menu for UI rendering
            schema
                .menu_lookup
                .insert(menu_key.clone(), schema.menus.len());
            schema.menus.push(menu);
        }

        schema.validate()?;
        Ok(schema)
    }

    /// Get all menus.
    pub fn menus(&self) -> &[ConfigMenu] {
        &self.menus
    }

    /// Get menu by key.
    pub fn get_menu(&self, key: &str) -> Option<&ConfigMenu> {
        self.menu_lookup.get(key).map(|&idx| &self.menus[idx])
    }

    /// Get the menu that contains a config item.
    pub fn get_menu_for_item(&self, item_key: &str) -> Option<&ConfigMenu> {
        let item = self.get(item_key)?;
        item.menu.as_ref().and_then(|m| self.get_menu(m))
    }

    /// Check if schema has any menus.
    pub fn has_menus(&self) -> bool {
        !self.menus.is_empty()
    }

    /// Get items grouped by their menu.
    pub fn items_by_menu(&self) -> HashMap<Option<String>, Vec<&ConfigItem>> {
        let mut groups: HashMap<Option<String>, Vec<&ConfigItem>> = HashMap::new();
        for item in &self.items {
            groups.entry(item.menu.clone()).or_default().push(item);
        }
        groups
    }

    /// Get all config items.
    pub fn items(&self) -> &[ConfigItem] {
        &self.items
    }

    /// Get item by key.
    pub fn get(&self, key: &str) -> Option<&ConfigItem> {
        self.lookup.get(key).map(|&idx| &self.items[idx])
    }

    /// Get index by key.
    pub fn get_index_by_key(&self, key: &str) -> Option<usize> {
        self.lookup.get(key).copied()
    }

    /// Get item by index.
    pub fn get_index(&self, index: usize) -> Option<&ConfigItem> {
        self.items.get(index)
    }

    /// Total number of config items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if schema is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Validate the schema for consistency.
    fn validate(&self) -> Result<()> {
        let keys: HashSet<_> = self.items.iter().map(|i| &i.key).collect();
        let menu_keys: HashSet<_> = self.menus.iter().map(|m| &m.key).collect();

        for item in &self.items {
            // Validate default value type matches config type
            Self::validate_default_type(&item.default, item.config_type, &item.key)?;

            // Validate dependencies exist
            for dep in &item.depends_on {
                if !keys.contains(dep) {
                    bail!(
                        "Config item '{}' depends on non-existent key '{}'",
                        item.key,
                        dep
                    );
                }
            }

            for conflict in &item.conflicts_with {
                if !keys.contains(conflict) {
                    bail!(
                        "Config item '{}' conflicts with non-existent key '{}'",
                        item.key,
                        conflict
                    );
                }
            }
        }

        for menu in &self.menus {
            if let Some(parent) = &menu.parent {
                if !menu_keys.contains(parent) {
                    bail!("Menu '{}' has non-existent parent '{}'", menu.key, parent);
                }
                if parent == &menu.key {
                    bail!("Menu '{}' cannot be its own parent", menu.key);
                }
            }

            for dep in &menu.depends_on {
                if !keys.contains(dep) {
                    bail!(
                        "Menu '{}' depends on non-existent config key '{}'",
                        menu.key,
                        dep
                    );
                }
            }
        }

        for menu in &self.menus {
            let mut seen = HashSet::new();
            let mut current = menu.parent.as_deref();
            while let Some(parent_key) = current {
                if !seen.insert(parent_key) {
                    bail!("Menu '{}' has a cyclic parent chain", menu.key);
                }
                current = self
                    .get_menu(parent_key)
                    .and_then(|parent| parent.parent.as_deref());
            }
        }

        Ok(())
    }

    fn validate_default_type(value: &toml::Value, expected: ConfigType, key: &str) -> Result<()> {
        // Handle large hex/binary strings as u64 defaults (e.g., "0xFFFF_8000_0000_0000")
        if let toml::Value::String(s) = value {
            if expected.is_unsigned() && Self::unsigned_literal_fits(s, expected) {
                return Ok(());
            }
        }

        match (value, expected) {
            (toml::Value::Boolean(_), ConfigType::Bool) => Ok(()),
            (toml::Value::Integer(i), ConfigType::U8) if (0..=255).contains(i) => Ok(()),
            (toml::Value::Integer(i), ConfigType::U16) if (0..=65535).contains(i) => Ok(()),
            (toml::Value::Integer(i), ConfigType::U32) if *i >= 0 => Ok(()),
            (toml::Value::Integer(i), ConfigType::U64) if *i >= 0 => Ok(()),
            (toml::Value::Integer(i), ConfigType::Usize) if *i >= 0 => Ok(()),
            (toml::Value::String(_), ConfigType::String) => Ok(()),
            _ => bail!(
                "Config item '{}' has invalid default type. Expected {:?}, got {:?}",
                key,
                expected,
                value
            ),
        }
    }

    fn unsigned_literal_fits(s: &str, expected: ConfigType) -> bool {
        parse_unsigned_literal(s)
            .map(|value| value <= max_value_for_type(expected))
            .unwrap_or(false)
    }

    /// Get all items that depend on the given key.
    pub fn dependents_of(&self, key: &str) -> Vec<&ConfigItem> {
        self.items
            .iter()
            .filter(|item| item.depends_on.contains(&key.to_string()))
            .collect()
    }

    /// Check if enabling `key` would create a dependency cycle.
    pub fn has_cycle(&self, key: &str) -> bool {
        let mut visited = HashSet::new();
        let mut stack: Vec<&str> = vec![key];

        while let Some(current) = stack.pop() {
            if visited.contains(current) {
                return true;
            }
            visited.insert(current.to_string());

            if let Some(item) = self.get(current) {
                for dep in &item.depends_on {
                    stack.push(dep.as_str());
                }
            }
        }
        false
    }
}

fn max_value_for_type(config_type: ConfigType) -> u128 {
    match config_type {
        ConfigType::U8 => u8::MAX as u128,
        ConfigType::U16 => u16::MAX as u128,
        ConfigType::U32 => u32::MAX as u128,
        ConfigType::U64 => u64::MAX as u128,
        ConfigType::Usize => usize::MAX as u128,
        ConfigType::Bool | ConfigType::String => 0,
    }
}

fn parse_unsigned_literal(s: &str) -> Option<u128> {
    let (base, digits) = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, rest)
    } else if let Some(rest) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        (2, rest)
    } else {
        return None;
    };

    if digits.is_empty() || !digits.chars().any(|c| c != '_') {
        return None;
    }

    let mut value = 0u128;
    for ch in digits.chars() {
        if ch == '_' {
            continue;
        }
        let digit = ch.to_digit(base)?;
        value = value.checked_mul(base as u128)?;
        value = value.checked_add(digit as u128)?;
    }
    Some(value)
}

impl fmt::Display for ConfigType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigType::Bool => write!(f, "bool"),
            ConfigType::U8 => write!(f, "u8"),
            ConfigType::U16 => write!(f, "u16"),
            ConfigType::U32 => write!(f, "u32"),
            ConfigType::U64 => write!(f, "u64"),
            ConfigType::Usize => write!(f, "usize"),
            ConfigType::String => write!(f, "string"),
        }
    }
}

/// Error type for schema operations.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("Parse error: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Validation failed: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_schema() {
        let toml = r#"
[[items]]
name = "Network Stack"
key = "NET_STACK"
type = "bool"
default = true
help = "Enable TCP/IP networking stack"

[[items]]
name = "Max IRQ"
key = "MAX_IRQ"
type = "u64"
default = 256
help = "Maximum number of IRQ handlers"
"#;
        let schema = ConfigSchema::from_str(toml).unwrap();
        assert_eq!(schema.len(), 2);

        let net = schema.get("NET_STACK").unwrap();
        assert_eq!(net.name, "Network Stack");
    }

    #[test]
    fn test_validate_dependencies() {
        let toml = r#"
[[items]]
name = "A"
key = "A"
type = "bool"
default = false

[[items]]
name = "B"
key = "B"
type = "bool"
default = false
depends_on = ["A"]
"#;
        let schema = ConfigSchema::from_str(toml).unwrap();
        assert_eq!(schema.dependents_of("A")[0].key, "B");
    }

    #[test]
    fn test_invalid_default_type() {
        let toml = r#"
[[items]]
name = "Test"
key = "TEST"
type = "bool"
default = "not a bool"
"#;
        assert!(ConfigSchema::from_str(toml).is_err());
    }

    #[test]
    fn test_nested_menu_parent() {
        let toml = r#"
[[menus]]
name = "Kernel"
key = "KERNEL"

[[menus]]
name = "Scheduler"
key = "SCHED"
parent = "KERNEL"

[[menus.items]]
name = "Tick Rate"
key = "TICK_RATE"
type = "u32"
default = 1000
"#;
        let schema = ConfigSchema::from_str(toml).unwrap();
        assert_eq!(
            schema.get_menu("SCHED").unwrap().parent.as_deref(),
            Some("KERNEL")
        );
        assert_eq!(
            schema.get("TICK_RATE").unwrap().menu.as_deref(),
            Some("SCHED")
        );
    }

    #[test]
    fn test_nested_menu_cycle_is_rejected() {
        let toml = r#"
[[menus]]
name = "A"
key = "A"
parent = "B"

[[menus]]
name = "B"
key = "B"
parent = "A"
"#;
        assert!(ConfigSchema::from_str(toml).is_err());
    }
}
