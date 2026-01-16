//! Core layout engine with incremental update support

use crate::document::{BlockKind, BlockMeta, Document, ParagraphId};
use crate::editing::{Cursor, DocPosition, EditResult, Selection};
use crate::layout::line_break::LineBreaker;
use crate::layout::pagination::PageLayout;
use crate::render::{RenderDiff, LayoutDiff};
use crate::Rect;
use rustc_hash::{FxHashMap, FxHashSet};
use std::ops::Range;

/// Default line height in points
pub const LINE_HEIGHT: f32 = 14.0;

/// Default baseline offset from top of line
pub const BASELINE: f32 = 11.0;

/// Indentation width per level
pub const INDENT_WIDTH: f32 = 24.0;

/// Layout constraints for the document
#[derive(Debug, Clone, Copy)]
pub struct LayoutConstraints {
    pub page_width: f32,
    pub page_height: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
}

impl Default for LayoutConstraints {
    fn default() -> Self {
        Self {
            page_width: 612.0,  // US Letter
            page_height: 792.0,
            margin_top: 72.0,   // 1 inch
            margin_bottom: 72.0,
            margin_left: 72.0,
            margin_right: 72.0,
        }
    }
}

impl LayoutConstraints {
    /// Get usable content width
    pub fn content_width(&self) -> f32 {
        self.page_width - self.margin_left - self.margin_right
    }

    /// Get usable content height per page
    pub fn content_height(&self) -> f32 {
        self.page_height - self.margin_top - self.margin_bottom
    }
}

/// Information about a grapheme cluster for cursor positioning
#[derive(Debug, Clone)]
pub struct ClusterInfo {
    /// Byte offset within paragraph
    pub byte_offset: usize,
    /// X position from left edge of line
    pub x: f32,
    /// Width of this cluster
    pub width: f32,
}

/// Layout result for a single line
#[derive(Debug, Clone)]
pub struct LineLayout {
    /// Byte range within paragraph this line covers
    pub byte_range: Range<usize>,
    /// Grapheme cluster info for cursor positioning
    pub clusters: Vec<ClusterInfo>,
    /// Line height
    pub height: f32,
    /// Baseline offset from top of line
    pub baseline: f32,
    /// Actual width of content
    pub width: f32,
}

impl LineLayout {
    /// Find cluster at byte offset
    pub fn cluster_at_offset(&self, byte_offset: usize) -> Option<&ClusterInfo> {
        self.clusters.iter().find(|c| c.byte_offset == byte_offset)
    }

    /// Find X position for byte offset
    pub fn x_for_offset(&self, byte_offset: usize) -> f32 {
        for cluster in &self.clusters {
            if cluster.byte_offset >= byte_offset {
                return cluster.x;
            }
        }
        self.width
    }

    /// Find byte offset for X position
    pub fn offset_for_x(&self, x: f32) -> usize {
        let mut best_offset = self.byte_range.start;
        let mut best_dist = f32::MAX;

        for cluster in &self.clusters {
            let dist = (cluster.x - x).abs();
            if dist < best_dist {
                best_dist = dist;
                best_offset = cluster.byte_offset;
            }

            // Also check end of cluster
            let end_x = cluster.x + cluster.width;
            let end_dist = (end_x - x).abs();
            if end_dist < best_dist {
                best_dist = end_dist;
                best_offset = cluster.byte_offset + 1; // Approximate
            }
        }

        best_offset
    }
}

/// Layout result for a paragraph
#[derive(Debug, Clone)]
pub struct ParagraphLayout {
    pub para_id: ParagraphId,
    /// Lines produced by line breaking
    pub lines: Vec<LineLayout>,
    /// Total height including spacing
    pub total_height: f32,
    /// Hash of paragraph content for change detection
    pub content_hash: u64,
}

impl ParagraphLayout {
    /// Get the line containing a byte offset
    pub fn line_at_offset(&self, byte_offset: usize) -> Option<(usize, &LineLayout)> {
        for (idx, line) in self.lines.iter().enumerate() {
            if line.byte_range.contains(&byte_offset) || 
               (byte_offset == line.byte_range.end && idx == self.lines.len() - 1) {
                return Some((idx, line));
            }
        }
        self.lines.last().map(|l| (self.lines.len() - 1, l))
    }

    /// Get total line count
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}

/// Complete layout state with incremental update support
pub struct LayoutState {
    /// Per-paragraph layout results
    paragraph_layouts: FxHashMap<ParagraphId, ParagraphLayout>,
    /// Page break positions
    pages: Vec<PageLayout>,
    /// Layout constraints
    constraints: LayoutConstraints,
    /// Paragraphs needing relayout
    dirty_paragraphs: FxHashSet<ParagraphId>,
    /// Version of document this layout corresponds to
    layout_version: u64,
    /// Line breaker instance
    line_breaker: LineBreaker,
    /// Y offset for each paragraph (cached)
    paragraph_y_offsets: FxHashMap<ParagraphId, f32>,
}

