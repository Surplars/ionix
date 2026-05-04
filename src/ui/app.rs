//! Application state management for the Ionix TUI.

#![allow(non_snake_case)]

use crate::schema::codegen::ConfigValues;
use crate::schema::{ConfigItem, ConfigSchema, ConfigType};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;

#[allow(dead_code)]
const EXPERT_MODE_THRESHOLD: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterMode {
    All,
    Modified,
    Expert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    None,
    Searching,
}

/// Represents a menu entry in the UI list (can be a menu header or config item).
#[derive(Debug, Clone)]
pub enum ListEntry {
    /// A menu/category header (e.g., "Network Options ---")
    Menu {
        key: String,
        name: String,
        expanded: bool,
    },
    /// A config item
    Item { schema_idx: usize, item: ConfigItem },
    /// Exit from submenu (shown at bottom of menu pages)
    /// Back to parent menu entry
    BackMenu { menu_key: String },
}

/// Inline editing state for numeric/string values.
#[derive(Debug, Clone)]
pub struct InlineEdit {
    pub key: String,
    pub value: String,
    pub cursor: usize,
    pub is_numeric: bool,
}

/// The main application state for the Ionix TUI.
pub struct AppState {
    pub schema: ConfigSchema,
    pub values: ConfigValues,
    pub modified: HashMap<String, toml::Value>,

    // Navigation
    pub selected_index: usize,
    pub scroll_offset: usize,
    /// Terminal height used for scroll calculations. Updated each render.
    pub visible_height: usize,

    // Filtering & Search
    pub filter_mode: FilterMode,
    pub search_query: String,
    pub search_mode: SearchMode,
    pub filtered_indices: Vec<usize>,

    // UI Modes
    pub expert_mode: bool,
    pub show_help: bool,
    pub inline_edit: Option<InlineEdit>,
    /// When true, show the save-before-quit TUI dialog
    pub save_dialog: bool,

    // Messages
    pub status_message: Option<String>,
    pub error_message: Option<String>,

    // Config file path
    pub config_path: Option<std::path::PathBuf>,

    // Menu state: which menus are expanded
    pub expanded_menus: HashMap<String, bool>,
    /// All visible entries (including menu headers) for current filter
    pub list_entries: Vec<ListEntry>,

    /// When in a submenu, this is the menu key. None = at root.
    pub current_menu: Option<String>,
    /// Parent menu key (for back navigation)
    pub parent_menu: Option<String>,
}

impl AppState {
    pub fn new(schema: ConfigSchema) -> Self {
        let modified = HashMap::new();
        let filtered_indices =
            Self::compute_filter_indices(&schema, "", FilterMode::All, false, &modified);

        // Initialize all menus as expanded by default
        let mut expanded_menus = HashMap::new();
        for menu in schema.menus() {
            expanded_menus.insert(menu.key.clone(), true);
        }

        // Compute list entries including menu headers
        let list_entries =
            Self::compute_list_entries(&schema, &filtered_indices, &expanded_menus, None);

        Self {
            schema,
            values: HashMap::new(),
            modified,
            selected_index: 0,
            scroll_offset: 0,
            visible_height: 20,
            filter_mode: FilterMode::All,
            search_query: String::new(),
            search_mode: SearchMode::None,
            filtered_indices,
            expert_mode: false,
            show_help: true,
            inline_edit: None,
            save_dialog: false,
            status_message: None,
            error_message: None,
            config_path: None,
            expanded_menus,
            list_entries,
            current_menu: None,
            parent_menu: None,
        }
    }

    /// Enter a submenu (full-page view)
    pub fn enter_menu(&mut self, menu_key: &str) {
        // Set current menu as parent before entering
        self.parent_menu = self.current_menu.clone();
        self.current_menu = Some(menu_key.to_string());
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.recompute_list();
    }

    /// Exit current submenu and return to parent (or root if no parent)
    pub fn exit_menu(&mut self) {
        self.current_menu = self.parent_menu.clone();
        self.parent_menu = None; // Clear grandparent
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.recompute_list();
    }

    /// Go back to root (from anywhere)
    pub fn go_to_root(&mut self) {
        self.current_menu = None;
        self.parent_menu = None;
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.recompute_list();
    }

    /// Check if currently in a submenu.
    pub fn in_menu(&self) -> bool {
        self.current_menu.is_some()
    }

    /// Get current menu name for title bar.
    pub fn current_menu_name(&self) -> Option<String> {
        self.current_menu
            .as_ref()
            .and_then(|key| self.schema.get_menu(key).map(|m| m.name.clone()))
    }

