//! HTML to ratatui renderer
//!
//! Converts HTML content from Azure DevOps work items into styled ratatui Lines.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Context for tracking list state
#[derive(Clone)]
enum ListType {
    Unordered,
    Ordered(usize), // current item number
}

/// Parser state for HTML rendering
struct HtmlParser {
    /// Stack of active style modifiers (bold, italic)
    style_stack: Vec<Modifier>,
    /// Stack of active lists for nesting
    list_stack: Vec<ListType>,
    /// Current line being built
    current_spans: Vec<Span<'static>>,
    /// Accumulated text for current span
    current_text: String,
    /// Current style being applied
    current_style: Style,
    /// Output lines
    lines: Vec<Line<'static>>,
    /// Whether last emitted line was blank (for collapsing)
    last_was_blank: bool,
    /// Whether we're inside an anchor tag
    in_anchor: bool,
    /// Work item ID extracted from anchor href
    anchor_work_item_id: Option<u32>,
    /// Maximum width for text wrapping
    max_width: usize,
    /// Current line width for wrapping
    current_line_width: usize,
    /// Indent prefix for current context
    indent: String,
}

impl HtmlParser {
    fn new(max_width: usize) -> Self {
        Self {
            style_stack: Vec::new(),
            list_stack: Vec::new(),
            current_spans: Vec::new(),
            current_text: String::new(),
            current_style: Style::default(),
            lines: Vec::new(),
            last_was_blank: false,
            in_anchor: false,
            anchor_work_item_id: None,
            max_width,
            current_line_width: 0,
            indent: String::new(),
        }
    }

    /// Compute current style from style stack
    fn compute_style(&self) -> Style {
        let mut style = Style::default();
        for modifier in &self.style_stack {
            style = style.add_modifier(*modifier);
        }
        if self.in_anchor {
            style = style.fg(Color::Cyan);
        }
        style
    }

    /// Flush current text to a span
    fn flush_text(&mut self) {
        if !self.current_text.is_empty() {
            self.current_spans.push(Span::styled(
                std::mem::take(&mut self.current_text),
                self.current_style,
            ));
        }
    }

    /// Flush current spans to a line
    fn flush_line(&mut self) {
        self.flush_text();

        let is_blank = self.current_spans.is_empty()
            || self
                .current_spans
                .iter()
                .all(|s| s.content.trim().is_empty());

        // Collapse consecutive blank lines
        if is_blank {
            if !self.last_was_blank && !self.lines.is_empty() {
                self.lines.push(Line::from(vec![]));
                self.last_was_blank = true;
            }
        } else {
            // Add indent if we have one
            if !self.indent.is_empty() && !self.current_spans.is_empty() {
                let mut spans = vec![Span::raw(self.indent.clone())];
                spans.append(&mut self.current_spans);
                self.lines.push(Line::from(spans));
            } else {
                self.lines
                    .push(Line::from(std::mem::take(&mut self.current_spans)));
            }
            self.last_was_blank = false;
        }

        self.current_spans = Vec::new();
        self.current_line_width = 0;
    }

    /// Add text content, handling word wrapping
    fn add_text(&mut self, text: &str) {
        let text = decode_html_entities(text);

        // Handle word wrapping
        for word in text.split_inclusive(char::is_whitespace) {
            let word_width = word.chars().count();
            let indent_width = self.indent.chars().count();
            let effective_max = self.max_width.saturating_sub(indent_width);

            // Check if we need to wrap
            if self.current_line_width + word_width > effective_max && self.current_line_width > 0 {
                self.flush_line();
            }

            self.current_text.push_str(word);
            self.current_line_width += word_width;
        }
    }

    /// Update indent based on list stack depth
    fn update_indent(&mut self) {
        self.indent = "  ".repeat(self.list_stack.len());
    }

