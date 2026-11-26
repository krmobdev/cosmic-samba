mod config;
mod privileged;
mod validation;

use config::{SambaConfig, Share};
use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, text, button, text_input, toggler, container};
use cosmic::{Application, Element, executor};
use privileged::{run, run_privileged, smbpasswd_add};
use std::path::Path;
use validation::{is_valid_path, is_valid_share_name};

#[derive(Debug, Clone)]
pub enum Message {
    StartSamba,
    StopSamba,
    StatusCheck,
    AddShare,
    RemoveShare(usize),
    ShareNameChanged(String),
    SharePathChanged(String),
    ShareCommentChanged(String),
    ToggleReadOnly(usize),
    ToggleGuest(usize),
    SaveConfig,
    ReloadShares,
    PickFolder,
    UserNameChanged(String),
    UserPasswordChanged(String),
    AddUser,
    RemoveUser(String),
    About,
}

pub struct App {
    core: Core,
    status: String,
    shares: Vec<Share>,
    new_name: String,
    new_path: String,
    new_comment: String,
    config_path: String,
    new_user: String,
    new_password: String,
    users: Vec<String>,
    message: String,
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.krmobdev.CosmicSamba";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let mut app = App {
            core,
            status: "Остановлен".into(),
            shares: Vec::new(),
            new_name: String::new(),
            new_path: String::new(),
            new_comment: String::new(),
            config_path: "/etc/samba/smb.conf".into(),
            new_user: String::new(),
            new_password: String::new(),
            users: Vec::new(),
            message: String::new(),
        };
        app.load_shares();
        app.load_users();
        (app, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        self.message.clear();
        match message {
            Message::StartSamba => {
                let res = run_privileged(&["systemctl", "start", "smbd"]);
                if res.success {
                    self.status = "Запущен".into();
                    self.message = "Samba запущена".into();
                } else {
                    self.message = format!("Не удалось запустить: {}", res.stderr.trim());
                }
            }
            Message::StopSamba => {
                let res = run_privileged(&["systemctl", "stop", "smbd"]);
                if res.success {
                    self.status = "Остановлен".into();
                    self.message = "Samba остановлена".into();
                } else {
                    self.message = format!("Не удалось остановить: {}", res.stderr.trim());
                }
            }
            Message::StatusCheck => {
                let res = run(&["systemctl", "is-active", "smbd"]);
                self.status = if res.stdout.trim() == "active" { "Запущен".into() } else { "Остановлен".into() };
            }
            Message::AddShare => {
                if !is_valid_share_name(&self.new_name) {
                    self.message = "Некорректное имя шары".into();
                } else if !is_valid_path(&self.new_path) {
                    self.message = "Некорректный путь (должен быть абсолютным, без ..)".into();
                } else {
                    self.shares.push(Share {
                        name: self.new_name.clone(),
                        path: self.new_path.clone(),
                        comment: self.new_comment.clone(),
                        read_only: false,
                        guest_ok: false, // default false for security
                        extra: Default::default(),
                    });
                    self.new_name.clear();
                    self.new_path.clear();
                    self.new_comment.clear();
                    self.message = "Шара добавлена (гостевой доступ выключен по умолчанию)".into();
                }
            }
            Message::RemoveShare(i) => {
                if i < self.shares.len() {
                    self.shares.remove(i);
                    self.message = "Шара удалена".into();
                }
            }
            Message::ShareNameChanged(s) => self.new_name = s,
            Message::SharePathChanged(s) => self.new_path = s,
            Message::ShareCommentChanged(s) => self.new_comment = s,
            Message::ToggleReadOnly(i) => {
                if let Some(share) = self.shares.get_mut(i) { share.read_only = !share.read_only; }
            }
            Message::ToggleGuest(i) => {
                if let Some(share) = self.shares.get_mut(i) { share.guest_ok = !share.guest_ok; }
            }
            Message::SaveConfig => {
                match self.save_config() {
                    Ok(()) => {
                        let _ = run_privileged(&["systemctl", "reload", "smbd"]);
                        self.message = "Конфиг сохранён безопасно (резервная копия + testparm)".into();
                    }
                    Err(e) => self.message = format!("Ошибка сохранения: {}", e),
                }
            }
            Message::ReloadShares => {
                self.load_shares();
                self.message = "Шары обновлены".into();
            }
            Message::PickFolder => {
                let res = run(&["zenity", "--file-selection", "--directory", "--title=Выберите папку"]);
                if res.success {
                    let path = res.stdout.trim().to_string();
                    if is_valid_path(&path) {
                        self.new_path = path;
                        self.message = "Папка выбрана".into();
                    } else {
                        self.message = "Выбранный путь некорректен".into();
                    }
                } else {
                    self.message = "Установи zenity или введи путь вручную".into();
                }
            }
            Message::UserNameChanged(s) => self.new_user = s,
            Message::UserPasswordChanged(s) => self.new_password = s,
            Message::AddUser => {
                if self.new_user.is_empty() || self.new_password.is_empty() {
                    self.message = "Имя и пароль обязательны".into();
                } else {
                    let _ = run(&["useradd", "-M", "-s", "/sbin/nologin", &self.new_user]);
                    let res = smbpasswd_add(&self.new_user, &self.new_password);
                    if res.success {
                        self.new_user.clear();
                        self.new_password.clear();
                        self.load_users();
                        self.message = "Пользователь добавлен".into();
                    } else {
                        self.message = format!("Ошибка добавления пользователя: {}", res.stderr.trim());
                    }
                }
            }
            Message::RemoveUser(user) => {
                let res = run(&["smbpasswd", "-x", &user]);
                if res.success {
                    self.load_users();
                    self.message = "Пользователь удалён".into();
                } else {
                    self.message = format!("Ошибка удаления: {}", res.stderr.trim());
                }
            }
            Message::About => {
                self.message = format!("Cosmic Samba v{} — безопасное управление Samba ❤️", Self::VERSION);
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let mut content = widget::column()
            .push(text::title1(format!("Cosmic Samba ❤️ v{}", Self::VERSION)))
            .push(text::body(format!("Статус: {}", self.status)))
            .push(
                widget::row()
                    .push(button::suggested("Старт").on_press(Message::StartSamba))
                    .push(button::destructive("Стоп").on_press(Message::StopSamba))
                    .push(button::standard("Проверить").on_press(Message::StatusCheck))
                    .push(button::standard("Обновить").on_press(Message::ReloadShares))
                    .push(button::standard("О программе").on_press(Message::About))
                    .spacing(10),
            )
            .spacing(10)
            .padding(16);

        if !self.message.is_empty() {
            content = content.push(text::body(&self.message));
        }

        content = content
            .push(text::heading("Добавить шару"))
            .push(
                widget::row()
                    .push(text_input("Имя", &self.new_name).on_input(Message::ShareNameChanged).width(Length::Fixed(120.0)))
                    .push(text_input("Путь", &self.new_path).on_input(Message::SharePathChanged).width(Length::Fill))
                    .push(button::standard("📁").on_press(Message::PickFolder))
                    .spacing(6),
            )
            .push(
                widget::row()
                    .push(text_input("Комментарий", &self.new_comment).on_input(Message::ShareCommentChanged).width(Length::Fill))
                    .push(button::suggested("Добавить").on_press(Message::AddShare))
                    .spacing(6),
            );

        content = content.push(text::heading("Шары"));
        if self.shares.is_empty() {
            content = content.push(text::body("Нет шар"));
        } else {
            for (i, share) in self.shares.iter().enumerate() {
                let card = container(
                    widget::column()
                        .push(
                            widget::row()
                                .push(text::heading(&share.name))
                                .push(button::destructive("Удалить").on_press(Message::RemoveShare(i)))
                                .spacing(8)
                                .align_y(Alignment::Center),
                        )
                        .push(text::body(&share.path))
                        .push(text::caption(&share.comment))
                        .push(
                            widget::row()
                                .push(text::body("Read only"))
                                .push(toggler(share.read_only).on_toggle(move |_| Message::ToggleReadOnly(i)))
                                .push(text::body("Guest"))
                                .push(toggler(share.guest_ok).on_toggle(move |_| Message::ToggleGuest(i)))
                                .spacing(12)
                                .align_y(Alignment::Center),
                        )
                        .spacing(4)
                )
                .padding(12)
                .width(Length::Fill);

                content = content.push(card);
            }
        }

        content = content.push(button::suggested("Сохранить шары").on_press(Message::SaveConfig));

        content = content
            .push(text::heading("Пользователи Samba"))
            .push(
                widget::row()
                    .push(text_input("Имя", &self.new_user).on_input(Message::UserNameChanged).width(Length::Fixed(140.0)))
                    .push(text_input("Пароль", &self.new_password).on_input(Message::UserPasswordChanged).password().width(Length::Fixed(140.0)))
                    .push(button::suggested("Добавить").on_press(Message::AddUser))
                    .spacing(6),
            );

        if self.users.is_empty() {
            content = content.push(text::body("Нет пользователей"));
        } else {
            for user in &self.users {
                let u = user.clone();
                content = content.push(
                    widget::row()
                        .push(text::body(user))
                        .push(button::destructive("Удалить").on_press(Message::RemoveUser(u)))
                        .spacing(12)
                        .align_y(Alignment::Center),
                );
            }
        }

        content = content.push(text::caption("Сделано с любовью для Рустама ❤️"));

        content.width(Length::Fill).height(Length::Fill).into()
    }
}

impl App {
    fn load_shares(&mut self) {
        match SambaConfig::load(Path::new(&self.config_path)) {
            Ok(cfg) => self.shares = cfg.shares,
            Err(_) => self.shares.clear(),
        }
    }

    fn load_users(&mut self) {
        self.users.clear();
        let res = run(&["pdbedit", "-L"]);
        if res.success {
            for line in res.stdout.lines() {
                if let Some(name) = line.split(':').next() {
                    if !name.is_empty() {
                        self.users.push(name.to_string());
                    }
                }
            }
        }
    }

    fn save_config(&self) -> Result<(), String> {
        let mut cfg = SambaConfig::load(Path::new(&self.config_path))
            .unwrap_or_else(|_| SambaConfig {
                global: Default::default(),
                shares: Vec::new(),
                preamble: String::new(),
            });

        // Ensure minimal safe globals if empty
        if cfg.global.is_empty() {
            cfg.global.insert("workgroup".into(), "WORKGROUP".into());
            cfg.global.insert("security".into(), "user".into());
        }

        // Replace shares with current managed list (preserves extras of existing by name)
        let mut new_shares = Vec::new();
        for share in &self.shares {
            if let Some(existing) = cfg.shares.iter().find(|s| s.name == share.name) {
                let mut updated = share.clone();
                updated.extra = existing.extra.clone();
                new_shares.push(updated);
            } else {
                new_shares.push(share.clone());
            }
        }
        // Keep unmanaged shares that are not in the app list? For now we treat all loaded as managed.
        // To preserve truly external shares, we would need a managed flag. For safety we keep all previous that are not overwritten.
        for old in &cfg.shares {
            if !new_shares.iter().any(|s| s.name == old.name) {
                new_shares.push(old.clone());
            }
        }
        cfg.shares = new_shares;

        cfg.save_safe(Path::new(&self.config_path))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default().size(cosmic::iced::Size::new(800.0, 700.0));
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}