    /// Compute list entries including menu headers.
    fn compute_list_entries(
        schema: &ConfigSchema,
        filtered_indices: &[usize],
        expanded_menus: &HashMap<String, bool>,
        current_menu: Option<&str>,
    ) -> Vec<ListEntry> {
        let mut entries = Vec::new();

        // Get items grouped by menu
        let items_by_menu = schema.items_by_menu();

        // If in a menu, show only that menu's items (full-page view)
        if let Some(menu_key) = current_menu {
            // Show items in this menu
            if let Some(items) = items_by_menu.get(&Some(menu_key.to_string())) {
                let filtered_items: Vec<_> = items
                    .iter()
                    .filter(|item| {
                        filtered_indices.contains(&schema.get_index_by_key(&item.key).unwrap_or(0))
                    })
                    .collect();

                for item in filtered_items {
                    if let Some(idx) = schema.get_index_by_key(&item.key) {
                        entries.push(ListEntry::Item {
                            schema_idx: idx,
                            item: (*item).clone(),
                        });
                    }
                }
            }

            // Check if this menu has submenus - add them as menu entries
            for menu in schema.menus() {
                // Check if this menu is a child of current menu (depends on current menu)
                if menu.depends_on.iter().any(|d| d == menu_key) {
                    let expanded = *expanded_menus.get(&menu.key).unwrap_or(&true);
                    entries.push(ListEntry::Menu {
                        key: menu.key.clone(),
                        name: menu.name.clone(),
                        expanded,
                    });
                }
            }

            // Add "Back" entry at the bottom to return to parent
            entries.push(ListEntry::BackMenu {
                menu_key: menu_key.to_string(),
            });

            return entries;
        }

        // Root view: show top-level menus AND items without a menu (like Linux menuconfig)
        // First, show items that don't belong to any menu
        if let Some(root_items) = items_by_menu.get(&None) {
            let filtered_items: Vec<_> = root_items
                .iter()
                .filter(|item| {
                    filtered_indices.contains(&schema.get_index_by_key(&item.key).unwrap_or(0))
                })
                .collect();

            for item in filtered_items {
                if let Some(idx) = schema.get_index_by_key(&item.key) {
                    entries.push(ListEntry::Item {
                        schema_idx: idx,
                        item: (*item).clone(),
                    });
                }
            }
        }

        // Then show top-level menus (menus with no dependencies)
        for menu in schema.menus() {
            // Skip menus that have dependencies (they are submenus)
            if !menu.depends_on.is_empty() {
                continue;
            }

            let expanded = *expanded_menus.get(&menu.key).unwrap_or(&true);
            entries.push(ListEntry::Menu {
                key: menu.key.clone(),
                name: menu.name.clone(),
                expanded,
            });
        }

        entries
    }

    /// Toggle a menu's expanded state.
    pub fn toggle_menu(&mut self, menu_key: &str) {
        let expanded = self
            .expanded_menus
            .entry(menu_key.to_string())
            .or_insert(true);
        *expanded = !*expanded;
        self.recompute_list();
    }

    /// Check if the selected entry is a menu.
    pub fn selected_is_menu(&self) -> bool {
        matches!(
            self.list_entries.get(self.selected_index),
            Some(ListEntry::Menu { .. })
        )
    }

    /// Get the menu key at the current selection (if any).
    pub fn selected_menu_key(&self) -> Option<String> {
        match self.list_entries.get(self.selected_index) {
            Some(ListEntry::Menu { key, .. }) => Some(key.clone()),
            _ => None,
        }
    }

    /// Recompute list entries after filter/search changes.
    fn recompute_list(&mut self) {
        self.list_entries = Self::compute_list_entries(
            &self.schema,
            &self.filtered_indices,
            &self.expanded_menus,
            self.current_menu.as_deref(),
        );
        // Clamp selected index
        if self.selected_index >= self.list_entries.len() {
            self.selected_index = self.list_entries.len().saturating_sub(1);
        }
    }

    pub fn recompute_current_filter_preserving(&mut self, key: &str) {
        self.filtered_indices = Self::compute_filter_indices(
            &self.schema,
            &self.search_query,
            self.filter_mode,
            self.expert_mode,
            &self.modified,
        );
        self.recompute_list();
        if let Some(index) = self
            .list_entries
            .iter()
            .position(|entry| matches!(entry, ListEntry::Item { item, .. } if item.key == key))
        {
            self.selected_index = index;
        }
        self.ensure_visible();
    }

    pub fn load_values(&mut self, values: ConfigValues) {
        self.values = values;
    }

    /// Return the full effective config: loaded values plus current edits.
    pub fn effective_values(&self) -> ConfigValues {
        let mut values = ConfigValues::new();
        for item in self.schema.items() {
            values.insert(item.key.clone(), self.effective_value(&item.key).clone());
        }
        values
    }

