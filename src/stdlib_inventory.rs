// Copyright (C) Brian G. Milnes 2025

//! Rust stdlib inventory types for parsing JSON output from rusticate-analyze-libs.
//!
//! This module provides types that can deserialize the JSON inventory generated
//! by `rusticate-analyze-libs`. Use `StdlibInventory::from_file()` to load an
//! inventory for analysis.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use anyhow::{Context, Result};

/// Root structure for the Rust standard library inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StdlibInventory {
    /// JSON Schema reference (optional)
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Timestamp when inventory was generated
    pub generated: String,
    /// Rust compiler version used
    pub rust_version: String,
    /// Path to Rust sysroot
    pub sysroot: String,
    /// Map of library name to library info
    pub libraries: BTreeMap<String, LibraryInfo>,
    /// Aggregate counts
    pub summary: Summary,
}

impl StdlibInventory {
    /// Load inventory from a JSON file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read inventory file: {}", path.display()))?;
        let inventory: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse inventory JSON: {}", path.display()))?;
        Ok(inventory)
    }
    
    /// Load inventory from JSON string
    pub fn from_str(json: &str) -> Result<Self> {
        let inventory: Self = serde_json::from_str(json)
            .context("Failed to parse inventory JSON")?;
        Ok(inventory)
    }
    
    /// Get a library by name
    pub fn get_library(&self, name: &str) -> Option<&LibraryInfo> {
        self.libraries.get(name)
    }
    
    /// Get all trait names across all libraries
    pub fn all_trait_names(&self) -> Vec<&str> {
        self.libraries.values()
            .flat_map(|lib| lib.traits.iter().map(|t| t.name.as_str()))
            .collect()
    }
    
    /// Get all type names across all libraries
    pub fn all_type_names(&self) -> Vec<&str> {
        self.libraries.values()
            .flat_map(|lib| lib.types.iter().map(|t| t.name.as_str()))
            .collect()
    }
}

/// Information about a single library (core, alloc, or std)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryInfo {
    /// Absolute path to library source
    pub path: String,
    /// Source files
    pub files: Vec<FileInfo>,
    /// Module tree
    pub modules: Vec<ModuleInfo>,
    /// Prelude info (if this library has one)
    pub prelude: Option<PreludeInfo>,
    /// Types (structs, enums)
    pub types: Vec<TypeInfo>,
    /// Traits
    pub traits: Vec<TraitInfo>,
    /// Free functions
    pub functions: Vec<FunctionInfo>,
    /// Macros
    pub macros: Vec<MacroInfo>,
    /// Constants
    pub constants: Vec<ConstantInfo>,
    /// Type aliases
    pub type_aliases: Vec<TypeAliasInfo>,
    /// Impl blocks
    pub impls: Vec<ImplInfo>,
}

/// A module in the module tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    /// Module name
    pub name: String,
    /// Full module path
    pub path: String,
    /// pub mod vs mod
    pub is_public: bool,
    /// File that defines this module
    pub source_file: String,
    /// Names of child modules
    pub child_modules: Vec<String>,
    /// pub use statements
    pub re_exports: Vec<ReExportInfo>,
    /// Items defined in this module
    pub items: ModuleItems,
}

/// A pub use re-export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReExportInfo {
    /// Public name (or * for glob)
    pub name: String,
    /// Where it comes from
    pub source_path: String,
    /// Kind: type, trait, type/trait, function, macro, module, all
    pub kind: String,
}

/// Items defined directly in a module
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModuleItems {
    pub types: Vec<String>,
    pub traits: Vec<String>,
    pub functions: Vec<String>,
    pub macros: Vec<String>,
    pub constants: Vec<String>,
    pub type_aliases: Vec<String>,
}

/// Prelude information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreludeInfo {
    /// Full module path
    pub module_path: String,
    /// Source file
    pub source_file: String,
    /// Types auto-imported
    pub types: Vec<PreludeItem>,
    /// Traits auto-imported
    pub traits: Vec<PreludeItem>,
    /// Macros auto-imported
    pub macros: Vec<PreludeItem>,
    /// Functions auto-imported
    pub functions: Vec<PreludeItem>,
}

/// An item in the prelude
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreludeItem {
    /// Public name
    pub name: String,
    /// Where it comes from
    pub source_path: String,
}

/// Information about a source file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// Relative path
    pub path: String,
    /// Module path
    pub module: String,
    /// Line count
    pub line_count: usize,
    /// Type count
    pub type_count: usize,
    /// Trait count
    pub trait_count: usize,
    /// Function count
    pub function_count: usize,
    /// Impl count
    pub impl_count: usize,
}

