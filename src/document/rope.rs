//! Rope data structure for efficient text storage and manipulation
//!
//! Provides O(log n) insert and delete operations.

use std::fmt;

/// Maximum size of a leaf node in bytes
const MAX_LEAF_SIZE: usize = 1024;

/// Rope data structure for efficient text editing
#[derive(Clone)]
pub struct Rope {
    root: RopeNode,
}

#[derive(Clone)]
enum RopeNode {
    /// Internal node with two children
    Branch {
        left: Box<RopeNode>,
        right: Box<RopeNode>,
        /// Total characters in left subtree
        left_weight: usize,
        /// Total lines in left subtree
        left_lines: usize,
    },
    /// Leaf node containing actual text
    Leaf {
        text: String,
        /// Cached line break count
        line_count: usize,
    },
    /// Empty node
    Empty,
}

impl Default for Rope {
    fn default() -> Self {
        Self::new()
    }
}

impl Rope {
    /// Create a new empty rope
    pub fn new() -> Self {
        Self {
            root: RopeNode::Empty,
        }
    }

    /// Create a rope from a string
    pub fn from_str(s: &str) -> Self {
        if s.is_empty() {
            return Self::new();
        }

        // Build balanced tree from chunks
        let chunks: Vec<_> = s
            .as_bytes()
            .chunks(MAX_LEAF_SIZE)
            .map(|chunk| {
                let text = String::from_utf8_lossy(chunk).into_owned();
                let line_count = text.chars().filter(|c| *c == '\n').count();
                RopeNode::Leaf { text, line_count }
            })
            .collect();

        Self {
            root: Self::build_tree(chunks),
        }
    }

    /// Build a balanced tree from leaf nodes
    fn build_tree(mut nodes: Vec<RopeNode>) -> RopeNode {
        if nodes.is_empty() {
            return RopeNode::Empty;
        }
        if nodes.len() == 1 {
            return nodes.remove(0);
        }

        while nodes.len() > 1 {
            let mut new_nodes = Vec::with_capacity((nodes.len() + 1) / 2);

            for pair in nodes.chunks(2) {
                if pair.len() == 2 {
                    let left = pair[0].clone();
                    let right = pair[1].clone();
                    let left_weight = left.len();
                    let left_lines = left.line_count();

                    new_nodes.push(RopeNode::Branch {
                        left: Box::new(left),
                        right: Box::new(right),
                        left_weight,
                        left_lines,
                    });
                } else {
                    new_nodes.push(pair[0].clone());
                }
            }

            nodes = new_nodes;
        }

        nodes.remove(0)
    }

    /// Get total length in bytes
    pub fn len(&self) -> usize {
        self.root.len()
    }