    /// Get currently visible items based on filter/search state.
    pub fn visible_items(&self) -> Vec<(usize, &ConfigItem)> {
        self.filtered_indices
            .iter()
            .filter_map(|&idx| self.schema.get_index(idx).map(|item| (idx, item)))
            .collect()
    }

    /// Get the currently selected list entry.
    pub fn selected_entry(&self) -> Option<&ListEntry> {
        self.list_entries.get(self.selected_index)
    }

    /// Get the currently selected config item (if not a menu).
    pub fn selected_item(&self) -> Option<(usize, &ConfigItem)> {
        match self.list_entries.get(self.selected_index) {
            Some(ListEntry::Item { schema_idx, item }) => Some((*schema_idx, item)),
            _ => None,
        }
    }

    /// Get the effective value for a config item (modified or default).
    pub fn effective_value(&self, key: &str) -> &toml::Value {
        self.modified
            .get(key)
            .or_else(|| self.values.get(key))
            .or_else(|| self.schema.get(key).map(|i| &i.default))
            .unwrap()
    }

    /// Check if a config item has been modified.
    pub fn is_modified(&self, key: &str) -> bool {
        self.modified.contains_key(key)
    }

    /// Get the value to display (considering modified state).
    pub fn display_value(&self, item: &ConfigItem) -> String {
        let effective = self.effective_value(&item.key);

        match item.config_type {
            ConfigType::Bool => effective
                .as_bool()
                .map(|b| if b { "[*]" } else { "[ ]" })
                .unwrap_or("[?]")
                .to_string(),
            ConfigType::String => effective.as_str().unwrap_or("").to_string(),
            ConfigType::U8
            | ConfigType::U16
            | ConfigType::U32
            | ConfigType::U64
            | ConfigType::Usize => effective
                .as_integer()
                .map(|i| i.to_string())
                .unwrap_or_default(),
        }
    }

    /// Toggle a boolean config item.
    pub fn toggle_bool(&mut self, idx: usize) {
        if let Some(item) = self.schema.get_index(idx) {
            if item.config_type == ConfigType::Bool {
                let current = self.effective_value(&item.key).as_bool().unwrap_or(false);
                let key = item.key.clone();
                let next = !current;
                self.modified
                    .insert(item.key.clone(), toml::Value::Boolean(next));
                if next {
                    for conflict in &item.conflicts_with {
                        self.modified
                            .insert(conflict.clone(), toml::Value::Boolean(false));
                    }
                }
                self.recompute_current_filter_preserving(&key);
            }
        }
    }

    /// Set value for a numeric config item.
    pub fn set_numeric(&mut self, idx: usize, value: i64) {
        if let Some(item) = self.schema.get_index(idx) {
            let key = item.key.clone();
            let tom_l_val = match item.config_type {
                ConfigType::U8 if (0..=255).contains(&value) => Some(toml::Value::Integer(value)),
                ConfigType::U16 if (0..=65535).contains(&value) => {
                    Some(toml::Value::Integer(value))
                }
                ConfigType::U32 | ConfigType::U64 | ConfigType::Usize if value >= 0 => {
                    Some(toml::Value::Integer(value))
                }
                _ => None,
            };

            if let Some(val) = tom_l_val {
                self.modified.insert(item.key.clone(), val);
                self.recompute_current_filter_preserving(&key);
            }
        }
    }

    /// Set value for a string config item.
    pub fn set_string(&mut self, idx: usize, value: String) {
        if let Some(item) = self.schema.get_index(idx) {
            if item.config_type == ConfigType::String {
                let key = item.key.clone();
                self.modified
                    .insert(item.key.clone(), toml::Value::String(value));
                self.recompute_current_filter_preserving(&key);
            }
        }
    }

    /// Revert changes for a specific item.
    pub fn revert_item(&mut self, key: &str) {
        self.modified.remove(key);
        self.recompute_current_filter_preserving(key);
    }

    /// Revert all changes.
    pub fn revert_all(&mut self) {
        self.modified.clear();
        self.recompute_filter();
    }

    /// Check if all dependencies are satisfied for an item.
    pub fn dependencies_satisfied(&self, item: &ConfigItem) -> bool {
        item.depends_on
            .iter()
            .all(|dep| self.effective_value(dep).as_bool().unwrap_or(false))
    }

    /// Get items that would be affected if we disable a key.
    pub fn affected_items(&self, key: &str) -> Vec<&ConfigItem> {
        self.schema.dependents_of(key)
    }

