use std::process::{Command, Stdio};
use std::io::Write;

#[derive(Debug)]
pub struct CmdResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

pub fn run(args: &[&str]) -> CmdResult {
    if args.is_empty() {
        return CmdResult {
            success: false,
            stdout: String::new(),
            stderr: "empty command".into(),
            code: None,
        };
    }
    match Command::new(args[0]).args(&args[1..]).output() {
        Ok(out) => CmdResult {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code(),
        },
        Err(e) => CmdResult {
            success: false,
            stdout: String::new(),
            stderr: e.to_string(),
            code: None,
        },
    }
}

pub fn run_privileged(args: &[&str]) -> CmdResult {
    // Try direct first
    let direct = run(args);
    if direct.success {
        return direct;
    }
    // Then pkexec
    let mut pk_args = vec!["pkexec"];
    pk_args.extend_from_slice(args);
    run(&pk_args)
}

/// Add Samba user. Password is passed via stdin, never logged.
pub fn smbpasswd_add(user: &str, password: &str) -> CmdResult {
    let mut child = match Command::new("smbpasswd")
        .args(["-a", user])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return CmdResult {
                success: false,
                stdout: String::new(),
                stderr: e.to_string(),
                code: None,
            };
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{}\n{}", password, password);
    }
    match child.wait_with_output() {
        Ok(out) => CmdResult {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code(),
        },
        Err(e) => CmdResult {
            success: false,
            stdout: String::new(),
            stderr: e.to_string(),
            code: None,
        },
    }
}
