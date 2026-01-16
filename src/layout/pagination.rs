//! Pagination for multi-page documents

use crate::document::ParagraphId;

/// Layout information for a page
#[derive(Debug, Clone)]
pub struct PageLayout {
    /// Page index (0-based)
    pub page_index: usize,
    /// Starting paragraph
    pub start_para: ParagraphId,
    /// Starting line index within paragraph
    pub start_line: usize,
    /// Ending paragraph
    pub end_para: ParagraphId,
    /// Ending line index within paragraph
    pub end_line: usize,
}

impl PageLayout {
    /// Create a new page layout
    pub fn new(page_index: usize) -> Self {
        Self {
            page_index,
            start_para: ParagraphId(0),
            start_line: 0,
            end_para: ParagraphId(0),
            end_line: 0,
        }
    }

    /// Check if this page contains a given paragraph
    pub fn contains_paragraph(&self, para_id: ParagraphId) -> bool {
        para_id >= self.start_para && para_id <= self.end_para
    }
}

/// Position within a page
#[derive(Debug, Clone, Copy)]
pub struct PagePosition {
    pub para_id: ParagraphId,
    pub line_index: usize,
}

impl PagePosition {
    pub fn new(para_id: ParagraphId, line_index: usize) -> Self {
        Self { para_id, line_index }
    }
}

/// Pagination rules
#[derive(Debug, Clone)]
pub struct PaginationRules {
    /// Minimum lines before a break (widow control)
    pub min_lines_before_break: usize,
    /// Minimum lines after a break (orphan control)
    pub min_lines_after_break: usize,
    /// Keep heading with following paragraph
    pub keep_heading_with_next: bool,
}

impl Default for PaginationRules {
    fn default() -> Self {
        Self {
            min_lines_before_break: 2,
            min_lines_after_break: 2,
            keep_heading_with_next: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_layout() {
        let page = PageLayout {
            page_index: 0,
            start_para: ParagraphId(0),
            start_line: 0,
            end_para: ParagraphId(2),
            end_line: 5,
        };

        assert!(page.contains_paragraph(ParagraphId(0)));
        assert!(page.contains_paragraph(ParagraphId(1)));
        assert!(page.contains_paragraph(ParagraphId(2)));
        assert!(!page.contains_paragraph(ParagraphId(3)));
    }
}
