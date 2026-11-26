use std::path::Path;

pub fn is_valid_share_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 80 {
        return false;
    }
    // Disallow control chars, brackets, newlines, path separators
    name.chars().all(|c| {
        c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' '
    }) && !name.contains(['[', ']', '\n', '\r', '/', '\\'])
}

pub fn is_valid_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\n') || path.contains('\r') {
        return false;
    }
    let p = Path::new(path);
    p.is_absolute() && !path.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_name_validation() {
        assert!(is_valid_share_name("shared"));
        assert!(is_valid_share_name("my-share"));
        assert!(!is_valid_share_name("bad\nname"));
        assert!(!is_valid_share_name("[hack]"));
        assert!(!is_valid_share_name(""));
    }

    #[test]
    fn path_validation() {
        assert!(is_valid_path("/home/user/shared"));
        assert!(!is_valid_path("relative"));
        assert!(!is_valid_path("/tmp/../etc/passwd"));
        assert!(!is_valid_path("/tmp\ninject"));
    }
}
