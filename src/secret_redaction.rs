use std::path::Path;
use std::sync::OnceLock;

use regex::{Captures, Regex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedactedText {
    pub text: String,
    pub redacted: bool,
}

pub fn redact_sensitive_text(locator: &str, content: &str) -> RedactedText {
    let mut text = redact_private_key_blocks(content);

    text = assignment_regex()
        .replace_all(&text, |captures: &Captures<'_>| {
            format!(
                "{}{}{}[REDACTED]{}",
                &captures[1], &captures[2], &captures[3], &captures[5]
            )
        })
        .into_owned();
    text = bearer_regex()
        .replace_all(&text, "${1}[REDACTED]")
        .into_owned();
    text = token_value_regex()
        .replace_all(&text, "[REDACTED]")
        .into_owned();

    if is_secret_material_path(locator) {
        text = redact_secret_file_lines(&text);
    }

    RedactedText {
        redacted: text != content,
        text,
    }
}

fn redact_secret_file_lines(content: &str) -> String {
    let mut lines = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with('{')
            || trimmed.starts_with('}')
            || trimmed.starts_with('[')
            || trimmed.starts_with(']')
            || trimmed.starts_with("--- ")
            || trimmed.starts_with("+++ ")
            || trimmed.contains("[REDACTED]")
        {
            lines.push(line.to_string());
            continue;
        }

        let separator = line
            .find('=')
            .or_else(|| line.find(':'))
            .map(|index| index + 1);
        if let Some(separator) = separator {
            let suffix = if line.trim_end().ends_with(',') {
                ","
            } else {
                ""
            };
            lines.push(format!("{} [REDACTED]{}", &line[..separator], suffix));
        } else {
            lines.push("[REDACTED]".to_string());
        }
    }

    let mut text = lines.join("\n");
    if content.ends_with('\n') {
        text.push('\n');
    }
    text
}

pub fn is_secret_material_path(raw: &str) -> bool {
    let normalized = raw.trim().replace('\\', "/").to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    let path = Path::new(&normalized);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let components = normalized.split('/').collect::<Vec<_>>();

    file_name == ".env"
        || file_name.starts_with(".env.")
        || matches!(
            file_name,
            "id_rsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
                | "auth.json"
                | "kubeconfig"
                | ".npmrc"
                | ".pypirc"
        )
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
        || components
            .windows(2)
            .any(|parts| matches!(parts, [".ssh", "config"] | [".aws", "credentials"]))
}

fn redact_private_key_blocks(content: &str) -> String {
    let mut inside_private_key = false;
    let mut output = Vec::new();

    for line in content.lines() {
        let upper = line.to_ascii_uppercase();
        if upper.contains("-----BEGIN") && upper.contains("PRIVATE KEY-----") {
            inside_private_key = true;
            output.push(line.to_string());
            output.push("[REDACTED]".to_string());
            continue;
        }
        if inside_private_key {
            if upper.contains("-----END") && upper.contains("PRIVATE KEY-----") {
                inside_private_key = false;
                output.push(line.to_string());
            }
            continue;
        }
        output.push(line.to_string());
    }

    let mut text = output.join("\n");
    if content.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn assignment_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?ix)
            (
                api[_-]?key|apikey|access[_-]?token|refresh[_-]?token|
                client[_-]?secret|private[_-]?key|password|passwd|secret
            )
            (\s*[:=]\s*)
            (["']?)
            ([^"'\s,;}\]]+)
            (["']?)
            "#,
        )
        .expect("sensitive assignment regex should compile")
    })
}

fn bearer_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?i)(authorization\s*[:=]\s*bearer\s+|bearer\s+)[A-Za-z0-9._~+/=-]+")
            .expect("bearer regex should compile")
    })
}

fn token_value_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"\b(?:sk-|ghp_|github_pat_|xox[baprs]-)[A-Za-z0-9_-]{8,}\b")
            .expect("token value regex should compile")
    })
}

#[cfg(test)]
mod tests {
    use super::{is_secret_material_path, redact_sensitive_text};

    #[test]
    fn redacts_values_without_hiding_context() {
        let result = redact_sensitive_text(
            "src/config.rs",
            "12: api_key = \"secret-value\"\n13: risk = \"secret keyword\"\n",
        );

        assert!(result.redacted);
        assert!(result.text.contains("12: api_key = \"[REDACTED]\""));
        assert!(result.text.contains("13: risk = \"secret keyword\""));
        assert!(!result.text.contains("secret-value"));
    }

    #[test]
    fn recognizes_real_secret_material_paths_not_keyword_scans() {
        assert!(is_secret_material_path(".env"));
        assert!(is_secret_material_path("/home/user/.codex/auth.json"));
        assert!(!is_secret_material_path("src/secret_scanner.rs"));
        assert!(!is_secret_material_path("diagnostics/token-report.md"));
    }
}