impl LayoutState {
    /// Create new layout state
    pub fn new(constraints: LayoutConstraints) -> Self {
        Self {
            paragraph_layouts: FxHashMap::default(),
            pages: Vec::new(),
            constraints,
            dirty_paragraphs: FxHashSet::default(),
            layout_version: 0,
            line_breaker: LineBreaker::new(),
            paragraph_y_offsets: FxHashMap::default(),
        }
    }

    /// Get constraints
    pub fn constraints(&self) -> &LayoutConstraints {
        &self.constraints
    }

    /// Mark paragraphs as needing relayout based on edit result
    pub fn invalidate(&mut self, edit_result: &EditResult) {
        for para_id in &edit_result.affected_paragraphs {
            self.dirty_paragraphs.insert(*para_id);
        }

        for para_id in &edit_result.created_paragraphs {
            self.dirty_paragraphs.insert(*para_id);
        }

        for para_id in &edit_result.deleted_paragraphs {
            self.paragraph_layouts.remove(para_id);
            self.paragraph_y_offsets.remove(para_id);
        }
    }

    /// Mark all paragraphs as dirty (for full relayout)
    pub fn invalidate_all(&mut self) {
        for para_id in self.paragraph_layouts.keys().copied().collect::<Vec<_>>() {
            self.dirty_paragraphs.insert(para_id);
        }
    }

    /// Perform incremental relayout
    pub fn relayout(&mut self, document: &Document) -> RenderDiff {
        let mut layout_diff = LayoutDiff::new();

        // Phase 1: Relayout dirty paragraphs
        let dirty: Vec<_> = self.dirty_paragraphs.drain().collect();

        for para_id in dirty {
            let old_height = self.paragraph_layouts
                .get(&para_id)
                .map(|l| l.total_height);

            // Get paragraph text and metadata
            let para_text = document.paragraph_text(para_id);
            let block_meta = document.block_meta(para_id)
                .cloned()
                .unwrap_or_else(|| BlockMeta {
                    kind: BlockKind::Paragraph,
                    start_offset: 0,
                    byte_len: para_text.len(),
                });

            // Perform line breaking
            let new_layout = self.line_breaker.layout_paragraph(
                para_id,
                &para_text,
                &block_meta,
                self.constraints.content_width(),
            );

            let new_height = new_layout.total_height;

            // Record change
            layout_diff.changed_paragraphs.insert(para_id);

            // Store new layout
            self.paragraph_layouts.insert(para_id, new_layout);

            // Height change triggers repagination
            if old_height != Some(new_height) {
                layout_diff.pagination_dirty = true;
            }
        }

        // Ensure all paragraphs have layouts
        for para_id in document.paragraph_order() {
            if !self.paragraph_layouts.contains_key(&para_id) {
                let para_text = document.paragraph_text(para_id);
                let block_meta = document.block_meta(para_id)
                    .cloned()
                    .unwrap_or_else(|| BlockMeta {
                        kind: BlockKind::Paragraph,
                        start_offset: 0,
                        byte_len: para_text.len(),
                    });

                let layout = self.line_breaker.layout_paragraph(
                    para_id,
                    &para_text,
                    &block_meta,
                    self.constraints.content_width(),
                );

                self.paragraph_layouts.insert(para_id, layout);
                layout_diff.changed_paragraphs.insert(para_id);
                layout_diff.pagination_dirty = true;
            }
        }

        // Phase 2: Repaginate if needed
        if layout_diff.pagination_dirty || self.pages.is_empty() {
            self.repaginate(document);
        }

        // Update Y offsets
        self.update_y_offsets(document);

        self.layout_version = document.version();

        // Build render diff
        RenderDiff::from_layout_diff(layout_diff, self.layout_version)
    }

    /// Recompute page breaks
    fn repaginate(&mut self, document: &Document) {
        self.pages.clear();

        let mut current_page = PageLayout::new(0);
        let mut y_on_page: f32 = 0.0;
        let content_height = self.constraints.content_height();

        for para_id in document.paragraph_order() {
            if let Some(para_layout) = self.paragraph_layouts.get(&para_id) {
                for (line_idx, line) in para_layout.lines.iter().enumerate() {
                    // Check if line fits on current page
                    if y_on_page + line.height > content_height && y_on_page > 0.0 {
                        // Finalize current page
                        self.pages.push(current_page);

                        // Start new page
                        current_page = PageLayout::new(self.pages.len());
                        current_page.start_para = para_id;
                        current_page.start_line = line_idx;
                        y_on_page = 0.0;
                    }

                    current_page.end_para = para_id;
                    current_page.end_line = line_idx;
                    y_on_page += line.height;
                }
            }
        }

        // Finalize last page
        self.pages.push(current_page);
    }

