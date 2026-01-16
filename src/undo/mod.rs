//! Undo/Redo system with transaction support

use crate::document::Document;
use crate::editing::{Cursor, EditOp, Selection};

/// Result of an undo/redo operation
#[derive(Debug, Clone)]
pub struct UndoResult {
    pub cursor: Cursor,
    pub selection: Option<Selection>,
}

/// A single transaction that can be undone/redone
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Description of the operation
    pub description: String,
    /// Forward operations
    pub forward_ops: Vec<EditOp>,
    /// Reverse operations (for undo)
    pub reverse_ops: Vec<EditOp>,
    /// Cursor state before the transaction
    pub cursor_before: Cursor,
    /// Selection state before the transaction
    pub selection_before: Option<Selection>,
    /// Timestamp for grouping (milliseconds)
    pub timestamp: u64,
}

impl Transaction {
    /// Create a new transaction
    pub fn new(
        description: impl Into<String>,
        cursor_before: &Cursor,
        selection_before: Option<&Selection>,
    ) -> Self {
        Self {
            description: description.into(),
            forward_ops: Vec::new(),
            reverse_ops: Vec::new(),
            cursor_before: cursor_before.clone(),
            selection_before: selection_before.cloned(),
            timestamp: current_timestamp(),
        }
    }

    /// Check if this transaction is empty
    pub fn is_empty(&self) -> bool {
        self.forward_ops.is_empty()
    }
}

/// Undo/Redo manager
pub struct UndoManager {
    /// Stack of undoable transactions
    undo_stack: Vec<Transaction>,
    /// Stack of redoable transactions
    redo_stack: Vec<Transaction>,
    /// Maximum history depth
    max_depth: usize,
    /// Current transaction being built
    pending: Option<Transaction>,
    /// Time window for merging transactions (ms)
    merge_window_ms: u64,
}

