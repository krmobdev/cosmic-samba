use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub name: String,
    pub path: String,
    pub comment: String,
    pub read_only: bool,
    pub guest_ok: bool,
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct RawSection {
    name: String,
    lines: Vec<String>,
    is_share: bool,
}

#[derive(Debug, Clone)]
pub struct SambaConfig {
    preamble: Vec<String>,
    sections: Vec<RawSection>,
    pub shares: Vec<Share>,
}

impl SambaConfig {
    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(content) => Self::parse(&content)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Ok(Self { preamble: Vec::new(), sections: Vec::new(), shares: Vec::new() })
            }
            Err(e) => Err(e),
        }
    }

    pub fn parse(content: &str) -> Result<Self, String> {
        let mut preamble = Vec::new();
        let mut sections: Vec<RawSection> = Vec::new();
        let mut current: Option<RawSection> = None;
        let mut names = HashSet::new();

        for (line_no, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                if !(trimmed.ends_with(']') && trimmed.len() >= 3) {
                    return Err(format!("Некорректный заголовок секции в строке {}", line_no + 1));
                }
                if let Some(section) = current.take() {
                    sections.push(section);
                }
                let name = trimmed[1..trimmed.len() - 1].trim().to_string();
                if name.is_empty() || !names.insert(name.to_ascii_lowercase()) {
                    return Err(format!("Пустая или повторяющаяся секция в строке {}", line_no + 1));
                }
                let reserved = ["global", "homes", "printers", "print$", "ipc$", "profiles"];
                let is_share = !reserved.iter().any(|reserved| name.eq_ignore_ascii_case(reserved));
                current = Some(RawSection { is_share, name, lines: vec![line.to_string()] });
            } else if let Some(section) = current.as_mut() {
                if let Some((key, _)) = assignment(trimmed) {
                    if key.eq_ignore_ascii_case("include") || key.eq_ignore_ascii_case("config file") {
                        return Err(format!("Конфигурация содержит include/config file (строка {}). Сначала объедините включаемые файлы вручную.", line_no + 1));
                    }
                }
                section.lines.push(line.to_string());
            } else {
                if let Some((key, _)) = assignment(trimmed) {
                    if key.eq_ignore_ascii_case("include") || key.eq_ignore_ascii_case("config file") {
                        return Err(format!("Конфигурация содержит include/config file (строка {}). Сначала объедините включаемые файлы вручную.", line_no + 1));
                    }
                }
                preamble.push(line.to_string());
            }
        }
        if let Some(section) = current {
            sections.push(section);
        }

        let mut shares = Vec::new();
        for section in &sections {
            if section.is_share {
                if let Some(share) = parse_share(section)? {
                    shares.push(share);
                }
            }
        }
        Ok(Self { preamble, sections, shares })
    }

    /// Rewrites only settings managed by the UI. All other original lines,
    /// comments, global settings and non-share sections remain verbatim.
    /// Existing path-bearing shares omitted from desired are intentionally removed.
    pub fn render_with_shares(&self, desired: &[Share]) -> Result<String, String> {
        let mut desired_by_name: BTreeMap<String, Share> = BTreeMap::new();
        for share in desired {
            if !crate::validation::is_valid_share_name(&share.name) {
                return Err(format!("Некорректное имя шары: {}", share.name));
            }
            if !crate::validation::is_valid_path(&share.path) {
                return Err(format!("Некорректный путь для шары {}", share.name));
            }
            if share.comment.chars().any(|c| c.is_control()) {
                return Err(format!("Комментарий шары {} содержит управляющие символы", share.name));
            }
            let key = share.name.to_ascii_lowercase();
            if desired_by_name.insert(key.clone(), share.clone()).is_some() {
                return Err(format!("Повторяющееся имя шары: {}", share.name));
            }
            if self.sections.iter().any(|section| {
                section.name.eq_ignore_ascii_case(&share.name)
                    && parse_share(section).map(|parsed| parsed.is_none()).unwrap_or(true)
            }) {
                return Err(format!("Имя шары {} конфликтует с существующей специальной секцией", share.name));
            }
        }

        let mut output = String::new();
        for line in &self.preamble {
            output.push_str(line);
            output.push('\n');
        }
        if !self.sections.iter().any(|section| !section.is_share) {
            output.push_str("[global]\n\n");
        }

        for section in &self.sections {
            if !section.is_share {
                append_lines(&mut output, &section.lines);
                continue;
            }
            if let Some(original) = parse_share(section)? {
                let key = original.name.to_ascii_lowercase();
                if let Some(updated) = desired_by_name.remove(&key) {
                    let patched = patch_share(&section.lines, &updated)?;
                    append_lines(&mut output, &patched);
                }
                // Path-bearing shares omitted by the UI are removed.
            } else {
                // Preserve special sections such as [printers] without a path.
                append_lines(&mut output, &section.lines);
            }
        }

        for share in desired_by_name.into_values() {
            if !output.is_empty() && !output.ends_with("\n\n") {
                output.push('\n');
            }
            output.push_str(&format!("[{}]\n", share.name));
            output.push_str(&format!("path = {}\n", quote_value(&share.path)));
            output.push_str("browseable = yes\n");
            output.push_str(&format!("read only = {}\n", if share.read_only { "yes" } else { "no" }));
            output.push_str(&format!("guest ok = {}\n", if share.guest_ok { "yes" } else { "no" }));
            if !share.comment.is_empty() {
                output.push_str(&format!("comment = {}\n", quote_value(&share.comment)));
            }
            output.push('\n');
        }
        if !output.ends_with('\n') { output.push('\n'); }
        Ok(output)
    }
}