    /// Update Y offsets for each paragraph
    fn update_y_offsets(&mut self, document: &Document) {
        self.paragraph_y_offsets.clear();

        let mut y: f32 = 0.0;
        for para_id in document.paragraph_order() {
            self.paragraph_y_offsets.insert(para_id, y);

            if let Some(layout) = self.paragraph_layouts.get(&para_id) {
                y += layout.total_height;
            }
        }
    }

    /// Get page count
    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    /// Get pages
    pub fn pages(&self) -> &[PageLayout] {
        &self.pages
    }

    /// Get paragraph layout
    pub fn paragraph_layout(&self, para_id: ParagraphId) -> Option<&ParagraphLayout> {
        self.paragraph_layouts.get(&para_id)
    }

    /// Get Y offset for paragraph
    pub fn paragraph_y(&self, para_id: ParagraphId) -> f32 {
        self.paragraph_y_offsets.get(&para_id).copied().unwrap_or(0.0)
    }

    /// Convert position to X coordinate
    pub fn position_to_x(&self, document: &Document, pos: &DocPosition) -> Option<f32> {
        let layout = self.paragraph_layouts.get(&pos.para_id)?;
        let (_, line) = layout.line_at_offset(pos.offset)?;
        Some(line.x_for_offset(pos.offset) + self.constraints.margin_left)
    }

    /// Move cursor vertically
    pub fn move_cursor_vertical(
        &self,
        document: &Document,
        current_pos: &DocPosition,
        delta_lines: i32,
        preferred_x: Option<f32>,
    ) -> Option<DocPosition> {
        let layout = self.paragraph_layouts.get(&current_pos.para_id)?;
        let (current_line_idx, current_line) = layout.line_at_offset(current_pos.offset)?;

        // Get X position to maintain
        let target_x = preferred_x.unwrap_or_else(|| {
            current_line.x_for_offset(current_pos.offset)
        });

        let target_line_idx = current_line_idx as i32 + delta_lines;

        if target_line_idx >= 0 && (target_line_idx as usize) < layout.lines.len() {
            // Same paragraph
            let target_line = &layout.lines[target_line_idx as usize];
            let new_offset = target_line.offset_for_x(target_x);
            Some(DocPosition::new(current_pos.para_id, new_offset))
        } else if delta_lines < 0 {
            // Move to previous paragraph
            let prev_para = document.paragraph_order()
                .take_while(|&id| id != current_pos.para_id)
                .last()?;
            
            let prev_layout = self.paragraph_layouts.get(&prev_para)?;
            let target_line = prev_layout.lines.last()?;
            let new_offset = target_line.offset_for_x(target_x);
            Some(DocPosition::new(prev_para, new_offset))
        } else {
            // Move to next paragraph
            let mut found_current = false;
            let next_para = document.paragraph_order()
                .find(|&id| {
                    if id == current_pos.para_id {
                        found_current = true;
                        false
                    } else {
                        found_current
                    }
                })?;
            
            let next_layout = self.paragraph_layouts.get(&next_para)?;
            let target_line = next_layout.lines.first()?;
            let new_offset = target_line.offset_for_x(target_x);
            Some(DocPosition::new(next_para, new_offset))
        }
    }

    /// Build display list for viewport
    pub fn build_display_list(
        &self,
        document: &Document,
        viewport: Rect,
        cursor: &Cursor,
        selection: Option<&Selection>,
    ) -> crate::render::DisplayList {
        crate::render::DisplayList::build(
            document,
            self,
            viewport,
            cursor,
            selection,
        )
    }

    /// Get indent for block type
    pub fn indent_for(&self, block_meta: &BlockMeta) -> f32 {
        match &block_meta.kind {
            BlockKind::ListItem { indent_level, .. } => {
                *indent_level as f32 * INDENT_WIDTH
            }
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_constraints() {
        let constraints = LayoutConstraints::default();
        assert_eq!(constraints.content_width(), 468.0); // 612 - 72 - 72
        assert_eq!(constraints.content_height(), 648.0); // 792 - 72 - 72
    }

    #[test]
    fn test_line_layout_x_for_offset() {
        let line = LineLayout {
            byte_range: 0..5,
            clusters: vec![
                ClusterInfo { byte_offset: 0, x: 0.0, width: 8.0 },
                ClusterInfo { byte_offset: 1, x: 8.0, width: 8.0 },
                ClusterInfo { byte_offset: 2, x: 16.0, width: 8.0 },
            ],
            height: LINE_HEIGHT,
            baseline: BASELINE,
            width: 24.0,
        };

        assert_eq!(line.x_for_offset(0), 0.0);
        assert_eq!(line.x_for_offset(1), 8.0);
        assert_eq!(line.x_for_offset(2), 16.0);
    }
}
