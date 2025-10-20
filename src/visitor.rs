// Copyright (C) Brian G. Milnes 2025

//! Visitor pattern implementation for traversing Rust AST

pub mod visitor {
    use ra_ap_syntax::{SyntaxNode, WalkEvent};

    /// A visitor that collects issues while traversing the AST
    pub struct IssueVisitor {
        pub issues: Vec<String>,
    }

    impl IssueVisitor {
        pub fn new() -> Self {
            IssueVisitor {
                issues: Vec::new(),
            }
        }
        
        /// Visit a syntax node and its children
        pub fn visit(&mut self, node: &SyntaxNode) {
            for event in node.preorder_with_tokens() {
                match event {
                    WalkEvent::Enter(_element) => {
                        // Custom analysis logic here
                    }
                    WalkEvent::Leave(_) => {}
                }
            }
        }
    }

    impl Default for IssueVisitor {
        fn default() -> Self {
            Self::new()
        }
    }

}

