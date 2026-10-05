//! Preferred editor commands are argv, never shell programs.
use anyhow::{bail, Context, Result};
use std::{path::Path, process::Command};

pub fn preference(configured: Option<&str>, visual: Option<&str>, editor: Option<&str>) -> String {
    [configured, visual, editor]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
        .unwrap_or("nano")
        .to_string()
}

pub fn preferred(configured: Option<&str>) -> String {
    preference(
        configured,
        std::env::var("VISUAL").ok().as_deref(),
        std::env::var("EDITOR").ok().as_deref(),
    )
}

/// Small literal argument lexer: whitespace, single/double quotes, backslash.
/// No expansions, pipes, substitutions, globbing, or shell evaluation.
pub fn arguments(command: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut started = false;
    for ch in command.chars() {
        if escaped {
            word.push(ch);
            escaped = false;
        } else if ch == '\\' && quote != Some('\'') {
            escaped = true;
            started = true;
        } else if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                word.push(ch);
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            started = true;
        } else if ch.is_whitespace() {
            if started {
                args.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            word.push(ch);
            started = true;
        }
    }
    if escaped || quote.is_some() {
        bail!("Invalid editor command: unfinished quote or escape");
    }
    if started {
        args.push(word);
    }
    if args.first().is_none_or(|s| s.is_empty()) {
        bail!("Invalid editor command: empty executable");
    }
    Ok(args)
}

pub fn run(args: &[String], path: &Path) -> Result<()> {
    let status = Command::new(&args[0])
        .args(&args[1..])
        .arg(path)
        .status()
        .context("Could not start preferred editor")?;
    if !status.success() {
        bail!("Editor exited unsuccessfully ({status}); active state not reloaded");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_precedence() {
        assert_eq!(
            preference(Some("config --wait"), Some("visual"), Some("editor")),
            "config --wait"
        );
        assert_eq!(preference(None, Some("visual"), Some("editor")), "visual");
        assert_eq!(preference(None, Some(" "), Some("editor")), "editor");
        assert_eq!(preference(None, None, None), "nano");
    }
    #[test]
    fn literal_quoted_arguments() {
        assert_eq!(
            arguments(r#"'/editor with spaces' --wait "two words" '' '$HOME;$(touch nope)'"#)
                .unwrap(),
            vec![
                "/editor with spaces",
                "--wait",
                "two words",
                "",
                "$HOME;$(touch nope)"
            ]
        );
        assert_eq!(arguments(r"editor a\ b").unwrap(), vec!["editor", "a b"]);
        for invalid in ["", "''", "editor '", "editor \\"] {
            assert!(arguments(invalid).is_err());
        }
    }
}
