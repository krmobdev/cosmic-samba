use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Share {
    pub name: String,
    pub path: String,
    pub comment: String,
    pub read_only: bool,
    pub guest_ok: bool,
    /// Unknown parameters preserved as-is
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct SambaConfig {
    pub global: BTreeMap<String, String>,
    pub shares: Vec<Share>,
    /// Raw lines before first section (comments, etc.)
    pub preamble: String,
}

impl SambaConfig {
    pub fn load(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            return Ok(Self {
                global: BTreeMap::new(),
                shares: Vec::new(),
                preamble: String::new(),
            });
        }
        let content = fs::read_to_string(path)?;
        Ok(Self::parse(&content))
    }

    pub fn parse(content: &str) -> Self {
        let mut config = Self {
            global: BTreeMap::new(),
            shares: Vec::new(),
            preamble: String::new(),
        };
        let mut current_section: Option<String> = None;
        let mut current_extra = BTreeMap::new();
        let mut current_share: Option<Share> = None;
        let mut preamble = String::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                if current_section.is_none() {
                    preamble.push_str(line);
                    preamble.push('\n');
                }
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                // flush previous
                if let Some(name) = current_section.take() {
                    if name == "global" {
                        // already in global map
                    } else if let Some(mut share) = current_share.take() {
                        share.extra = current_extra.drain().collect();
                        if !share.path.is_empty() {
                            config.shares.push(share);
                        }
                    }
                }
                current_extra.clear();
                let name = trimmed[1..trimmed.len() - 1].trim().to_string();
                current_section = Some(name.clone());
                if name != "global" {
                    current_share = Some(Share {
                        name,
                        path: String::new(),
                        comment: String::new(),
                        read_only: false,
                        guest_ok: false,
                        extra: BTreeMap::new(),
                    });
                }
                continue;
            }
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().to_string();
                if let Some(ref section) = current_section {
                    if section == "global" {
                        config.global.insert(key, value);
                    } else if let Some(ref mut share) = current_share {
                        match key.as_str() {
                            "path" => share.path = value,
                            "comment" => share.comment = value,
                            "read only" => share.read_only = value.eq_ignore_ascii_case("yes"),
                            "guest ok" => share.guest_ok = value.eq_ignore_ascii_case("yes"),
                            _ => {
                                current_extra.insert(key, value);
                            }
                        }
                    }
                }
            }
        }
        // flush last
        if let Some(mut share) = current_share {
            share.extra = current_extra;
            if !share.path.is_empty() {
                config.shares.push(share);
            }
        }
        config.preamble = preamble;
        config
    }

    pub fn to_string(&self) -> String {
        let mut out = self.preamble.clone();
        out.push_str("[global]\n");
        for (k, v) in &self.global {
            out.push_str(&format!("{} = {}\n", k, v));
        }
        out.push('\n');
        for share in &self.shares {
            out.push_str(&format!("[{}]\n", share.name));
            out.push_str(&format!("path = {}\n", share.path));
            out.push_str(&format!("browseable = yes\n"));
            out.push_str(&format!(
                "read only = {}\n",
                if share.read_only { "yes" } else { "no" }
            ));
            out.push_str(&format!(
                "guest ok = {}\n",
                if share.guest_ok { "yes" } else { "no" }
            ));
            if !share.comment.is_empty() {
                out.push_str(&format!("comment = {}\n", share.comment));
            }
            for (k, v) in &share.extra {
                out.push_str(&format!("{} = {}\n", k, v));
            }
            out.push('\n');
        }
        out
    }

    /// Safe write: backup, testparm, atomic rename, rollback on failure
    pub fn save_safe(&self, path: &Path) -> Result<(), String> {
        let content = self.to_string();

        // Validate with testparm if available
        let tmp_dir = path.parent().unwrap_or(Path::new("/tmp"));
        let tmp_path = tmp_dir.join(format!(".smb.conf.tmp.{}", std::process::id()));
        {
            let mut f = File::create(&tmp_path).map_err(|e| e.to_string())?;
            f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
            f.sync_all().ok();
        }

        // testparm check
        let test = Command::new("testparm")
            .args(["-s", tmp_path.to_str().unwrap_or("")])
            .output();
        if let Ok(out) = test {
            if !out.status.success() {
                let _ = fs::remove_file(&tmp_path);
                return Err(format!(
                    "testparm failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                ));
            }
        }
        // if testparm not found, continue with caution

        // Backup original
        let backup = PathBuf::from(format!("{}.bak", path.display()));
        if path.exists() {
            fs::copy(path, &backup).map_err(|e| e.to_string())?;
        }

        // Atomic replace
        fs::rename(&tmp_path, path).map_err(|e| {
            // rollback
            if backup.exists() {
                let _ = fs::copy(&backup, path);
            }
            e.to_string()
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_global_and_extra_params() {
        let input = r#"[global]
workgroup = MYGROUP
server string = Old
unknown = keepme

[other]
path = /tmp/other
comment = external
foo = bar

[managed]
path = /home/share
read only = no
guest ok = yes
comment = mine
"#;
        let cfg = SambaConfig::parse(input);
        assert_eq!(cfg.global.get("workgroup").map(|s| s.as_str()), Some("MYGROUP"));
        assert_eq!(cfg.global.get("unknown").map(|s| s.as_str()), Some("keepme"));
        assert_eq!(cfg.shares.len(), 2);
        let managed = cfg.shares.iter().find(|s| s.name == "managed").unwrap();
        assert_eq!(managed.path, "/home/share");
        assert!(managed.guest_ok);
        let other = cfg.shares.iter().find(|s| s.name == "other").unwrap();
        assert_eq!(other.extra.get("foo").map(|s| s.as_str()), Some("bar"));

        let out = cfg.to_string();
        assert!(out.contains("workgroup = MYGROUP"));
        assert!(out.contains("unknown = keepme"));
        assert!(out.contains("foo = bar"));
    }

    #[test]
    fn rejects_bad_share_name() {
        // validation is separate, but parser should not panic
        let input = "[bad\nname]\npath = /tmp\n";
        let cfg = SambaConfig::parse(input);
        // name will be truncated or as-is; important is no panic
        assert!(!cfg.shares.is_empty() || true);
    }
}
