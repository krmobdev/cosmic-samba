use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{env, fs, io};

const SMB_CONF: &str = "/etc/samba/smb.conf";

#[derive(Debug)]
pub struct CmdResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

impl CmdResult {
    fn from_output(out: Output) -> Self {
        Self {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            code: out.status.code(),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self { success: false, stdout: String::new(), stderr: message.into(), code: None }
    }
}

pub fn run(args: &[&str]) -> CmdResult {
    if args.is_empty() { return CmdResult::error("empty command"); }
    match Command::new(args[0]).args(&args[1..]).output() {
        Ok(out) => CmdResult::from_output(out),
        Err(e) => CmdResult::error(e.to_string()),
    }
}

fn is_root() -> bool {
    Command::new("id").arg("-u").output()
        .map(|out| out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "0")
        .unwrap_or(false)
}

/// Privileged commands are run directly only as root; otherwise pkexec is used.
/// A failed command is never retried with elevated privileges, avoiding duplicate actions.
pub fn run_privileged(args: &[&str]) -> CmdResult {
    if args.is_empty() { return CmdResult::error("empty command"); }
    if is_root() { return run(args); }
    let mut command = Command::new("pkexec");
    command.args(args);
    match command.output() {
        Ok(out) => CmdResult::from_output(out),
        Err(e) => CmdResult::error(format!("Не удалось запустить pkexec: {e}")),
    }
}

pub fn save_config_privileged(content: &str) -> Result<(), String> {
    if is_root() {
        return write_config_atomically(Path::new(SMB_CONF), content);
    }
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let mut child = Command::new("pkexec")
        .arg(executable)
        .arg("--admin-write-smb-config")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Не удалось запустить pkexec: {e}"))?;

    let write_error = if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes()).err()
    } else {
        Some(io::Error::new(io::ErrorKind::BrokenPipe, "stdin pkexec недоступен"))
    };
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if let Some(error) = write_error {
        return Err(format!("Не удалось передать конфигурацию: {error}"));
    }
    if output.status.success() { Ok(()) } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn add_samba_user(user: &str, password: &str) -> CmdResult {
    if !valid_system_username(user) { return CmdResult::error("Некорректное имя пользователя"); }
    if password.is_empty() || password.chars().any(|c| c.is_control()) {
        return CmdResult::error("Пароль пуст или содержит управляющие символы");
    }
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(e) => return CmdResult::error(e.to_string()),
    };
    let mut command = if is_root() {
        let mut c = Command::new(executable);
        c.arg("--admin-add-samba-user").arg(user);
        c
    } else {
        let mut c = Command::new("pkexec");
        c.arg(executable).arg("--admin-add-samba-user").arg(user);
        c
    };
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => return CmdResult::error(format!("Не удалось запустить привилегированную операцию: {e}")),
    };
    let write_error = if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{0}\n{0}", password).err()
    } else {
        Some(io::Error::new(io::ErrorKind::BrokenPipe, "stdin helper недоступен"))
    };
    match child.wait_with_output() {
        Ok(output) if write_error.is_none() => CmdResult::from_output(output),
        Ok(_) => CmdResult::error(format!("Не удалось передать пароль: {}", write_error.unwrap())),
        Err(e) => CmdResult::error(e.to_string()),
    }
}

pub fn remove_samba_user(user: &str) -> CmdResult {
    if !valid_system_username(user) { return CmdResult::error("Некорректное имя пользователя"); }
    let executable = match env::current_exe() {
        Ok(path) => path,
        Err(e) => return CmdResult::error(e.to_string()),
    };
    let mut command = if is_root() {
        let mut c = Command::new(executable);
        c.arg("--admin-remove-samba-user").arg(user);
        c
    } else {
        let mut c = Command::new("pkexec");
        c.arg(executable).arg("--admin-remove-samba-user").arg(user);
        c
    };
    match command.output() {
        Ok(output) => CmdResult::from_output(output),
        Err(e) => CmdResult::error(format!("Не удалось запустить привилегированную операцию: {e}")),
    }
}