impl UndoManager {
    /// Create a new undo manager
    pub fn new(max_depth: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth,
            pending: None,
            merge_window_ms: 500,
        }
    }

    /// Begin a new transaction
    pub fn begin_transaction(
        &mut self,
        description: &str,
        cursor: &Cursor,
        selection: Option<&Selection>,
    ) {
        self.pending = Some(Transaction::new(description, cursor, selection));
    }

    /// Record an edit within the current transaction
    pub fn record_edit(&mut self, forward: EditOp, reverse: EditOp) {
        if let Some(ref mut txn) = self.pending {
            txn.forward_ops.push(forward);
            txn.reverse_ops.push(reverse);
        }
    }

    /// Commit the current transaction
    pub fn commit(&mut self) {
        if let Some(txn) = self.pending.take() {
            if txn.is_empty() {
                return;
            }

            // Clear redo stack on new edit
            self.redo_stack.clear();

            // Try to merge with previous transaction
            if self.should_merge(&txn) {
                self.merge_with_last(txn);
            } else {
                self.undo_stack.push(txn);
            }

            // Enforce depth limit
            while self.undo_stack.len() > self.max_depth {
                self.undo_stack.remove(0);
            }
        }
    }

    /// Check if transaction should merge with previous
    /// Note: Currently disabled because merging with absolute offsets is complex
    /// and requires offset adjustment. Each transaction is kept separate for correctness.
    fn should_merge(&self, _txn: &Transaction) -> bool {
        // Disabled: merging transactions with absolute offsets requires
        // careful offset adjustment to maintain correctness
        false
    }

    /// Check if operations are compatible for merging
    fn ops_compatible(ops1: &[EditOp], ops2: &[EditOp]) -> bool {
        if ops1.len() != 1 || ops2.len() != 1 {
            return false;
        }

        match (&ops1[0], &ops2[0]) {
            (EditOp::Insert { .. }, EditOp::Insert { .. }) => true,
            (EditOp::Delete { .. }, EditOp::Delete { .. }) => true,
            _ => false,
        }
    }

    /// Merge transaction with last one
    fn merge_with_last(&mut self, txn: Transaction) {
        if let Some(last) = self.undo_stack.last_mut() {
            last.forward_ops.extend(txn.forward_ops);
            // Append reverse ops (they will be applied in reverse order during undo)
            last.reverse_ops.extend(txn.reverse_ops);
            last.timestamp = txn.timestamp;
        }
    }

    /// Undo the last transaction
    pub fn undo(&mut self, document: &mut Document) -> Option<UndoResult> {
        let txn = self.undo_stack.pop()?;

        // Apply reverse operations
        for op in txn.reverse_ops.iter().rev() {
            document.apply_edit(op.clone());
        }

        let result = UndoResult {
            cursor: txn.cursor_before.clone(),
            selection: txn.selection_before.clone(),
        };

        // Move to redo stack
        self.redo_stack.push(txn);

        Some(result)
    }

    /// Redo the last undone transaction
    pub fn redo(&mut self, document: &mut Document) -> Option<UndoResult> {
        let txn = self.redo_stack.pop()?;

        // Apply forward operations
        let mut final_cursor = txn.cursor_before.clone();
        for op in &txn.forward_ops {
            let result = document.apply_edit(op.clone());
            final_cursor = Cursor::new(result.new_cursor);
        }

        let result = UndoResult {
            cursor: final_cursor,
            selection: None,
        };

        // Move to undo stack
        self.undo_stack.push(txn);

        Some(result)
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get undo stack depth
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get redo stack depth
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.pending = None;
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::{AbsoluteOffset, DocPosition};

    fn test_cursor() -> Cursor {
        Cursor::new(DocPosition::default())
    }

    #[test]
    fn test_undo_manager_creation() {
        let manager = UndoManager::new(100);
        assert!(!manager.can_undo());
        assert!(!manager.can_redo());
    }

    #[test]
    fn test_transaction() {
        let cursor = test_cursor();
        let mut manager = UndoManager::new(100);

        manager.begin_transaction("test", &cursor, None);
        manager.record_edit(
            EditOp::Insert {
                position: AbsoluteOffset(0),
                text: "Hello".to_string(),
            },
            EditOp::Delete {
                start: AbsoluteOffset(0),
                end: AbsoluteOffset(5),
            },
        );
        manager.commit();

        assert!(manager.can_undo());
        assert!(!manager.can_redo());
        assert_eq!(manager.undo_depth(), 1);
    }

    #[test]
    fn test_undo_redo() {
        let cursor = test_cursor();
        let mut manager = UndoManager::new(100);
        let mut doc = Document::new();

        // Insert text
        manager.begin_transaction("insert", &cursor, None);
        let insert_op = EditOp::Insert {
            position: AbsoluteOffset(0),
            text: "Hello".to_string(),
        };
        let reverse = doc.compute_reverse(&insert_op);
        doc.apply_edit(insert_op.clone());
        manager.record_edit(insert_op, reverse);
        manager.commit();

        assert_eq!(doc.text(), "Hello");

        // Undo
        manager.undo(&mut doc);
        assert_eq!(doc.text(), "");

        // Redo
        manager.redo(&mut doc);
        assert_eq!(doc.text(), "Hello");
    }

    #[test]
    fn test_max_depth() {
        let cursor = test_cursor();
        let mut manager = UndoManager::new(3);

        for i in 0..5 {
            manager.begin_transaction(&format!("op {}", i), &cursor, None);
            manager.record_edit(
                EditOp::Insert {
                    position: AbsoluteOffset(0),
                    text: "x".to_string(),
                },
                EditOp::Delete {
                    start: AbsoluteOffset(0),
                    end: AbsoluteOffset(1),
                },
            );
            // Add small delay to prevent merging
            std::thread::sleep(std::time::Duration::from_millis(600));
            manager.commit();
        }

        // Should be limited to max_depth
        assert_eq!(manager.undo_depth(), 3);
    }
}