    /// Check if there are unsatisfied dependencies that would prevent enabling.
    pub fn check_enable_blockers(&self, item: &ConfigItem) -> Vec<String> {
        item.depends_on
            .iter()
            .filter(|dep| !self.effective_value(dep).as_bool().unwrap_or(false))
            .cloned()
            .collect()
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.ensure_visible();
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        if self.selected_index < self.list_entries.len().saturating_sub(1) {
            self.selected_index += 1;
            self.ensure_visible();
        }
    }

    /// Move selection by pages.
    pub fn move_page_up(&mut self) {
        let page = self.visible_height.saturating_sub(1).max(1);
        self.selected_index = self.selected_index.saturating_sub(page);
        self.ensure_visible();
    }

    /// Move selection by pages.
    pub fn move_page_down(&mut self) {
        let page = self.visible_height.saturating_sub(1).max(1);
        let max = self.list_entries.len().saturating_sub(1);
        self.selected_index = (self.selected_index + page).min(max);
        self.ensure_visible();
    }

    pub fn set_visible_height(&mut self, height: usize) {
        self.visible_height = height.max(1);
        self.ensure_visible();
    }

    fn ensure_visible(&mut self) {
        // Cursor should always be within visible range
        if self.selected_index < self.scroll_offset {
            // Cursor above visible area - scroll up to show it
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + self.visible_height {
            // Cursor below visible area - scroll down to show it
            self.scroll_offset = self.selected_index - self.visible_height + 1;
        }
    }

    pub fn ensure_selected_visible(&mut self) {
        self.ensure_visible();
    }

    /// Start search mode.
    pub fn start_search(&mut self) {
        self.search_mode = SearchMode::Searching;
        self.search_query.clear();
    }

    /// Exit search mode.
    pub fn exit_search(&mut self) {
        self.search_mode = SearchMode::None;
        self.search_query.clear();
        self.recompute_filter();
    }

    /// Update search query and recompute filtered indices.
    pub fn update_search(&mut self, query: String) {
        self.search_query = query;
        self.recompute_filter();
    }

    /// Set filter mode and recompute.
    pub fn set_filter(&mut self, mode: FilterMode) {
        self.filter_mode = mode;
        self.recompute_filter();
    }

    /// Toggle expert mode.
    pub fn toggle_expert(&mut self) {
        self.expert_mode = !self.expert_mode;
        self.recompute_filter();
    }

    /// Recompute filtered indices based on current search/filter.
    fn recompute_filter(&mut self) {
        self.filtered_indices = Self::compute_filter_indices(
            &self.schema,
            &self.search_query,
            self.filter_mode,
            self.expert_mode,
            &self.modified,
        );
        // Also recompute list entries including menu headers
        self.recompute_list();
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    fn compute_filter_indices(
        schema: &ConfigSchema,
        query: &str,
        filter_mode: FilterMode,
        expert_mode: bool,
        modified: &HashMap<String, toml::Value>,
    ) -> Vec<usize> {
        let matcher = SkimMatcherV2::default();
        let mut results: Vec<(usize, i64)> = Vec::new();

        for idx in 0..schema.len() {
            let Some(item) = schema.get_index(idx) else {
                continue;
            };

            // Filter by expert mode
            if item.expert && !expert_mode {
                continue;
            }

            // Filter by mode
            match filter_mode {
                FilterMode::Modified => {
                    if !modified.contains_key(&item.key) {
                        continue;
                    }
                }
                FilterMode::Expert => {
                    if !item.expert {
                        continue;
                    }
                }
                FilterMode::All => {}
            }

            // Fuzzy search
            if !query.is_empty() {
                let name_score = matcher.fuzzy_match(&item.name, query);
                let key_score = matcher.fuzzy_match(&item.key, query);
                let help_score = matcher.fuzzy_match(&item.help, query);

                let best = [name_score, key_score, help_score]
                    .into_iter()
                    .flatten()
                    .max();

                if let Some(score) = best {
                    results.push((idx, score));
                }
            } else {
                results.push((idx, 0));
            }
        }

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results.into_iter().map(|(idx, _)| idx).collect()
    }

    /// Get statistics about the configuration.
    pub fn stats(&self) -> ConfigStats {
        let total = self.schema.len();
        let modified = self.modified.len();
        let expert = self.schema.items().iter().filter(|i| i.expert).count();

        ConfigStats {
            total,
            modified,
            expert,
            visible: self.filtered_indices.len(),
        }
    }

    /// Set status message to display.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
    }

    /// Set error message to display.
    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.error_message = Some(msg.into());
    }

    /// Clear messages.
    pub fn clear_messages(&mut self) {
        self.status_message = None;
        self.error_message = None;
    }
}

#[derive(Debug, Clone)]
pub struct ConfigStats {
    pub total: usize,
    pub modified: usize,
    pub expert: usize,
    pub visible: usize,
}
