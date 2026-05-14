use std::cmp::Ordering;

use anyhow::{Result, bail};

/// Normalize user-supplied versions such as `gradle-8.13-bin` into `8.13`.
pub(super) fn normalize_version(input: &str) -> Result<String> {
    let mut version = input.trim();
    if let Some(stripped) = version.strip_prefix("gradle-") {
        version = stripped;
    }
    if let Some(stripped) = version
        .strip_suffix("-bin")
        .or_else(|| version.strip_suffix("-all"))
    {
        version = stripped;
    }
    let version = version.trim().to_owned();

    if version.is_empty() {
        bail!("version cannot be empty");
    }
    if version.contains('/') || version.contains('\\') {
        bail!("version cannot contain path separators");
    }

    Ok(version)
}

/// Make a version string safe to embed in temporary directory names.
pub(super) fn sanitize_version(version: &str) -> String {
    version
        .chars()
        .map(|character| match character {
            '/' | '\\' | ' ' => '-',
            other => other,
        })
        .collect()
}

/// Extract the leading numeric major version from a Gradle version string.
pub(super) fn version_major(version: &str) -> Option<u64> {
    version
        .split(|character: char| !character.is_ascii_alphanumeric())
        .find(|part| !part.is_empty() && part.chars().all(|character| character.is_ascii_digit()))
        .and_then(|part| part.parse::<u64>().ok())
}

/// Compare Gradle versions in a human-friendly order.
pub(super) fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_tokens = tokenize_version(left);
    let right_tokens = tokenize_version(right);
    let shared = left_tokens.len().min(right_tokens.len());

    for index in 0..shared {
        let ordering = left_tokens[index].cmp(&right_tokens[index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    left_tokens.len().cmp(&right_tokens.len())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionToken {
    Number(u64),
    Text(String),
}

impl Ord for VersionToken {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Number(left), Self::Number(right)) => left.cmp(right),
            (Self::Text(left), Self::Text(right)) => left.cmp(right),
            (Self::Number(_), Self::Text(_)) => Ordering::Greater,
            (Self::Text(_), Self::Number(_)) => Ordering::Less,
        }
    }
}

impl PartialOrd for VersionToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn tokenize_version(version: &str) -> Vec<VersionToken> {
    let mut tokens = Vec::new();
    let mut buffer = String::new();
    let mut mode = TokenMode::None;

    for character in version.chars() {
        if character.is_ascii_alphanumeric() {
            let next_mode = if character.is_ascii_digit() {
                TokenMode::Number
            } else {
                TokenMode::Text
            };

            if mode != TokenMode::None && mode != next_mode {
                push_token(&mut tokens, &mut buffer, mode);
            }

            buffer.push(character.to_ascii_lowercase());
            mode = next_mode;
        } else if !buffer.is_empty() {
            push_token(&mut tokens, &mut buffer, mode);
            mode = TokenMode::None;
        }
    }

    if !buffer.is_empty() {
        push_token(&mut tokens, &mut buffer, mode);
    }

    tokens
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TokenMode {
    None,
    Number,
    Text,
}

fn push_token(tokens: &mut Vec<VersionToken>, buffer: &mut String, mode: TokenMode) {
    let value = std::mem::take(buffer);
    match mode {
        TokenMode::Number => {
            let number = value.parse::<u64>().unwrap_or(u64::MAX);
            tokens.push(VersionToken::Number(number));
        }
        TokenMode::Text => tokens.push(VersionToken::Text(value)),
        TokenMode::None => {}
    }
}
