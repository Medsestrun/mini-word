//! Edit operations and results

use crate::document::ParagraphId;
use crate::editing::DocPosition;
use smallvec::SmallVec;

/// Absolute byte offset in the document
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct AbsoluteOffset(pub usize);

impl AbsoluteOffset {
    /// Create a new absolute offset
    pub fn new(offset: usize) -> Self {
        Self(offset)
    }
}

/// An atomic edit operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOp {
    /// Insert text at a position
    Insert {
        position: AbsoluteOffset,
        text: String,
    },
    /// Delete text in a range
    Delete {
        start: AbsoluteOffset,
        end: AbsoluteOffset,
    },
    /// A composite transaction of multiple operations
    Transaction {
        ops: Vec<EditOp>,
    },
}

impl EditOp {
    /// Create an insert operation
    pub fn insert(position: usize, text: impl Into<String>) -> Self {
        Self::Insert {
            position: AbsoluteOffset(position),
            text: text.into(),
        }
    }

    /// Create a delete operation
    pub fn delete(start: usize, end: usize) -> Self {
        Self::Delete {
            start: AbsoluteOffset(start),
            end: AbsoluteOffset(end),
        }
    }

    /// Create a transaction
    pub fn transaction(ops: Vec<EditOp>) -> Self {
        Self::Transaction { ops }
    }

    /// Get the affected range of this operation
    pub fn affected_range(&self) -> (usize, usize) {
        match self {
            EditOp::Insert { position, text } => (position.0, position.0 + text.len()),
            EditOp::Delete { start, end } => (start.0, end.0),
            EditOp::Transaction { ops } => {
                let mut min_start = usize::MAX;
                let mut max_end = 0;
                for op in ops {
                    let (s, e) = op.affected_range();
                    min_start = min_start.min(s);
                    max_end = max_end.max(e);
                }
                (min_start, max_end)
            }
        }
    }
}

/// Result of applying an edit operation
#[derive(Debug, Clone, Default)]
pub struct EditResult {
    /// New document version after this edit
    pub version: u64,
    /// Paragraphs that were modified
    pub affected_paragraphs: SmallVec<[ParagraphId; 4]>,
    /// Paragraphs that were created
    pub created_paragraphs: SmallVec<[ParagraphId; 2]>,
    /// Paragraphs that were deleted
    pub deleted_paragraphs: SmallVec<[ParagraphId; 2]>,
    /// New cursor position after the edit
    pub new_cursor: DocPosition,
}

impl EditResult {
    /// Check if any paragraphs were affected
    pub fn has_changes(&self) -> bool {
        !self.affected_paragraphs.is_empty()
            || !self.created_paragraphs.is_empty()
            || !self.deleted_paragraphs.is_empty()
    }

    /// Get all paragraphs that need relayout
    pub fn paragraphs_to_relayout(&self) -> impl Iterator<Item = &ParagraphId> {
        self.affected_paragraphs
            .iter()
            .chain(self.created_paragraphs.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_op_insert() {
        let op = EditOp::insert(10, "Hello");
        let (start, end) = op.affected_range();
        assert_eq!(start, 10);
        assert_eq!(end, 15);
    }

    #[test]
    fn test_edit_op_delete() {
        let op = EditOp::delete(5, 15);
        let (start, end) = op.affected_range();
        assert_eq!(start, 5);
        assert_eq!(end, 15);
    }

    #[test]
    fn test_edit_result() {
        let result = EditResult {
            version: 1,
            affected_paragraphs: smallvec::smallvec![ParagraphId(0)],
            created_paragraphs: smallvec::smallvec![],
            deleted_paragraphs: smallvec::smallvec![],
            new_cursor: DocPosition::default(),
        };
        assert!(result.has_changes());
    }
}
