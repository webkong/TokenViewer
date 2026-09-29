use std::path::{Component, Path, PathBuf};

/// Resolve persistent paths without evaluating shell expressions.
pub fn absolute_path(raw: &str, home: &Path) -> Result<PathBuf, String> {
    let invalid = |s: &str| s.chars().any(|c| c.is_control() || c == '$' || c == '~');
    let mut value = raw.to_owned();
    for prefix in ["~", "$HOME", "${HOME}"] {
        if raw == prefix || raw.starts_with(&format!("{prefix}/")) {
            let home_text = home.to_str().ok_or("Invalid home directory")?;
            if !home.is_absolute() || invalid(home_text) {
                return Err("Home directory must be absolute and expanded".into());
            }
            value = format!(
                "{}{}",
                home_text.trim_end_matches('/'),
                &raw[prefix.len()..]
            );
            if value.is_empty() {
                value = "/".into();
            }
            break;
        }
    }
    let path = Path::new(&value);
    if invalid(&value) || !path.is_absolute() {
        return Err(
            "Path must be absolute with no unresolved variables or control characters".into(),
        );
    }
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            _ => result.push(part.as_os_str()),
        }
    }
    Ok(result)
}

/// Keep this explicit list in parity with Swift SkillPathPolicy.
pub fn is_directory_variable(name: &str) -> bool {
    matches!(
        name,
        "HOME"
            | "AGENTS_HOME"
            | "CODEX_HOME"
            | "ORCA_CODEX_HOME"
            | "XDG_CONFIG_HOME"
            | "XDG_CACHE_HOME"
            | "XDG_DATA_HOME"
            | "XDG_STATE_HOME"
            | "XDG_RUNTIME_DIR"
            | "SELF_IMPROVEMENT_HOME"
            | "SELF_IMPROVEMENT_PROJECT_ROOT"
            | "TOKENVIEWER_SKILLS_ROOT"
    )
}

pub fn environment_value(name: &str, value: &str, home: &Path) -> Result<String, String> {
    if value.contains('\0') {
        return Err("Environment value contains NUL".into());
    }
    if !value.is_empty() && is_directory_variable(name) {
        return absolute_path(value, home).map(|path| path.to_string_lossy().into_owned());
    }
    Ok(value.to_owned())
}