/// A struct or enum definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeInfo {
    /// Type name
    pub name: String,
    /// Fully qualified path
    pub qualified_path: String,
    /// "struct" or "enum"
    pub kind: String,
    /// Has type parameters
    pub is_generic: bool,
    /// Requires unsafe
    pub is_unsafe: bool,
    /// Derived traits
    pub derives: Vec<String>,
    /// Inherent methods
    pub methods: Vec<MethodInfo>,
    /// Source file
    pub source_file: String,
    /// Source line
    pub source_line: u32,
}

/// An inherent method on a type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodInfo {
    pub name: String,
    pub is_generic: bool,
    pub is_unsafe: bool,
    pub can_panic: bool,
    pub must_use: bool,
    pub is_const: bool,
    /// "self", "&self", "&mut self", or "none"
    pub takes_self: String,
}

/// A trait definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitInfo {
    pub name: String,
    pub qualified_path: String,
    pub is_unsafe: bool,
    pub is_auto: bool,
    pub supertraits: Vec<String>,
    pub associated_types: Vec<String>,
    pub associated_consts: Vec<String>,
    pub methods: Vec<TraitMethodInfo>,
    pub source_file: String,
    pub source_line: u32,
}

/// A method defined in a trait
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitMethodInfo {
    pub name: String,
    pub is_generic: bool,
    pub is_unsafe: bool,
    pub has_default: bool,
    pub can_panic: bool,
    pub must_use: bool,
}

/// A free (top-level) function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    pub name: String,
    pub qualified_path: String,
    pub is_generic: bool,
    pub is_unsafe: bool,
    pub can_panic: bool,
    pub must_use: bool,
    pub is_const: bool,
    pub source_file: String,
    pub source_line: u32,
}

/// A macro definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroInfo {
    pub name: String,
    pub qualified_path: String,
    /// "declarative" or "procedural"
    pub kind: String,
    pub is_exported: bool,
    pub source_file: String,
    pub source_line: u32,
}

/// A constant or static
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstantInfo {
    pub name: String,
    pub qualified_path: String,
    pub const_type: String,
    pub value: Option<String>,
    pub source_file: String,
    pub source_line: u32,
}

/// A type alias
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeAliasInfo {
    pub name: String,
    pub qualified_path: String,
    pub target: String,
    pub source_file: String,
    pub source_line: u32,
}

/// An impl block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplInfo {
    /// The type being implemented
    pub impl_type: String,
    /// Trait being implemented, or None for inherent impl
    pub trait_name: Option<String>,
    pub is_unsafe: bool,
    /// Blanket impl (generic applying to multiple types)
    pub is_blanket: bool,
    /// Forwarding impl (propagates traits through wrappers)
    pub is_forwarding: bool,
    /// Bridge impl (trait A gives you trait B)
    pub is_bridge: bool,
    pub where_clause: Option<String>,
    /// Method names in this impl
    pub methods: Vec<String>,
    pub source_file: String,
    pub source_line: u32,
}

/// Aggregate counts across all libraries
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Summary {
    pub total_libraries: usize,
    pub total_files: usize,
    pub total_modules: usize,
    pub total_public_modules: usize,
    pub total_re_exports: usize,
    pub total_prelude_items: usize,
    pub total_types: usize,
    pub total_traits: usize,
    pub total_type_methods: usize,
    pub total_trait_methods: usize,
    pub total_functions: usize,
    pub total_macros: usize,
    pub total_constants: usize,
    pub total_type_aliases: usize,
    pub total_impls: usize,
    pub total_blanket_impls: usize,
    pub total_forwarding_impls: usize,
    pub total_bridge_impls: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_minimal_inventory() {
        let json = r#"{
            "generated": "2025-12-14",
            "rust_version": "rustc 1.88.0",
            "sysroot": "/usr",
            "libraries": {},
            "summary": {
                "total_libraries": 0,
                "total_files": 0,
                "total_modules": 0,
                "total_public_modules": 0,
                "total_re_exports": 0,
                "total_prelude_items": 0,
                "total_types": 0,
                "total_traits": 0,
                "total_type_methods": 0,
                "total_trait_methods": 0,
                "total_functions": 0,
                "total_macros": 0,
                "total_constants": 0,
                "total_type_aliases": 0,
                "total_impls": 0,
                "total_blanket_impls": 0,
                "total_forwarding_impls": 0,
                "total_bridge_impls": 0
            }
        }"#;
        
        let inventory = StdlibInventory::from_str(json).unwrap();
        assert_eq!(inventory.rust_version, "rustc 1.88.0");
        assert!(inventory.libraries.is_empty());
    }
}