fn append_lines(output: &mut String, lines: &[String]) {
    for line in lines { output.push_str(line); output.push('\n'); }
}

fn assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') { return None; }
    line.split_once('=').map(|(key, value)| (key.trim(), value.trim()))
}

fn parse_share(section: &RawSection) -> Result<Option<Share>, String> {
    let mut values: HashMap<String, String> = HashMap::new();
    for (offset, line) in section.lines.iter().enumerate().skip(1) {
        if let Some((key, value)) = assignment(line) {
            let key = key.to_ascii_lowercase();
            let canonical = match key.as_str() {
                "path" | "directory" => "path",
                "comment" => "comment",
                "read only" | "writable" | "writeable" => "read only",
                "guest ok" | "public" => "guest ok",
                _ => continue,
            };
            let normalized_value = match key.as_str() {
                "writable" | "writeable" => {
                    if parse_bool(value)? { "no" } else { "yes" }.to_string()
                }
                _ => value.to_string(),
            };
            if values.insert(canonical.to_string(), normalized_value).is_some() {
                return Err(format!("Повторяющийся параметр '{}' или его псевдоним в секции [{}], строка внутри секции {}", canonical, section.name, offset + 1));
            }
        }
    }
    let Some(path) = values.get("path") else { return Ok(None) };
    Ok(Some(Share {
        name: section.name.clone(),
        path: unquote(path),
        comment: values.get("comment").map(|v| unquote(v)).unwrap_or_default(),
        read_only: values.get("read only").map(|v| parse_bool(v)).transpose()?.unwrap_or(true),
        guest_ok: values.get("guest ok").map(|v| parse_bool(v)).transpose()?.unwrap_or(false),
        extra: BTreeMap::new(),
    }))
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match unquote(value).to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" => Ok(true),
        "no" | "false" | "0" => Ok(false),
        other => Err(format!("Некорректное логическое значение Samba: {other}")),
    }
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1].to_string()
    } else { value.to_string() }
}

