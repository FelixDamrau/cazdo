/// Simple wildcard pattern matching for branch names
///
/// Supports:
/// - `*` matches any sequence of characters (including empty)
/// - Literal characters match themselves
///
/// Examples:
/// - `main` matches only "main"
/// - `releases/*` matches "releases/v1.0", "releases/v12.x"
/// - `feature-*-test` matches "feature-123-test", "feature-abc-test"

/// Check if a branch name matches a single pattern
pub fn matches_pattern(text: &str, pattern: &str) -> bool {
    matches_pattern_impl(text.as_bytes(), pattern.as_bytes())
}

fn matches_pattern_impl(text: &[u8], pattern: &[u8]) -> bool {
    let mut t = 0; // text index
    let mut p = 0; // pattern index
    let mut star_p = None; // position after last '*' in pattern
    let mut star_t = 0; // position in text when we matched '*'

    while t < text.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            // Record the position and try matching zero characters
            star_p = Some(p + 1);
            star_t = t;
            p += 1;
        } else if p < pattern.len() && text[t] == pattern[p] {
            // Characters match, advance both
            t += 1;
            p += 1;
        } else if let Some(sp) = star_p {
            // Mismatch, but we have a star to backtrack to
            // Try matching one more character with the star
            star_t += 1;
            t = star_t;
            p = sp;
        } else {
            // No match and no star to backtrack
            return false;
        }
    }

    // Skip any remaining '*' in pattern
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }

    p == pattern.len()
}

/// Check if a branch name matches any of the given patterns
pub fn is_protected(branch_name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| matches_pattern(branch_name, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert!(matches_pattern("main", "main"));
        assert!(matches_pattern("master", "master"));
        assert!(!matches_pattern("main", "master"));
        assert!(!matches_pattern("main-feature", "main"));
    }

    #[test]
    fn test_wildcard_suffix() {
        assert!(matches_pattern("releases/v1.0", "releases/*"));
        assert!(matches_pattern("releases/v12.x", "releases/*"));
        assert!(matches_pattern("releases/", "releases/*"));
        assert!(!matches_pattern("releases", "releases/*"));
        assert!(!matches_pattern("other/v1.0", "releases/*"));
    }

    #[test]
    fn test_wildcard_prefix() {
        assert!(matches_pattern("feature-main", "*main"));
        assert!(matches_pattern("main", "*main"));
        assert!(!matches_pattern("main-feature", "*main"));
    }

    #[test]
    fn test_wildcard_middle() {
        assert!(matches_pattern("feature-123-test", "feature-*-test"));
        assert!(matches_pattern("feature--test", "feature-*-test"));
        assert!(!matches_pattern("feature-123-prod", "feature-*-test"));
    }

    #[test]
    fn test_multiple_wildcards() {
        assert!(matches_pattern("a/b/c", "*/*"));
        assert!(matches_pattern("foo/bar/baz", "*/*"));
    }

    #[test]
    fn test_is_protected() {
        let patterns = vec![
            "main".to_string(),
            "master".to_string(),
            "releases/*".to_string(),
        ];

        assert!(is_protected("main", &patterns));
        assert!(is_protected("master", &patterns));
        assert!(is_protected("releases/v1.0", &patterns));
        assert!(!is_protected("feature/123", &patterns));
        assert!(!is_protected("develop", &patterns));
    }

    #[test]
    fn test_star_only() {
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("", "*"));
    }
}