    /// Check if rope is empty
    pub fn is_empty(&self) -> bool {
        self.root.len() == 0
    }

    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.root.line_count()
    }

    /// Insert text at the given byte offset
    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let line_count = text.chars().filter(|c| *c == '\n').count();
        let new_leaf = RopeNode::Leaf {
            text: text.to_string(),
            line_count,
        };

        self.root = Self::insert_node(std::mem::take(&mut self.root), offset, new_leaf);
        self.rebalance_if_needed();
    }

    /// Insert a node at the given offset
    fn insert_node(node: RopeNode, offset: usize, new_node: RopeNode) -> RopeNode {
        match node {
            RopeNode::Empty => new_node,
            RopeNode::Leaf { text, line_count } => {
                if offset == 0 {
                    // Insert before
                    let new_weight = new_node.len();
                    let new_lines = new_node.line_count();
                    RopeNode::Branch {
                        left: Box::new(new_node),
                        right: Box::new(RopeNode::Leaf { text, line_count }),
                        left_weight: new_weight,
                        left_lines: new_lines,
                    }
                } else if offset >= text.len() {
                    // Insert after
                    RopeNode::Branch {
                        left: Box::new(RopeNode::Leaf { text, line_count }),
                        right: Box::new(new_node),
                        left_weight: offset,
                        left_lines: line_count,
                    }
                } else {
                    // Split leaf
                    let (left_text, right_text) = text.split_at(offset);
                    let left_lines = left_text.chars().filter(|c| *c == '\n').count();
                    let right_lines = right_text.chars().filter(|c| *c == '\n').count();

                    let left_leaf = RopeNode::Leaf {
                        text: left_text.to_string(),
                        line_count: left_lines,
                    };
                    let right_leaf = RopeNode::Leaf {
                        text: right_text.to_string(),
                        line_count: right_lines,
                    };

                    // Insert between the split parts
                    let new_weight = left_text.len();
                    let left_combined = RopeNode::Branch {
                        left: Box::new(left_leaf),
                        right: Box::new(new_node),
                        left_weight: new_weight,
                        left_lines,
                    };

                    let combined_weight = left_combined.len();
                    let combined_lines = left_combined.line_count();

                    RopeNode::Branch {
                        left: Box::new(left_combined),
                        right: Box::new(right_leaf),
                        left_weight: combined_weight,
                        left_lines: combined_lines,
                    }
                }
            }
            RopeNode::Branch {
                left,
                right,
                left_weight,
                left_lines,
            } => {
                if offset <= left_weight {
                    let new_left = Self::insert_node(*left, offset, new_node);
                    let new_left_weight = new_left.len();
                    let new_left_lines = new_left.line_count();
                    RopeNode::Branch {
                        left: Box::new(new_left),
                        right,
                        left_weight: new_left_weight,
                        left_lines: new_left_lines,
                    }
                } else {
                    let new_right = Self::insert_node(*right, offset - left_weight, new_node);
                    RopeNode::Branch {
                        left,
                        right: Box::new(new_right),
                        left_weight,
                        left_lines,
                    }
                }
            }
        }
    }

    /// Delete text in the given byte range
    pub fn delete(&mut self, start: usize, end: usize) {
        if start >= end || start >= self.len() {
            return;
        }

        let end = end.min(self.len());
        self.root = Self::delete_range(std::mem::take(&mut self.root), start, end);
        self.rebalance_if_needed();
    }

    /// Delete a range from a node
    fn delete_range(node: RopeNode, start: usize, end: usize) -> RopeNode {
        match node {
            RopeNode::Empty => RopeNode::Empty,
            RopeNode::Leaf { text, .. } => {
                let new_text = if start == 0 && end >= text.len() {
                    String::new()
                } else if start == 0 {
                    text[end..].to_string()
                } else if end >= text.len() {
                    text[..start].to_string()
                } else {
                    format!("{}{}", &text[..start], &text[end..])
                };

                if new_text.is_empty() {
                    RopeNode::Empty
                } else {
                    let line_count = new_text.chars().filter(|c| *c == '\n').count();
                    RopeNode::Leaf {
                        text: new_text,
                        line_count,
                    }
                }
            }
            RopeNode::Branch {
                left,
                right,
                left_weight,
                left_lines,
            } => {
                let left_end = left_weight;

                if end <= left_end {
                    // Delete entirely in left subtree
                    let new_left = Self::delete_range(*left, start, end);
                    Self::merge_nodes(new_left, *right)
                } else if start >= left_end {
                    // Delete entirely in right subtree
                    let new_right = Self::delete_range(*right, start - left_end, end - left_end);
                    Self::merge_nodes(*left, new_right)
                } else {
                    // Delete spans both subtrees
                    let new_left = Self::delete_range(*left, start, left_end);
                    let new_right = Self::delete_range(*right, 0, end - left_end);
                    Self::merge_nodes(new_left, new_right)
                }
            }
        }
    }

    /// Merge two nodes into one
    fn merge_nodes(left: RopeNode, right: RopeNode) -> RopeNode {
        match (&left, &right) {
            (RopeNode::Empty, _) => right,
            (_, RopeNode::Empty) => left,
            _ => {
                let left_weight = left.len();
                let left_lines = left.line_count();
                RopeNode::Branch {
                    left: Box::new(left),
                    right: Box::new(right),
                    left_weight,
                    left_lines,
                }
            }
        }
    }

    /// Get a slice of text
    pub fn slice(&self, start: usize, end: usize) -> String {
        let mut result = String::with_capacity(end - start);
        self.root.collect_range(start, end, &mut result);
        result
    }

    /// Rebalance tree if needed
    fn rebalance_if_needed(&mut self) {
        let height = self.root.height();
        let optimal_height = (self.len() as f64 / MAX_LEAF_SIZE as f64).log2().ceil() as usize + 1;

        // Rebalance if tree is too unbalanced (2x optimal height is a reasonable heuristic)
        if height > optimal_height * 2 {
            // Collect all leaves
            let mut leaves = Vec::new();
            // Take the root to consume it
            let root = std::mem::take(&mut self.root);
            root.collect_leaves(&mut leaves);
            
            // Rebuild balanced tree
            self.root = Self::build_tree(leaves);
        }
    }
}

