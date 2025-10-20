// Copyright (C) Brian G. Milnes 2025

//! Tests for fixer module

use rusticate::{parse_file, fix};

#[test]
fn test_fix_preserves_code() {
    let source = r#"fn main() {
    println!("Hello, world!");
}"#;
    
    let syntax = parse_file(source).unwrap();
    let fixed = fix(&syntax).unwrap();
    
    // The fixed code should still be parseable
    assert!(parse_file(&fixed).is_ok());
}

#[test]
fn test_fix_pub_mod_code() {
    let source = r#"pub mod example {
    pub fn hello() -> String {
        String::from("Hello")
    }
}"#;
    
    let syntax = parse_file(source).unwrap();
    let fixed = fix(&syntax).unwrap();
    
    // The fixed code should still be parseable
    assert!(parse_file(&fixed).is_ok());
}

