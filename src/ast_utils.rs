// Copyright (C) Brian G. Milnes 2025

//! AST traversal utilities for analyzing Rust code
//!
//! Provides common functions for walking and querying the syntax tree

pub mod ast_utils {
    use ra_ap_syntax::{SyntaxNode, SyntaxKind, SyntaxToken, SourceFile, Edition, WalkEvent};
    use anyhow::Result;
    
    /// Parse a Rust source file from a string
    pub fn parse_source(source: &str) -> Result<SourceFile> {
        let parsed = SourceFile::parse(source, Edition::Edition2021);
        
        if !parsed.errors().is_empty() {
            return Err(anyhow::anyhow!("Parse errors: {:?}", parsed.errors()));
        }
        
        Ok(parsed.tree())
    }
    
    /// Find all nodes of a specific kind in the syntax tree
    pub fn find_nodes(root: &SyntaxNode, kind: SyntaxKind) -> Vec<SyntaxNode> {
        let mut results = Vec::new();
        
        for event in root.preorder() {
            if let WalkEvent::Enter(node) = event {
                if node.kind() == kind {
                    results.push(node);
                }
            }
        }
        
        results
    }
    
    /// Find all nodes matching a predicate
    pub fn find_nodes_where<F>(root: &SyntaxNode, predicate: F) -> Vec<SyntaxNode>
    where
        F: Fn(&SyntaxNode) -> bool,
    {
        let mut results = Vec::new();
        
        for event in root.preorder() {
            if let WalkEvent::Enter(node) = event {
                if predicate(&node) {
                    results.push(node);
                }
            }
        }
        
        results
    }
    
    /// Get the text content of a node
    pub fn node_text(node: &SyntaxNode) -> String {
        node.text().to_string()
    }
    
    /// Find the first token of a specific kind within a node
    pub fn find_token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
        node.descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == kind)
    }
    
    /// Get all tokens of a specific kind within a node
    pub fn find_tokens(node: &SyntaxNode, kind: SyntaxKind) -> Vec<SyntaxToken> {
        node.descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == kind)
            .collect()
    }
    
    /// Check if a node is inside another node of a specific kind
    pub fn is_inside_node_kind(node: &SyntaxNode, kind: SyntaxKind) -> bool {
        let mut current = node.parent();
        while let Some(parent) = current {
            if parent.kind() == kind {
                return true;
            }
            current = parent.parent();
        }
        false
    }
    
    /// Get the line number of a node (1-indexed)
    pub fn line_number(node: &SyntaxNode, source: &str) -> usize {
        let offset = node.text_range().start().into();
        source[..offset].lines().count()
    }
    
    /// Get all child nodes of a specific kind
    pub fn children_of_kind(node: &SyntaxNode, kind: SyntaxKind) -> Vec<SyntaxNode> {
        node.children().filter(|child| child.kind() == kind).collect()
    }
    
    /// Check if a node has a parent of a specific kind
    pub fn has_parent_of_kind(node: &SyntaxNode, kind: SyntaxKind) -> bool {
        node.parent().is_some_and(|p| p.kind() == kind)
    }
}