impl Default for RopeNode {
    fn default() -> Self {
        RopeNode::Empty
    }
}

impl RopeNode {
    fn len(&self) -> usize {
        match self {
            RopeNode::Empty => 0,
            RopeNode::Leaf { text, .. } => text.len(),
            RopeNode::Branch {
                left_weight, right, ..
            } => left_weight + right.len(),
        }
    }

    fn line_count(&self) -> usize {
        match self {
            RopeNode::Empty => 0,
            RopeNode::Leaf { line_count, .. } => *line_count,
            RopeNode::Branch {
                left_lines, right, ..
            } => left_lines + right.line_count(),
        }
    }

    fn height(&self) -> usize {
        match self {
            RopeNode::Empty | RopeNode::Leaf { .. } => 1,
            RopeNode::Branch { left, right, .. } => 1 + left.height().max(right.height()),
        }
    }

    fn collect_leaves(self, leaves: &mut Vec<RopeNode>) {
        match self {
            RopeNode::Empty => {}
            RopeNode::Leaf { .. } => leaves.push(self),
            RopeNode::Branch { left, right, .. } => {
                left.collect_leaves(leaves);
                right.collect_leaves(leaves);
            }
        }
    }

    fn collect_range(&self, start: usize, end: usize, result: &mut String) {
        if start >= end {
            return;
        }

        match self {
            RopeNode::Empty => {}
            RopeNode::Leaf { text, .. } => {
                let s = start.min(text.len());
                let e = end.min(text.len());
                if s < e {
                    result.push_str(&text[s..e]);
                }
            }
            RopeNode::Branch {
                left,
                right,
                left_weight,
                ..
            } => {
                if start < *left_weight {
                    left.collect_range(start, end.min(*left_weight), result);
                }
                if end > *left_weight {
                    right.collect_range(
                        start.saturating_sub(*left_weight),
                        end - *left_weight,
                        result,
                    );
                }
            }
        }
    }

    fn collect_all(&self, result: &mut String) {
        match self {
            RopeNode::Empty => {}
            RopeNode::Leaf { text, .. } => result.push_str(text),
            RopeNode::Branch { left, right, .. } => {
                left.collect_all(result);
                right.collect_all(result);
            }
        }
    }
}

impl fmt::Display for Rope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::with_capacity(self.len());
        self.root.collect_all(&mut result);
        write!(f, "{}", result)
    }
}

impl fmt::Debug for Rope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Rope({:?})", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_rope() {
        let rope = Rope::new();
        assert_eq!(rope.len(), 0);
        assert!(rope.is_empty());
    }

    #[test]
    fn test_from_str() {
        let rope = Rope::from_str("Hello, World!");
        assert_eq!(rope.len(), 13);
        assert_eq!(rope.to_string(), "Hello, World!");
    }

    #[test]
    fn test_insert() {
        let mut rope = Rope::from_str("Hello World");
        rope.insert(5, ",");
        assert_eq!(rope.to_string(), "Hello, World");
    }

    #[test]
    fn test_delete() {
        let mut rope = Rope::from_str("Hello, World");
        rope.delete(5, 6); // Delete just the comma
        assert_eq!(rope.to_string(), "Hello World");
    }

    #[test]
    fn test_slice() {
        let rope = Rope::from_str("Hello, World!");
        assert_eq!(rope.slice(0, 5), "Hello");
        assert_eq!(rope.slice(7, 12), "World");
    }

    #[test]
    fn test_line_count() {
        let rope = Rope::from_str("Line 1\nLine 2\nLine 3");
        assert_eq!(rope.line_count(), 2);
    }

    #[test]
    fn test_large_insert() {
        let mut rope = Rope::new();
        let large_text = "x".repeat(10000);
        rope.insert(0, &large_text);
        assert_eq!(rope.len(), 10000);
    }
}