fn valid_system_username(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 32
        && user.as_bytes()[0].is_ascii_alphabetic()
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

struct ConfigLock(PathBuf);

impl Drop for ConfigLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn acquire_config_lock(parent: &Path) -> Result<ConfigLock, String> {
    let lock_path = parent.join(".cosmic-samba.lock");
    for attempt in 0..2 {
        match fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&lock_path) {
            Ok(mut file) => {
                if let Err(error) = writeln!(file, "{}", std::process::id()) {
                    drop(file);
                    let _ = fs::remove_file(&lock_path);
                    return Err(format!("Не удалось записать блокировку конфигурации: {error}"));
                }
                if let Err(error) = file.sync_all() {
                    drop(file);
                    let _ = fs::remove_file(&lock_path);
                    return Err(format!("Не удалось синхронизировать блокировку: {error}"));
                }
                return Ok(ConfigLock(lock_path));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempt == 0 => {
                let pid_text = fs::read_to_string(&lock_path)
                    .map_err(|e| format!("Конфигурация уже заблокирована; не удалось проверить блокировку: {e}"))?;
                let pid: u32 = pid_text.trim().parse()
                    .map_err(|_| "Конфигурация заблокирована неизвестным процессом; удалите /etc/samba/.cosmic-samba.lock после проверки".to_string())?;
                let alive = Command::new("kill").args(["-0", &pid.to_string()]).status()
                    .map(|status| status.success()).unwrap_or(true);
                if alive {
                    return Err("Другая операция уже изменяет smb.conf. Повторите позже.".into());
                }
                fs::remove_file(&lock_path)
                    .map_err(|e| format!("Не удалось удалить устаревшую блокировку: {e}"))?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err("Другая операция уже изменяет smb.conf. Повторите позже.".into());
            }
            Err(error) => return Err(format!("Не удалось заблокировать smb.conf: {error}")),
        }
    }
    Err("Не удалось получить блокировку smb.conf".into())
}

fn write_config_atomically(path: &Path, content: &str) -> Result<(), String> {
    if path != Path::new(SMB_CONF) {
        return Err("Запись разрешена только в /etc/samba/smb.conf".into());
    }
    let parent = path.parent().ok_or("У конфигурации отсутствует родительский каталог")?;
    if !parent.is_dir() { return Err("Каталог /etc/samba не существует".into()); }
    let _lock = acquire_config_lock(parent)?;

    let original = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Отказ от изменения smb.conf: файл является символической ссылкой".into());
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err("Отказ от изменения smb.conf: путь не является обычным файлом".into());
        }
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Не удалось прочитать метаданные конфигурации: {error}")),
    };

    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?.as_nanos();
    let temp_path = parent.join(format!(".smb.conf.cosmic-samba.{}.{}", std::process::id(), nonce));
    let backup_path = PathBuf::from(format!("{SMB_CONF}.bak"));

    let write_result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true).create_new(true).mode(0o600).open(&temp_path)
            .map_err(|e| format!("Не удалось создать временный файл: {e}"))?;
        file.write_all(content.as_bytes()).map_err(|e| format!("Не удалось записать конфигурацию: {e}"))?;

        if let Some(metadata) = &original {
            fs::set_permissions(&temp_path, fs::Permissions::from_mode(metadata.permissions().mode() & 0o777))
                .map_err(|e| format!("Не удалось сохранить права доступа: {e}"))?;
            let owner = Command::new("chown")
                .arg(format!("--reference={}", path.display())).arg(&temp_path).output()
                .map_err(|e| format!("Не удалось сохранить владельца файла: {e}"))?;
            if !owner.status.success() {
                return Err(format!("Не удалось сохранить владельца файла: {}", String::from_utf8_lossy(&owner.stderr).trim()));
            }
        } else {
            fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o644))
                .map_err(|e| format!("Не удалось установить права доступа: {e}"))?;
            let owner = Command::new("chown").arg("root:root").arg(&temp_path).output()
                .map_err(|e| format!("Не удалось назначить владельца файла: {e}"))?;
            if !owner.status.success() {
                return Err(format!("Не удалось назначить владельца файла: {}", String::from_utf8_lossy(&owner.stderr).trim()));
            }
        }
        file.sync_all().map_err(|e| format!("Не удалось синхронизировать временный файл: {e}"))?;
        drop(file);

        let test = Command::new("testparm").arg("-s").arg(&temp_path).output()
            .map_err(|e| format!("Не удалось запустить testparm; конфигурация не записана: {e}"))?;
        if !test.status.success() {
            return Err(format!("testparm отклонил конфигурацию: {}", String::from_utf8_lossy(&test.stderr).trim()));
        }

        if path.exists() {
            if let Ok(backup_metadata) = fs::symlink_metadata(&backup_path) {
                if backup_metadata.file_type().is_symlink() || !backup_metadata.is_file() {
                    return Err("Отказ от создания резервной копии: smb.conf.bak не является обычным файлом".into());
                }
            }
            fs::copy(path, &backup_path).map_err(|e| format!("Не удалось создать резервную копию: {e}"))?;
            if let Some(metadata) = &original {
                fs::set_permissions(&backup_path, fs::Permissions::from_mode(metadata.permissions().mode() & 0o777))
                    .map_err(|e| format!("Не удалось установить права резервной копии: {e}"))?;
                let owner = Command::new("chown")
                    .arg(format!("--reference={}", path.display()))
                    .arg(&backup_path)
                    .output()
                    .map_err(|e| format!("Не удалось сохранить владельца резервной копии: {e}"))?;
                if !owner.status.success() {
                    return Err(format!("Не удалось сохранить владельца резервной копии: {}", String::from_utf8_lossy(&owner.stderr).trim()));
                }
            }
            fs::File::open(&backup_path).and_then(|f| f.sync_all())
                .map_err(|e| format!("Не удалось синхронизировать резервную копию: {e}"))?;
        }

        fs::rename(&temp_path, path).map_err(|e| format!("Не удалось атомарно заменить конфигурацию: {e}"))?;
        fs::File::open(parent).and_then(|f| f.sync_all())
            .map_err(|e| format!("Конфигурация заменена, но не удалось синхронизировать каталог: {e}"))?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

