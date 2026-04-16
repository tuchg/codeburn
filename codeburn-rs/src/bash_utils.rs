use std::path::Path;

/// Strips quoted strings from a command, replacing them with spaces to preserve positions.
fn strip_quoted_strings(command: &str) -> String {
    let mut result = String::with_capacity(command.len());
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            let quote = ch;
            result.push(' ');
            i += 1;
            while i < chars.len() && chars[i] != quote {
                result.push(' ');
                i += 1;
            }
            if i < chars.len() {
                result.push(' ');
                i += 1;
            }
        } else {
            result.push(ch);
            i += 1;
        }
    }

    result
}

/// Extracts bash command names from a shell command string.
/// Handles chained commands (&&, ;), pipes (|), full paths, and filters out `cd`.
pub fn extract_bash_commands(command: &str) -> Vec<String> {
    if command.trim().is_empty() {
        return Vec::new();
    }

    let stripped = strip_quoted_strings(command);

    // Find separator positions in the stripped (quote-removed) version
    let separator_re = regex::Regex::new(r"\s*(?:&&|;|\|)\s*").unwrap();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut cursor = 0;

    for mat in separator_re.find_iter(&stripped) {
        ranges.push((cursor, mat.start()));
        cursor = mat.end();
    }
    ranges.push((cursor, command.len()));

    let mut commands = Vec::new();
    for (start, end) in ranges {
        let segment = command[start..end].trim();
        if segment.is_empty() {
            continue;
        }

        let first_token = segment.split_whitespace().next().unwrap_or("");
        let base = Path::new(first_token)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        if !base.is_empty() && base != "cd" {
            commands.push(base);
        }
    }

    commands
}

/// Set of tool names that represent bash execution
pub const BASH_TOOL_NAMES: &[&str] = &["Bash", "BashTool", "PowerShellTool"];

pub fn is_bash_tool(name: &str) -> bool {
    BASH_TOOL_NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_command() {
        assert_eq!(extract_bash_commands("git status"), vec!["git"]);
    }

    #[test]
    fn extracts_chained_commands_with_and() {
        assert_eq!(
            extract_bash_commands("git add . && git commit -m \"x\""),
            vec!["git", "git"]
        );
    }

    #[test]
    fn extracts_chained_commands_with_semicolon() {
        assert_eq!(extract_bash_commands("ls; pwd"), vec!["ls", "pwd"]);
    }

    #[test]
    fn extracts_piped_commands() {
        assert_eq!(
            extract_bash_commands("cat file | grep pattern"),
            vec!["cat", "grep"]
        );
    }

    #[test]
    fn filters_out_cd() {
        assert_eq!(
            extract_bash_commands("cd /path && git status"),
            vec!["git"]
        );
    }

    #[test]
    fn returns_empty_for_cd_only() {
        assert_eq!(extract_bash_commands("cd /path"), Vec::<String>::new());
    }

    #[test]
    fn returns_empty_for_empty_string() {
        assert_eq!(extract_bash_commands(""), Vec::<String>::new());
    }

    #[test]
    fn returns_empty_for_whitespace_only() {
        assert_eq!(extract_bash_commands("   "), Vec::<String>::new());
    }

    #[test]
    fn extracts_basename_from_full_path() {
        assert_eq!(extract_bash_commands("/usr/bin/git status"), vec!["git"]);
    }

    #[test]
    fn handles_mixed_separators() {
        assert_eq!(
            extract_bash_commands("cd /x && npm install; npm run build | tee log"),
            vec!["npm", "npm", "tee"]
        );
    }

    #[test]
    fn handles_extra_whitespace() {
        assert_eq!(extract_bash_commands("  git   status  "), vec!["git"]);
    }

    #[test]
    fn handles_quotes_containing_separators() {
        assert_eq!(
            extract_bash_commands("echo \"hello && world\""),
            vec!["echo"]
        );
    }

    #[test]
    fn handles_quoted_separators_followed_by_real() {
        assert_eq!(
            extract_bash_commands("echo \"hello && world\" && git status"),
            vec!["echo", "git"]
        );
    }

    #[test]
    fn handles_single_quoted_separators() {
        assert_eq!(
            extract_bash_commands("echo 'hello && world'"),
            vec!["echo"]
        );
    }
}
