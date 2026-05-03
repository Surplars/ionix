//! Schema definitions and parsing for Ionix configuration.
//!
//! This module handles the TOML-based configuration schema definition,
//! including config items, types, defaults, dependencies, and help text.

pub mod codegen;
pub mod kconfig;

pub use codegen::CodeGenerator;
pub use kconfig::{ConfigItem, ConfigSchema, ConfigType, Deprecation, SchemaError};