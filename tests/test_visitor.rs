// Copyright (C) Brian G. Milnes 2025

//! Tests for visitor module

use rusticate::{parse_file, IssueVisitor};
use ra_ap_syntax::ast::AstNode;

#[test]
fn test_visitor_traversal() {
    let source = r#"
        fn main() {
            println!("Hello, world!");
        }
    "#;
    
    let syntax = parse_file(source).unwrap();
    let mut visitor = IssueVisitor::new();
    visitor.visit(syntax.syntax());
    
    // The visitor should have traversed the file without errors
    assert!(visitor.issues.is_empty());
}

#[test]
fn test_visitor_on_pub_mod() {
    let source = r#"
        pub mod example {
            pub fn hello() -> String {
                String::from("Hello")
            }
        }
    "#;
    
    let syntax = parse_file(source).unwrap();
    let mut visitor = IssueVisitor::new();
    visitor.visit(syntax.syntax());
    
    assert!(visitor.issues.is_empty());
}

#[test]
fn test_visitor_on_complex_structure() {
    let source = r#"
        pub mod example {
            pub struct Point {
                x: i32,
                y: i32,
            }
            
            impl Point {
                pub fn new(x: i32, y: i32) -> Self {
                    Point { x, y }
                }
            }
        }
    "#;
    
    let syntax = parse_file(source).unwrap();
    let mut visitor = IssueVisitor::new();
    visitor.visit(syntax.syntax());
    
    assert!(visitor.issues.is_empty());
}