    /// Handle opening tag
    fn handle_open_tag(&mut self, tag: &str, attrs: &str) {
        let tag_lower = tag.to_lowercase();

        match tag_lower.as_str() {
            // Block elements that create line breaks
            "br" => {
                self.flush_line();
            }
            "p" | "div" | "h4" | "h5" | "h6" => {
                if !self.current_spans.is_empty() || !self.current_text.is_empty() {
                    self.flush_line();
                }
            }
            "h1" | "h2" | "h3" => {
                self.flush_line();
                // Add blank line before header if we have content
                if !self.lines.is_empty() && !self.last_was_blank {
                    self.lines.push(Line::from(vec![]));
                }
                self.flush_text();
                self.current_style = self.compute_style();
                self.style_stack.push(Modifier::BOLD);
                self.current_style = self.compute_style();
            }

            // Inline formatting
            "b" | "strong" => {
                self.flush_text();
                self.style_stack.push(Modifier::BOLD);
                self.current_style = self.compute_style();
            }
            "u" => {
                self.flush_text();
                self.style_stack.push(Modifier::UNDERLINED);
                self.current_style = self.compute_style();
            }
            "s" | "strike" | "del" => {
                self.flush_text();
                self.style_stack.push(Modifier::CROSSED_OUT);
                self.current_style = self.compute_style();
            }

            // Links
            "a" => {
                self.flush_text();
                self.in_anchor = true;
                self.anchor_work_item_id = extract_work_item_id(attrs);
                self.current_style = self.compute_style();
            }

            // Lists
            "ul" => {
                self.flush_line();
                self.list_stack.push(ListType::Unordered);
                self.update_indent();
            }
            "ol" => {
                self.flush_line();
                self.list_stack.push(ListType::Ordered(0));
                self.update_indent();
            }
            "li" => {
                self.flush_line();

                // Get list prefix
                let prefix = if let Some(list_type) = self.list_stack.last_mut() {
                    match list_type {
                        ListType::Unordered => "• ".to_string(),
                        ListType::Ordered(n) => {
                            *n += 1;
                            format!("{}. ", n)
                        }
                    }
                } else {
                    "• ".to_string()
                };

                // Add prefix with indent
                self.current_spans.push(Span::raw(prefix));
                self.current_line_width = 2; // Account for prefix width
            }

            // Images
            "img" => {
                self.flush_text();
                self.current_spans.push(Span::styled(
                    "[image]",
                    Style::default().fg(Color::DarkGray),
                ));
            }

            // Table handling (basic)
            "table" | "tbody" => {
                self.flush_line();
            }
            "tr" => {
                self.flush_line();
            }
            "td" | "th" => {
                if !self.current_text.is_empty() || !self.current_spans.is_empty() {
                    self.add_text(" | ");
                }
            }

            // Code
            "code" | "pre" => {
                self.flush_text();
                self.current_style = self.compute_style().fg(Color::Yellow);
            }

            _ => {}
        }
    }

    /// Handle closing tag
    fn handle_close_tag(&mut self, tag: &str) {
        let tag_lower = tag.to_lowercase();

        match tag_lower.as_str() {
            // Block elements
            "p" | "div" | "h4" | "h5" | "h6" => {
                self.flush_line();
            }
            "h1" | "h2" | "h3" => {
                self.flush_text();
                self.style_stack.pop();
                self.current_style = self.compute_style();
                self.flush_line();
            }

            // Inline formatting
            "b" | "strong" | "u" | "s" | "strike" | "del" => {
                self.flush_text();
                self.style_stack.pop();
                self.current_style = self.compute_style();
            }

            // Links
            "a" => {
                // If we found a work item ID, show it as a reference
                if let Some(wi_id) = self.anchor_work_item_id.take() {
                    self.flush_text();
                    self.current_spans.push(Span::styled(
                        format!("#{}", wi_id),
                        Style::default().fg(Color::Cyan),
                    ));
                }
                self.in_anchor = false;
                self.current_style = self.compute_style();
            }

            // Lists
            "ul" | "ol" => {
                self.flush_line();
                self.list_stack.pop();
                self.update_indent();
            }

            // Table
            "tr" => {
                self.flush_line();
            }
            "table" => {
                self.flush_line();
            }

            // Code
            "code" | "pre" => {
                self.flush_text();
                self.current_style = self.compute_style();
            }

            _ => {}
        }
    }

    /// Parse and render HTML to Lines
    fn parse(mut self, html: &str) -> Vec<Line<'static>> {
        let mut chars = html.chars().peekable();
        let mut in_tag = false;
        let mut tag_content = String::new();

        while let Some(c) = chars.next() {
            if c == '<' {
                // Flush any pending text before tag
                if !in_tag {
                    in_tag = true;
                    tag_content.clear();
                }
            } else if c == '>' && in_tag {
                in_tag = false;
                self.process_tag(&tag_content);
                tag_content.clear();
            } else if in_tag {
                tag_content.push(c);
            } else {
                // Regular text content
                let mut text = String::new();
                text.push(c);

                // Collect consecutive text
                while let Some(&next_c) = chars.peek() {
                    if next_c == '<' {
                        break;
                    }
                    text.push(chars.next().unwrap());
                }

                // Normalize whitespace
                let normalized = normalize_whitespace(&text);
                if !normalized.is_empty() {
                    self.add_text(&normalized);
                }
            }
        }

        // Flush any remaining content
        self.flush_line();

