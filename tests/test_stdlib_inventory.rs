// Copyright (C) Brian G. Milnes 2025

//! Tests for stdlib inventory parsing.

use rusticate::stdlib_inventory::StdlibInventory;
use std::path::PathBuf;

/// Get path to the generated inventory file
fn inventory_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("analyses/rust_stdlib_inventory.json")
}

#[test]
fn test_parse_generated_inventory() {
    let path = inventory_path();
    if !path.exists() {
        eprintln!("Skipping test: inventory file not found at {}", path.display());
        eprintln!("Run `cargo run --release --bin rusticate-analyze-libs` first.");
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path)
        .expect("Failed to parse inventory");
    
    // Basic sanity checks
    assert_eq!(inventory.summary.total_libraries, 3, "Should have 3 libraries: core, alloc, std");
    assert!(inventory.libraries.contains_key("core"), "Should have core library");
    assert!(inventory.libraries.contains_key("alloc"), "Should have alloc library");
    assert!(inventory.libraries.contains_key("std"), "Should have std library");
    
    // Check we have reasonable counts
    assert!(inventory.summary.total_types > 1000, "Should have >1000 types");
    assert!(inventory.summary.total_traits > 300, "Should have >300 traits");
    assert!(inventory.summary.total_functions > 1000, "Should have >1000 functions");
    assert!(inventory.summary.total_impls > 5000, "Should have >5000 impls");
}

#[test]
fn test_inventory_core_library() {
    let path = inventory_path();
    if !path.exists() {
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path).unwrap();
    let core = inventory.get_library("core").expect("core library not found");
    
    // Core should have Option and Result
    let has_option = core.types.iter().any(|t| t.name == "Option");
    let has_result = core.types.iter().any(|t| t.name == "Result");
    assert!(has_option, "core should have Option type");
    assert!(has_result, "core should have Result type");
    
    // Core should have Iterator trait
    let has_iterator = core.traits.iter().any(|t| t.name == "Iterator");
    assert!(has_iterator, "core should have Iterator trait");
    
    // Core should have Clone, Debug, etc
    let has_clone = core.traits.iter().any(|t| t.name == "Clone");
    let has_debug = core.traits.iter().any(|t| t.name == "Debug");
    assert!(has_clone, "core should have Clone trait");
    assert!(has_debug, "core should have Debug trait");
}

#[test]
fn test_inventory_std_library() {
    let path = inventory_path();
    if !path.exists() {
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path).unwrap();
    let std = inventory.get_library("std").expect("std library not found");
    
    // std should have HashMap, Mutex, etc.
    let has_hashmap = std.types.iter().any(|t| t.name == "HashMap");
    let has_mutex = std.types.iter().any(|t| t.name == "Mutex");
    assert!(has_hashmap, "std should have HashMap type");
    assert!(has_mutex, "std should have Mutex type");
    
    // std should have a prelude
    assert!(std.prelude.is_some(), "std should have a prelude");
    
    let prelude = std.prelude.as_ref().unwrap();
    assert!(!prelude.types.is_empty(), "std prelude should have types");
    assert!(!prelude.traits.is_empty(), "std prelude should have traits");
}

#[test]
fn test_inventory_alloc_library() {
    let path = inventory_path();
    if !path.exists() {
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path).unwrap();
    let alloc = inventory.get_library("alloc").expect("alloc library not found");
    
    // alloc should have Vec, Box, String
    let has_vec = alloc.types.iter().any(|t| t.name == "Vec");
    let has_box = alloc.types.iter().any(|t| t.name == "Box");
    let has_string = alloc.types.iter().any(|t| t.name == "String");
    assert!(has_vec, "alloc should have Vec type");
    assert!(has_box, "alloc should have Box type");
    assert!(has_string, "alloc should have String type");
}

#[test]
fn test_blanket_impl_counts() {
    let path = inventory_path();
    if !path.exists() {
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path).unwrap();
    
    // Check blanket impl categorization
    assert!(inventory.summary.total_blanket_impls > 0, "Should have blanket impls");
    assert!(inventory.summary.total_forwarding_impls > 0, "Should have forwarding impls");
    assert!(inventory.summary.total_bridge_impls > 0, "Should have bridge impls");
    
    // Forwarding + bridge should be <= blanket (some blanket impls may be neither)
    let categorized = inventory.summary.total_forwarding_impls + inventory.summary.total_bridge_impls;
    assert!(categorized <= inventory.summary.total_blanket_impls,
            "Forwarding + bridge should be <= blanket impls");
}

#[test]
fn test_all_trait_names() {
    let path = inventory_path();
    if !path.exists() {
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path).unwrap();
    let trait_names = inventory.all_trait_names();
    
    // Should have common traits
    assert!(trait_names.contains(&"Clone"), "Should have Clone trait");
    assert!(trait_names.contains(&"Debug"), "Should have Debug trait");
    assert!(trait_names.contains(&"Iterator"), "Should have Iterator trait");
    assert!(trait_names.contains(&"Display"), "Should have Display trait");
}

#[test]
fn test_all_type_names() {
    let path = inventory_path();
    if !path.exists() {
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path).unwrap();
    let type_names = inventory.all_type_names();
    
    // Should have common types
    assert!(type_names.contains(&"Option"), "Should have Option type");
    assert!(type_names.contains(&"Result"), "Should have Result type");
    assert!(type_names.contains(&"Vec"), "Should have Vec type");
    assert!(type_names.contains(&"String"), "Should have String type");
}

#[test]
fn test_module_tree() {
    let path = inventory_path();
    if !path.exists() {
        return;
    }
    
    let inventory = StdlibInventory::from_file(&path).unwrap();
    let core = inventory.get_library("core").unwrap();
    
    // Should have modules
    assert!(!core.modules.is_empty(), "core should have modules");
    
    // Should have re-exports in some modules
    let total_reexports: usize = core.modules.iter()
        .map(|m| m.re_exports.len())
        .sum();
    assert!(total_reexports > 0, "core should have re-exports");
}

