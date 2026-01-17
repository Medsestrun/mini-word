//! Paragraph indexing for fast lookups

use std::collections::BTreeMap;

/// Stable identifier for paragraphs that survives edits
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParagraphId(pub u64);

impl Default for ParagraphId {
    fn default() -> Self {
        Self(0)
    }
}

/// Index structure for fast paragraph lookups
#[derive(Debug, Clone)]
pub struct ParagraphIndex {
    /// Maps start offset to paragraph ID
    offset_to_para: BTreeMap<usize, ParagraphId>,
    /// Maps paragraph ID to (start_offset, length)
    para_bounds: rustc_hash::FxHashMap<ParagraphId, (usize, usize)>,
    /// Sequential order of paragraphs
    order: Vec<ParagraphId>,
}

impl Default for ParagraphIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl ParagraphIndex {
    /// Create a new empty paragraph index
    pub fn new() -> Self {
        Self {
            offset_to_para: BTreeMap::new(),
            para_bounds: rustc_hash::FxHashMap::default(),
            order: Vec::new(),
        }
    }

    /// Insert a new paragraph
    pub fn insert(&mut self, para_id: ParagraphId, start_offset: usize, length: usize) {
        self.offset_to_para.insert(start_offset, para_id);
        self.para_bounds.insert(para_id, (start_offset, length));
        
        // Find insertion position in order
        let pos = self.order.iter().position(|&id| {
            self.para_bounds.get(&id).map(|(s, _)| *s > start_offset).unwrap_or(false)
        }).unwrap_or(self.order.len());
        
        self.order.insert(pos, para_id);
    }

    /// Insert a paragraph after another
    pub fn insert_after(
        &mut self,
        after: ParagraphId,
        para_id: ParagraphId,
        start_offset: usize,
        length: usize,
    ) {
        self.offset_to_para.insert(start_offset, para_id);
        self.para_bounds.insert(para_id, (start_offset, length));
        
        if let Some(pos) = self.order.iter().position(|&id| id == after) {
            self.order.insert(pos + 1, para_id);
        } else {
            self.order.push(para_id);
        }
    }

    /// Remove a paragraph
    pub fn remove(&mut self, para_id: ParagraphId) {
        if let Some((start, _)) = self.para_bounds.remove(&para_id) {
            self.offset_to_para.remove(&start);
        }
        self.order.retain(|&id| id != para_id);
    }

    /// Update paragraph length
    pub fn update_length(&mut self, para_id: ParagraphId, new_length: usize) {
        if let Some((_, len)) = self.para_bounds.get_mut(&para_id) {
            *len = new_length;
        }
    }

    /// Update lengths for paragraphs after a position
    pub fn update_lengths_after(&mut self, after_offset: usize, delta: isize) {
        // Collect paragraphs to update
        let to_update: Vec<_> = self.para_bounds
            .iter()
            .filter(|(_, (start, _))| *start > after_offset)
            .map(|(&id, _)| id)
            .collect();

        // Update offsets
        for para_id in to_update {
            if let Some((start, _)) = self.para_bounds.get(&para_id).copied() {
                // Remove old offset mapping
                self.offset_to_para.remove(&start);
                
                // Calculate new offset
                let new_start = (start as isize + delta) as usize;
                
                // Update bounds
                if let Some((s, _)) = self.para_bounds.get_mut(&para_id) {
                    *s = new_start;
                }
                
                // Add new offset mapping
                self.offset_to_para.insert(new_start, para_id);
            }
        }
    }

    /// Find paragraph containing an offset
    pub fn para_at_offset(&self, offset: usize) -> (ParagraphId, usize) {
        // Find the largest start offset <= target offset
        if let Some((&start, &para_id)) = self.offset_to_para.range(..=offset).next_back() {
            return (para_id, start);
        }

        // Return first paragraph if offset is 0 or before first paragraph
        if let Some(&para_id) = self.order.first() {
            if let Some(&(start, _)) = self.para_bounds.get(&para_id) {
                return (para_id, start);
            }
        }

        (ParagraphId(0), 0)
    }

    /// Get the first paragraph
    pub fn first(&self) -> ParagraphId {
        self.order.first().copied().unwrap_or(ParagraphId(0))
    }

    /// Get paragraph bounds
    pub fn bounds(&self, para_id: ParagraphId) -> Option<(usize, usize)> {
        self.para_bounds.get(&para_id).copied()
    }

    /// Iterate over paragraphs in order
    pub fn iter(&self) -> impl Iterator<Item = ParagraphId> + '_ {
        self.order.iter().copied()
    }

    /// Iterate over paragraphs starting from the given offset
    pub fn iter_from(&self, start_offset: usize) -> impl Iterator<Item = ParagraphId> + '_ {
        self.offset_to_para.range(start_offset..).map(|(_, v)| v).copied()
    }

    /// Get paragraph count
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Get next paragraph after given one
    pub fn next(&self, para_id: ParagraphId) -> Option<ParagraphId> {
        let pos = self.order.iter().position(|&id| id == para_id)?;
        self.order.get(pos + 1).copied()
    }

    /// Get previous paragraph before given one
    pub fn prev(&self, para_id: ParagraphId) -> Option<ParagraphId> {
        let pos = self.order.iter().position(|&id| id == para_id)?;
        if pos > 0 {
            self.order.get(pos - 1).copied()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_lookup() {
        let mut index = ParagraphIndex::new();
        
        index.insert(ParagraphId(0), 0, 10);
        index.insert(ParagraphId(1), 11, 15);
        index.insert(ParagraphId(2), 27, 20);

        assert_eq!(index.para_at_offset(5).0, ParagraphId(0));
        assert_eq!(index.para_at_offset(15).0, ParagraphId(1));
        assert_eq!(index.para_at_offset(30).0, ParagraphId(2));
    }

    #[test]
    fn test_order() {
        let mut index = ParagraphIndex::new();
        
        index.insert(ParagraphId(0), 0, 10);
        index.insert(ParagraphId(1), 11, 15);

        let order: Vec<_> = index.iter().collect();
        assert_eq!(order, vec![ParagraphId(0), ParagraphId(1)]);
    }

    #[test]
    fn test_next_prev() {
        let mut index = ParagraphIndex::new();
        
        index.insert(ParagraphId(0), 0, 10);
        index.insert(ParagraphId(1), 11, 15);
        index.insert(ParagraphId(2), 27, 20);

        assert_eq!(index.next(ParagraphId(0)), Some(ParagraphId(1)));
        assert_eq!(index.next(ParagraphId(1)), Some(ParagraphId(2)));
        assert_eq!(index.next(ParagraphId(2)), None);

        assert_eq!(index.prev(ParagraphId(0)), None);
        assert_eq!(index.prev(ParagraphId(1)), Some(ParagraphId(0)));
        assert_eq!(index.prev(ParagraphId(2)), Some(ParagraphId(1)));
    }
}
