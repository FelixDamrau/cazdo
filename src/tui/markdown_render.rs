//! Markdown to ratatui renderer.
//!
//! Converts Markdown to HTML with `pulldown-cmark` and delegates to
//! [`render_html`], reusing its wrapping and styling instead of duplicating them.

use pulldown_cmark::{Options, Parser, html};
use ratatui::text::Line;

use crate::tui::html_render::render_html;

/// Render Markdown to styled Lines; a `max_width` of 0 disables wrapping.
pub fn render_markdown(markdown: &str, max_width: usize) -> Vec<Line<'static>> {
    // Task lists intentionally off: html_render has no `<input>` checkbox support.
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;

    let mut html_buf = String::new();
    html::push_html(&mut html_buf, Parser::new_ext(markdown, options));

    render_html(&html_buf, max_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};
    use ratatui::text::Span;

    /// Join a rendered field into one string for content assertions.
    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("")
    }

    /// The first span whose content contains `needle`.
    fn span_with<'a>(lines: &'a [Line<'static>], needle: &str) -> &'a Span<'static> {
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.contains(needle))
            .unwrap_or_else(|| panic!("no span containing {needle:?}"))
    }

    #[test]
    fn renders_markdown_without_leaking_syntax() {
        let lines = render_markdown("# Title\n\nSome **bold** and *italic* text.", 80);
        let content = text(&lines);

        assert!(content.contains("Title"));
        assert!(content.contains("bold"));
        assert!(content.contains("italic"));
        assert!(!content.contains('#'));
        assert!(!content.contains("**"));
        assert!(!content.contains('*'));
    }

    #[test]
    fn bold_marker_becomes_a_bold_span() {
        let lines = render_markdown("plain **strong** plain", 80);
        let has_bold = lines.iter().flat_map(|line| line.spans.iter()).any(|span| {
            span.content.contains("strong") && span.style.add_modifier.contains(Modifier::BOLD)
        });

        assert!(has_bold, "bold markdown should produce a BOLD span");
    }

    #[test]
    fn bullet_list_becomes_bulleted_lines() {
        let lines = render_markdown("- first\n- second", 80);
        let content = text(&lines);

        assert!(content.contains("• first"));
        assert!(content.contains("• second"));
    }

    #[test]
    fn italic_marker_becomes_an_italic_span() {
        let lines = render_markdown("plain _emph_ plain", 80);
        assert!(
            span_with(&lines, "emph")
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    #[test]
    fn nested_italic_bold_matches_real_azure_markdown() {
        // WI 204's real Description value.
        let lines = render_markdown("THIS IS IN _**mark** down_", 80);
        let modifiers = span_with(&lines, "mark").style.add_modifier;

        assert!(modifiers.contains(Modifier::ITALIC));
        assert!(modifiers.contains(Modifier::BOLD));
    }

    #[test]
    fn link_shows_its_text_without_leaking_the_url() {
        let lines = render_markdown("see [the docs](https://example.test/page)", 80);

        assert!(text(&lines).contains("the docs"));
        assert!(!text(&lines).contains("https://example.test"));
        assert_eq!(span_with(&lines, "the docs").style.fg, Some(Color::Cyan));
    }

    #[test]
    fn inline_code_becomes_a_code_styled_span() {
        let lines = render_markdown("run `cargo test` first", 80);
        assert_eq!(
            span_with(&lines, "cargo test").style.fg,
            Some(Color::Yellow)
        );
    }

    #[test]
    fn task_list_markers_survive_as_literal_text() {
        let content = text(&render_markdown("- [x] done\n- [ ] open", 80));

        assert!(content.contains("[x] done"));
        assert!(content.contains("[ ] open"));
    }

    #[test]
    fn gfm_table_and_strikethrough_render() {
        let lines = render_markdown("| A | B |\n|---|---|\n| 1 | 2 |\n\n~~gone~~", 80);
        let content = text(&lines);

        assert!(content.contains("A") && content.contains("1"));
        assert!(
            span_with(&lines, "gone")
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
    }
}