fn quote_value(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn patch_share(lines: &[String], share: &Share) -> Result<Vec<String>, String> {
    let mut result = Vec::with_capacity(lines.len() + 4);
    let mut seen = HashSet::new();
    for (index, line) in lines.iter().enumerate() {
        if index == 0 { result.push(line.clone()); continue; }
        let Some((key, _)) = assignment(line) else { result.push(line.clone()); continue; };
        let normalized = key.to_ascii_lowercase();
        let (canonical, value) = match normalized.as_str() {
            "path" | "directory" => ("path", quote_value(&share.path)),
            "comment" => ("comment", quote_value(&share.comment)),
            "read only" | "writable" | "writeable" => ("read only", if share.read_only { "yes" } else { "no" }.to_string()),
            "guest ok" | "public" => ("guest ok", if share.guest_ok { "yes" } else { "no" }.to_string()),
            _ => { result.push(line.clone()); continue; }
        };
        if !seen.insert(canonical.to_string()) {
            return Err(format!("Повторяющийся параметр '{}' или его псевдоним в секции [{}]", canonical, share.name));
        }
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        result.push(format!("{}{} = {}", indent, canonical, value));
    }
    for (key, value) in [
        ("path", quote_value(&share.path)),
        ("read only", if share.read_only { "yes".to_string() } else { "no".to_string() }),
        ("guest ok", if share.guest_ok { "yes".to_string() } else { "no".to_string() }),
    ] {
        if !seen.contains(key) { result.push(format!("{} = {}", key, value)); }
    }
    if !share.comment.is_empty() && !seen.contains("comment") {
        result.push(format!("comment = {}", quote_value(&share.comment)));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Keep this header\n\n[global]\nworkgroup = MYGROUP\nserver string = Old\n\n# External section\n[other]\npath = \"/tmp/other\"\nfoo = bar\n# keep this comment\n\n[printers]\nprintable = yes\n";

    #[test]
    fn preserves_globals_comments_unknown_settings_and_special_sections() {
        let cfg = SambaConfig::parse(SAMPLE).unwrap();
        let rendered = cfg.render_with_shares(&cfg.shares).unwrap();
        assert!(rendered.contains("workgroup = MYGROUP"));
        assert!(rendered.contains("server string = Old"));
        assert!(rendered.contains("foo = bar"));
        assert!(rendered.contains("# keep this comment"));
        assert!(rendered.contains("[printers]"));
        assert!(rendered.contains("printable = yes"));
        assert!(rendered.contains("# Keep this header"));
    }

    #[test]
    fn updates_only_managed_values_and_keeps_unknown_lines() {
        let cfg = SambaConfig::parse("[global]\nworkgroup = WORKGROUP\n\n[data]\npath = /old\nread only = yes\ncustom = keep\n# comment survives\n").unwrap();
        let mut desired = cfg.shares.clone();
        desired[0].path = "/new folder".into();
        desired[0].read_only = false;
        let rendered = cfg.render_with_shares(&desired).unwrap();
        assert!(rendered.contains("path = \"/new folder\""));
        assert!(rendered.contains("read only = no"));
        assert!(rendered.contains("custom = keep"));
        assert!(rendered.contains("# comment survives"));
        assert!(rendered.contains("workgroup = WORKGROUP"));
    }

    #[test]
    fn removes_a_share_removed_from_desired_list() {
        let cfg = SambaConfig::parse("[global]\nworkgroup = WORKGROUP\n[a]\npath = /a\n[b]\npath = /b\n").unwrap();
        let rendered = cfg.render_with_shares(&cfg.shares[..1]).unwrap();
        assert!(rendered.contains("[a]"));
        assert!(!rendered.contains("[b]"));
    }

    #[test]
    fn adds_new_share_without_rewriting_existing_sections() {
        let cfg = SambaConfig::parse("[global]\nworkgroup = WORKGROUP\n").unwrap();
        let share = Share { name: "new".into(), path: "/srv/new".into(), comment: String::new(), read_only: false, guest_ok: false, extra: BTreeMap::new() };
        let rendered = cfg.render_with_shares(&[share]).unwrap();
        assert!(rendered.contains("[global]\nworkgroup = WORKGROUP"));
        assert!(rendered.contains("[new]"));
    }

    #[test]
    fn refuses_ambiguous_or_external_include_configuration() {
        assert!(SambaConfig::parse("[global]\ninclude = /etc/samba/extra.conf\n").is_err());
        assert!(SambaConfig::parse("[a]\npath = /a\n[a]\npath = /b\n").is_err());
        assert!(SambaConfig::parse("[broken\npath = /x\n").is_err());
    }

    #[test]
    fn preserves_reserved_samba_sections_even_when_they_have_paths() {
        let cfg = SambaConfig::parse("[global]\nworkgroup = WORKGROUP\n[homes]\npath = /home/%S\nread only = yes\n[printers]\npath = /var/spool/samba\nprintable = yes\n[print$]\npath = /var/lib/samba/printers\n").unwrap();
        assert!(cfg.shares.is_empty());
        let rendered = cfg.render_with_shares(&[]).unwrap();
        assert!(rendered.contains("[homes]"));
        assert!(rendered.contains("path = /home/%S"));
        assert!(rendered.contains("[printers]"));
        assert!(rendered.contains("[print$]"));
    }

    #[test]
    fn rejects_invalid_managed_boolean_values() {
        assert!(SambaConfig::parse("[data]\npath = /srv/data\nread only = maybe\n").is_err());
        assert!(SambaConfig::parse("[data]\npath = /srv/data\nguest ok = perhaps\n").is_err());
    }

    #[test]
    fn handles_samba_aliases_without_leaving_conflicting_settings() {
        let cfg = SambaConfig::parse("[data]\ndirectory = /srv/data\nwritable = yes\npublic = yes\n").unwrap();
        assert_eq!(cfg.shares[0].path, "/srv/data");
        assert!(!cfg.shares[0].read_only);
        assert!(cfg.shares[0].guest_ok);
        let mut desired = cfg.shares.clone();
        desired[0].read_only = true;
        desired[0].guest_ok = false;
        let rendered = cfg.render_with_shares(&desired).unwrap();
        assert!(rendered.contains("path = \"/srv/data\""));
        assert!(rendered.contains("read only = yes"));
        assert!(rendered.contains("guest ok = no"));
        assert!(!rendered.contains("writable ="));
        assert!(!rendered.contains("public ="));
    }

    #[test]
    fn rejects_conflicting_samba_aliases() {
        assert!(SambaConfig::parse("[data]\npath = /srv/data\ndirectory = /other\n").is_err());
        assert!(SambaConfig::parse("[data]\npath = /srv/data\nread only = yes\nwritable = yes\n").is_err());
    }

    #[test]
    fn rejects_duplicate_managed_keys() {
        assert!(SambaConfig::parse("[a]\npath = /a\npath = /b\n").is_err());
    }
}
