//! Conservative presentation, not a credential detector or security assessment.
use crate::inventory::safe_text;

/// Instructions are prose, so perfect secret detection is impossible. Suppress
/// assignments, credential-like lines, PEM blocks and long opaque tokens.
pub fn preview(text: &str) -> String {
    let mut pem = false;
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            let start = lower.contains("-----begin");
            let end = lower.contains("-----end");
            let sensitive = pem
                || start
                || lower.contains("password")
                || lower.contains("secret")
                || lower.contains("token")
                || lower.contains("authorization")
                || lower.contains("api_key")
                || lower.contains("api-key")
                || lower.contains("credential")
                || lower.contains("private key")
                || line.contains('=')
                || line.contains("://")
                || line.split_whitespace().any(|word| {
                    word.len() > 40
                        && word
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_-/+.=".contains(&b))
                });
            if start {
                pem = true;
            }
            if end {
                pem = false;
            }
            if sensitive {
                "[redacted line]".into()
            } else {
                safe_text(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub fn path(path: &std::path::Path) -> String {
    preview(&path.to_string_lossy())
}
