//! Safe file names for on-disk chat storage under `~/.atria`.

/// Sanitize a chat id into a safe file-stem: only ASCII letters, digits, `-`
/// and `_`; anything else collapses to `_`; capped at 64 chars; never empty.
pub fn sanitize_file_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len().min(64));
    for c in id.chars() {
        if out.len() >= 64 {
            break;
        }
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches(|c| c == '_' || c == '-').to_string();
    if trimmed.is_empty() {
        "chat".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_id_unchanged() {
        assert_eq!(sanitize_file_id("c1758000000abcd"), "c1758000000abcd");
    }

    #[test]
    fn traversal_is_neutralized() {
        let s = sanitize_file_id("../../etc/passwd");
        assert!(!s.contains('/') && !s.contains('.') && !s.contains('\\'));
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn windows_separators_neutralized() {
        let s = sanitize_file_id("a\\b:c*d?e\"f<g>h|i");
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }

    #[test]
    fn empty_and_unicode_fall_back() {
        assert_eq!(sanitize_file_id(""), "chat");
        assert_eq!(sanitize_file_id("___"), "chat");
        assert_eq!(sanitize_file_id("chat id / 1"), "chat_id___1");
    }

    #[test]
    fn long_ids_are_capped() {
        let s = sanitize_file_id(&"x".repeat(500));
        assert!(s.len() <= 64);
    }
}