        // Remove trailing blank lines
        while self
            .lines
            .last()
            .map(|l| l.spans.is_empty())
            .unwrap_or(false)
        {
            self.lines.pop();
        }

        self.lines
    }

    /// Process a tag string (without < >)
    fn process_tag(&mut self, tag_content: &str) {
        let tag_content = tag_content.trim();

        if let Some(rest) = tag_content.strip_prefix('/') {
            // Closing tag
            let tag_name = rest.split_whitespace().next().unwrap_or("");
            self.handle_close_tag(tag_name);
        } else if let Some(rest) = tag_content.strip_suffix('/') {
            // Self-closing tag
            let parts: Vec<&str> = rest.trim().splitn(2, char::is_whitespace).collect();
            let tag_name = parts.first().unwrap_or(&"");
            let attrs = parts.get(1).unwrap_or(&"");
            self.handle_open_tag(tag_name, attrs);
        } else {
            // Opening tag
            let parts: Vec<&str> = tag_content.splitn(2, char::is_whitespace).collect();
            let tag_name = parts.first().unwrap_or(&"");
            let attrs = parts.get(1).unwrap_or(&"");
            self.handle_open_tag(tag_name, attrs);
        }
    }
}

/// Decode common HTML entities
fn decode_html_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&#x27;", "'")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&hellip;", "…")
        .replace("&bull;", "•")
        .replace("&copy;", "©")
        .replace("&reg;", "®")
        .replace("&trade;", "™")
}

/// Normalize whitespace in text content
fn normalize_whitespace(s: &str) -> String {
    let mut result = String::new();
    let mut last_was_space = false;

    for c in s.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }

    result
}

/// Extract work item ID from anchor href attribute
/// Looks for patterns like: href="...workitems/edit/123" or href="...workitems/123"
fn extract_work_item_id(attrs: &str) -> Option<u32> {
    // Find href attribute
    let href_start = attrs.find("href=")?;
    let rest = &attrs[href_start + 5..];

    // Find the URL value (handle both single and double quotes)
    let url = if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        &stripped[..end]
    } else if let Some(stripped) = rest.strip_prefix('\'') {
        let end = stripped.find('\'')?;
        &stripped[..end]
    } else {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        &rest[..end]
    };

    // Look for work item patterns
    // Pattern 1: workitems/edit/123
    if let Some(pos) = url.find("workitems/edit/") {
        let id_start = pos + "workitems/edit/".len();
        let id_str: String = url[id_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        return id_str.parse().ok();
    }

    // Pattern 2: workitems/123
    if let Some(pos) = url.find("workitems/") {
        let id_start = pos + "workitems/".len();
        let id_str: String = url[id_start..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        return id_str.parse().ok();
    }

    None
}

/// Render HTML content to styled ratatui Lines
///
/// # Arguments
/// * `html` - HTML string to parse
/// * `max_width` - Maximum width for word wrapping (0 = no wrapping)
///
/// # Returns
/// Vector of styled Lines ready for ratatui Paragraph
pub fn render_html(html: &str, max_width: usize) -> Vec<Line<'static>> {
    let parser = HtmlParser::new(max_width);
    parser.parse(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_text() {
        let lines = render_html("Hello world", 80);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_paragraph() {
        let lines = render_html("<p>First paragraph</p><p>Second paragraph</p>", 80);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_bold() {
        let lines = render_html("Hello <b>bold</b> world", 80);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].spans.len() >= 2);
    }

    #[test]
    fn test_list() {
        let lines = render_html("<ul><li>Item 1</li><li>Item 2</li></ul>", 80);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_ordered_list() {
        let lines = render_html("<ol><li>First</li><li>Second</li></ol>", 80);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_work_item_link() {
        let id =
            extract_work_item_id(r#"href="https://dev.azure.com/org/project/_workitems/edit/123""#);
        assert_eq!(id, Some(123));
    }

    #[test]
    fn test_html_entities() {
        let decoded = decode_html_entities("Hello&nbsp;&amp;&nbsp;world");
        assert_eq!(decoded, "Hello & world");
    }

    #[test]
    fn test_nested_list() {
        let html = "<ul><li>Outer<ul><li>Inner</li></ul></li></ul>";
        let lines = render_html(html, 80);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_list_spacing() {
        let lines = render_html("<ul><li>Item 1</li><li>Item 2</li></ul>", 80);
        // Should be exactly 2 lines if no blank lines are inserted
        assert_eq!(lines.len(), 2);
        assert!(lines[0].spans.iter().any(|s| s.content.contains("Item 1")));
        assert!(lines[1].spans.iter().any(|s| s.content.contains("Item 2")));
    }
}
