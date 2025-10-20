// Copyright (C) Brian G. Milnes 2025

//! Tests for parser module

use rusticate::parse_file;

#[test]
fn test_parse_simple_function() {
    let source = r#"
        fn main() {
            println!("Hello, world!");
        }
    "#;
    
    let result = parse_file(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_struct() {
    let source = r#"
        struct Point {
            x: i32,
            y: i32,
        }
    "#;
    
    let result = parse_file(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_with_pub_mod() {
    let source = r#"
        pub mod example {
            pub fn hello() -> String {
                String::from("Hello")
            }
        }
    "#;
    
    let result = parse_file(source);
    assert!(result.is_ok());
}

#[test]
fn test_parse_invalid_syntax() {
    let source = r#"
        fn broken {{{
    "#;
    
    let result = parse_file(source);
    assert!(result.is_err());
}

