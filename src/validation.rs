use std::path::{Component, Path};

pub fn is_valid_share_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 80 || name.trim() != name {
        return false;
    }
    if name.eq_ignore_ascii_case("global") {
        return false;
    }
    name.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' '
    }) && name != "." && name != ".."
}

pub fn is_valid_path(path: &str) -> bool {
    if path.is_empty() || path.trim() != path || path.chars().any(|c| c.is_control() || c == '"' || c == '\\') {
        return false;
    }
    let p = Path::new(path);
    p.is_absolute() && p.components().all(|component| !matches!(component, Component::ParentDir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_name_validation() {
        assert!(is_valid_share_name("shared"));
        assert!(is_valid_share_name("my-share"));
        assert!(is_valid_share_name("my share"));
        assert!(!is_valid_share_name("bad\nname"));
        assert!(!is_valid_share_name("[hack]"));
        assert!(!is_valid_share_name(""));
        assert!(!is_valid_share_name(" global "));
        assert!(!is_valid_share_name("GLOBAL"));
    }

    #[test]
    fn path_validation() {
        assert!(is_valid_path("/home/user/shared"));
        assert!(is_valid_path("/home/user/my..folder"));
        assert!(!is_valid_path("relative"));
        assert!(!is_valid_path("/tmp/../etc/passwd"));
        assert!(!is_valid_path("/tmp\ninject"));
        assert!(!is_valid_path("/tmp/quoted\"path"));
    }
}
