use std::fmt::Display;

/// Format an error for display in a user-facing message, retaining alternate
/// details such as an anyhow cause chain.
pub(crate) fn format_error_chain(error: &impl Display) -> String {
    format!("{error:#}")
}

#[cfg(test)]
mod tests {

    use super::format_error_chain;

    #[test]
    fn format_error_preserves_anyhow_context_chain() {
        let error = anyhow::anyhow!("leaf failure")
            .context("inner operation")
            .context("outer operation");

        assert_eq!(
            format_error_chain(&error),
            "outer operation: inner operation: leaf failure"
        );
    }
}
