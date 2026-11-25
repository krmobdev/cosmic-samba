use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, text, button, text_input, toggler};
use cosmic::{Application, Element, executor};
use std::fs;
use std::process::Command;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum Message {
    StartSamba,
    StopSamba,
    StatusCheck,
    AddShare,
    RemoveShare(usize),
    ShareNameChanged(String),
    SharePathChanged(String),
    ToggleReadOnly(usize),
    ToggleGuest(usize),
    SaveConfig,
    ReloadShares,
    PickFolder,
    UserNameChanged(String),
    UserPasswordChanged(String),
    AddUser,
    RemoveUser(String),
}

#[derive(Debug, Clone)]
struct Share {
    name: String,
    path: String,
    comment: String,
    read_only: bool,
    guest_ok: bool,
}

pub struct App {
    core: Core,
    status: String,
    shares: Vec<Share>,
    new_name: String,
    new_path: String,
    config_path: String,
    new_user: String,
    new_password: String,
    users: Vec<String>,
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.krmobdev.CosmicSamba";

    fn core(&self) -> &Core { &self.core }
    fn core_mut(&mut self) -> &mut Core { &mut self.core }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let mut app = App {
            core,
            status: "Остановлен".into(),
            shares: Vec::new(),
            new_name: String::new(),
            new_path: String::new(),
            config_path: "/etc/samba/smb.conf".into(),
            new_user: String::new(),
            new_password: String::new(),
            users: Vec::new(),
        };
        app.load_shares();
        app.load_users();
        (app, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::StartSamba => {
                let _ = Command::new("systemctl").args(["start", "smbd"]).status();
                self.status = "Запущен".into();
            }
            Message::StopSamba => {
                let _ = Command::new("systemctl").args(["stop", "smbd"]).status();
                self.status = "Остановлен".into();
            }
            Message::StatusCheck => {
                if let Ok(out) = Command::new("systemctl").args(["is-active", "smbd"]).output() {
                    self.status = if out.stdout == b"active\n" { "Запущен".into() } else { "Остановлен".into() };
                }
            }
            Message::AddShare => {
                if !self.new_name.is_empty() && !self.new_path.is_empty() {
                    self.shares.push(Share {
                        name: self.new_name.clone(),
                        path: self.new_path.clone(),
                        comment: String::new(),
                        read_only: false,
                        guest_ok: true,
                    });
                    self.new_name.clear();
                    self.new_path.clear();
                }
            }
            Message::RemoveShare(i) => {
                if i < self.shares.len() { self.shares.remove(i); }
            }
            Message::ShareNameChanged(s) => self.new_name = s,
            Message::SharePathChanged(s) => self.new_path = s,
            Message::ToggleReadOnly(i) => {
                if let Some(share) = self.shares.get_mut(i) { share.read_only = !share.read_only; }
            }
            Message::ToggleGuest(i) => {
                if let Some(share) = self.shares.get_mut(i) { share.guest_ok = !share.guest_ok; }
            }
            Message::SaveConfig => {
                self.save_smb_conf();
                let _ = Command::new("systemctl").args(["reload", "smbd"]).status();
            }
            Message::ReloadShares => self.load_shares(),
            Message::PickFolder => {
                if let Ok(output) = Command::new("zenity").args(["--file-selection", "--directory", "--title=Выберите папку"]).output() {
                    if output.status.success() {
                        if let Ok(path) = String::from_utf8(output.stdout) {
                            self.new_path = path.trim().to_string();
                        }
                    }
                }
            }
            Message::UserNameChanged(s) => self.new_user = s,
            Message::UserPasswordChanged(s) => self.new_password = s,
            Message::AddUser => {
                if !self.new_user.is_empty() && !self.new_password.is_empty() {
                    let _ = Command::new("useradd").args(["-M", "-s", "/sbin/nologin", &self.new_user]).status();
                    let mut child = Command::new("smbpasswd").args(["-a", &self.new_user]).stdin(std::process::Stdio::piped()).spawn().unwrap();
                    if let Some(mut stdin) = child.stdin.take() {
                        use std::io::Write;
                        let _ = writeln!(stdin, "{}\n{}", self.new_password, self.new_password);
                    }
                    let _ = child.wait();
                    self.new_user.clear();
                    self.new_password.clear();
                    self.load_users();
                }
            }
            Message::RemoveUser(user) => {
                let _ = Command::new("smbpasswd").args(["-x", &user]).status();
                self.load_users();
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let mut content = widget::column()
            .push(text::title1("Cosmic Samba ❤️"))
            .push(text::body(format!("Статус: {}", self.status)))
            .push(
                widget::row()
                    .push(button::suggested("Старт").on_press(Message::StartSamba))
                    .push(button::destructive("Стоп").on_press(Message::StopSamba))
                    .push(button::standard("Проверить").on_press(Message::StatusCheck))
                    .push(button::standard("Обновить").on_press(Message::ReloadShares))
                    .spacing(10),
            )
            .spacing(12)
            .padding(16);

        content = content
            .push(text::heading("Добавить шару"))
            .push(
                widget::row()
                    .push(text_input("Имя", &self.new_name).on_input(Message::ShareNameChanged).width(Length::Fixed(140.0)))
                    .push(text_input("Путь", &self.new_path).on_input(Message::SharePathChanged).width(Length::Fill))
                    .push(button::standard("📁").on_press(Message::PickFolder))
                    .push(button::suggested("Добавить").on_press(Message::AddShare))
                    .spacing(6),
            );

        content = content.push(text::heading("Шары"));
        if self.shares.is_empty() {
            content = content.push(text::body("Нет шар"));
        } else {
            for (i, share) in self.shares.iter().enumerate() {
                content = content.push(
                    widget::column()
                        .push(
                            widget::row()
                                .push(text::body(format!("{} → {}", share.name, share.path)))
                                .push(button::destructive("Удалить").on_press(Message::RemoveShare(i)))
                                .spacing(8)
                                .align_y(Alignment::Center),
                        )
                        .push(
                            widget::row()
                                .push(text::body("Read only"))
                                .push(toggler(share.read_only).on_toggle(move |_| Message::ToggleReadOnly(i)))
                                .push(text::body("Guest"))
                                .push(toggler(share.guest_ok).on_toggle(move |_| Message::ToggleGuest(i)))
                                .spacing(12)
                                .align_y(Alignment::Center),
                        )
                        .spacing(2)
                        .padding([0, 0, 6, 0]),
                );
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

        content.width(Length::Fill).height(Length::Fill).into()
    }
}

impl App {
    fn load_shares(&mut self) {
        self.shares.clear();
        if !Path::new(&self.config_path).exists() { return; }
        if let Ok(content) = fs::read_to_string(&self.config_path) {
            let mut current: Option<Share> = None;
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('[') && line.ends_with(']') && line != "[global]" {
                    if let Some(share) = current.take() {
                        if !share.path.is_empty() { self.shares.push(share); }
                    }
                    let name = line[1..line.len()-1].to_string();
                    current = Some(Share { name, path: String::new(), comment: String::new(), read_only: false, guest_ok: false });
                } else if let Some(ref mut share) = current {
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim();
                        match key {
                            "path" => share.path = value.to_string(),
                            "comment" => share.comment = value.to_string(),
                            "read only" => share.read_only = value.eq_ignore_ascii_case("yes"),
                            "guest ok" => share.guest_ok = value.eq_ignore_ascii_case("yes"),
                            _ => {}
                        }
                    }
                }
            }
            if let Some(share) = current {
                if !share.path.is_empty() { self.shares.push(share); }
            }
        }
    }

    fn load_users(&mut self) {
        self.users.clear();
        if let Ok(output) = Command::new("pdbedit").args(["-L"]).output() {
            if let Ok(text) = String::from_utf8(output.stdout) {
                for line in text.lines() {
                    if let Some(name) = line.split(':').next() {
                        if !name.is_empty() { self.users.push(name.to_string()); }
                    }
                }
            }
        }
    }

    fn save_smb_conf(&self) {
        let mut conf = String::from("[global]\nworkgroup = WORKGROUP\nserver string = Cosmic Samba Manager\nsecurity = user\nmap to guest = Bad User\nguest account = nobody\n\n");
        for share in &self.shares {
            conf.push_str(&format!("[{}]\npath = {}\nbrowseable = yes\nread only = {}\nguest ok = {}\ncomment = {}\n\n", share.name, share.path, if share.read_only {"yes"} else {"no"}, if share.guest_ok {"yes"} else {"no"}, share.comment));
        }
        if Path::new("/etc/samba").exists() {
            let _ = fs::write(&self.config_path, conf);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default().size(cosmic::iced::Size::new(760.0, 640.0));
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}
