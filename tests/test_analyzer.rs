// Copyright (C) Brian G. Milnes 2025

//! Tests for analyzer module

use rusticate::{parse_file, analyze};

#[test]
fn test_analyze_empty_file() {
    let source = "";
    
    let syntax = parse_file(source).unwrap();
    let issues = analyze(&syntax).unwrap();
    
    assert!(issues.is_empty());
}

#[test]
fn test_analyze_simple_function() {
    let source = r#"
        fn test_function() {
            println!("test");
        }
    "#;
    
    let syntax = parse_file(source).unwrap();
    let issues = analyze(&syntax).unwrap();
    
    // For now, analyzer returns empty - to be implemented
    assert!(issues.is_empty());
}

#[test]
fn test_analyze_pub_mod_structure() {
    let source = r#"
        pub mod example {
            pub fn hello() -> String {
                String::from("Hello")
            }
        }
    "#;
    
    let syntax = parse_file(source).unwrap();
    let issues = analyze(&syntax).unwrap();
    
    // Should not error on valid structure
    assert!(issues.is_empty());
}