/// Handles private pkexec entry points before the graphical application starts.
pub fn maybe_handle_admin_command() -> Option<i32> {
    let mut args = env::args().skip(1);
    let operation = args.next()?;
    let result = match operation.as_str() {
        "--admin-write-smb-config" => {
            if !is_root() { eprintln!("Операция требует root"); return Some(1); }
            let mut content = String::new();
            if let Err(error) = io::stdin().read_to_string(&mut content) {
                eprintln!("Не удалось прочитать конфигурацию: {error}");
                return Some(1);
            }
            write_config_atomically(Path::new(SMB_CONF), &content)
        }
        "--admin-add-samba-user" => {
            if !is_root() { eprintln!("Операция требует root"); return Some(1); }
            let Some(user) = args.next() else { eprintln!("Не указано имя пользователя"); return Some(1); };
            if !valid_system_username(&user) { eprintln!("Некорректное имя пользователя"); return Some(1); }
            let mut password_input = String::new();
            if let Err(error) = io::stdin().read_to_string(&mut password_input) {
                eprintln!("Не удалось прочитать пароль: {error}");
                return Some(1);
            }
            let password = password_input.lines().next().unwrap_or_default();
            if password.is_empty() || password_input.lines().nth(1) != Some(password) {
                eprintln!("Пароль пуст или подтверждения не совпадают");
                return Some(1);
            }
            admin_add_user(&user, password)
        }
        "--admin-remove-samba-user" => {
            if !is_root() { eprintln!("Операция требует root"); return Some(1); }
            let Some(user) = args.next() else { eprintln!("Не указано имя пользователя"); return Some(1); };
            if !valid_system_username(&user) { eprintln!("Некорректное имя пользователя"); return Some(1); }
            let out = Command::new("smbpasswd").args(["-x", &user]).output();
            match out {
                Ok(output) if output.status.success() => Ok(()),
                Ok(output) => Err(format!("smbpasswd завершился с ошибкой: {}", String::from_utf8_lossy(&output.stderr).trim())),
                Err(error) => Err(error.to_string()),
            }
        }
        _ => return None,
    };
    match result {
        Ok(()) => Some(0),
        Err(error) => { eprintln!("{error}"); Some(1) }
    }
}

fn admin_add_user(user: &str, password: &str) -> Result<(), String> {
    let exists = Command::new("id").arg("-u").arg(user).output()
        .map(|out| out.status.success()).unwrap_or(false);
    let created = if exists {
        false
    } else {
        let output = Command::new("useradd")
            .args(["-M", "-s", "/usr/sbin/nologin", "--", user])
            .output().map_err(|e| format!("Не удалось запустить useradd: {e}"))?;
        if !output.status.success() {
            return Err(format!("useradd завершился с ошибкой: {}", String::from_utf8_lossy(&output.stderr).trim()));
        }
        true
    };

    let mut child = match Command::new("smbpasswd")
        .args(["-s", "-a", user])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn() {
            Ok(child) => child,
            Err(error) => {
                if created { let _ = Command::new("userdel").arg(user).output(); }
                return Err(format!("Не удалось запустить smbpasswd: {error}"));
            }
        };
    let write_error = if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{0}\n{0}", password).err()
    } else {
        Some(io::Error::new(io::ErrorKind::BrokenPipe, "stdin smbpasswd недоступен"))
    };
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if let Some(error) = write_error {
        if created { let _ = Command::new("userdel").arg(user).output(); }
        return Err(format!("Не удалось передать пароль в smbpasswd: {error}"));
    }
    if !output.status.success() {
        if created { let _ = Command::new("userdel").arg(user).output(); }
        return Err(format!("smbpasswd завершился с ошибкой: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(())
}
