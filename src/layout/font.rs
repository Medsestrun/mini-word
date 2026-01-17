//! Font metrics for layout

/// Metrics needed for text layout
#[derive(Debug, Clone)]
pub struct FontMetrics {
    /// Line height in logical pixels
    pub line_height: f32,
    /// Width of ASCII characters (0-127)
    pub char_widths: Vec<f32>,
    /// Default width for non-ASCII characters
    pub default_width: f32,
}

impl Default for FontMetrics {
    fn default() -> Self {
        // Default to the previous hardcoded values
        // 14px * 1.2 = 16.8
        // 8.41px for monospace char
        let default_width = 8.41;
        let mut char_widths = Vec::with_capacity(128);
        for _ in 0..128 {
            char_widths.push(default_width);
        }

        Self {
            line_height: 16.8,
            char_widths,
            default_width,
        }
    }
}

impl FontMetrics {
    pub fn new(line_height: f32, char_widths: Vec<f32>, default_width: f32) -> Self {
        Self {
            line_height,
            char_widths,
            default_width,
        }
    }

    /// Get width of a character
    pub fn width(&self, c: char) -> f32 {
        if c.is_ascii() {
            if let Some(w) = self.char_widths.get(c as usize) {
                return *w;
            }
        }
        self.default_width
    }
}

/// Unique identifier for a loaded font
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontId(pub u32);

/// Library of loaded fonts
#[derive(Debug, Clone)]
pub struct FontLibrary {
    fonts: std::collections::HashMap<FontId, FontMetrics>,
    next_id: u32,
}

impl Default for FontLibrary {
    fn default() -> Self {
        let mut fonts = std::collections::HashMap::new();
        // Add default font as ID 0
        fonts.insert(FontId(0), FontMetrics::default());
        
        Self {
            fonts,
            next_id: 1,
        }
    }
}

impl FontLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new font and return its ID
    pub fn add(&mut self, metrics: FontMetrics) -> FontId {
        let id = FontId(self.next_id);
        self.next_id += 1;
        self.fonts.insert(id, metrics);
        id
    }

    /// Set font metrics for a specific ID
    pub fn set(&mut self, id: FontId, metrics: FontMetrics) {
        self.fonts.insert(id, metrics);
    }

    /// Get font metrics by ID
    pub fn get(&self, id: FontId) -> Option<&FontMetrics> {
        self.fonts.get(&id)
    }
    
    /// Get mutable font metrics by ID (for updates)
    pub fn get_mut(&mut self, id: FontId) -> Option<&mut FontMetrics> {
        self.fonts.get_mut(&id)
    }
}
