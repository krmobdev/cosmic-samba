use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, text, button, text_input};
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
    SaveConfig,
    ReloadShares,
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
        };
        app.load_shares();
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
                if i < self.shares.len() {
                    self.shares.remove(i);
                }
            }
            Message::ShareNameChanged(s) => self.new_name = s,
            Message::SharePathChanged(s) => self.new_path = s,
            Message::SaveConfig => {
                self.save_smb_conf();
                let _ = Command::new("systemctl").args(["reload", "smbd"]).status();
            }
            Message::ReloadShares => {
                self.load_shares();
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
                    .push(button::standard("Обновить шары").on_press(Message::ReloadShares))
                    .spacing(12),
            )
            .spacing(16)
            .padding(20);

        content = content
            .push(text::heading("Добавить шару"))
            .push(
                widget::row()
                    .push(text_input("Имя", &self.new_name).on_input(Message::ShareNameChanged).width(Length::Fixed(180.0)))
                    .push(text_input("Путь", &self.new_path).on_input(Message::SharePathChanged).width(Length::Fill))
                    .push(button::suggested("Добавить").on_press(Message::AddShare))
                    .spacing(8),
            );

        content = content.push(text::heading("Текущие шары"));
        if self.shares.is_empty() {
            content = content.push(text::body("Шары не найдены"));
        } else {
            for (i, share) in self.shares.iter().enumerate() {
                let info = format!("{} → {}  (guest: {}, ro: {})", share.name, share.path, share.guest_ok, share.read_only);
                content = content.push(
                    widget::row()
                        .push(text::body(info))
                        .push(button::destructive("Удалить").on_press(Message::RemoveShare(i)))
                        .spacing(12)
                        .align_y(Alignment::Center),
                );
            }
        }

        content = content.push(
            widget::row()
                .push(button::suggested("Сохранить и перезагрузить").on_press(Message::SaveConfig))
                .spacing(12)
        );

        content.width(Length::Fill).height(Length::Fill).into()
    }
}

impl App {
    fn load_shares(&mut self) {
        self.shares.clear();
        if !Path::new(&self.config_path).exists() {
            return;
        }
        if let Ok(content) = fs::read_to_string(&self.config_path) {
            let mut current: Option<Share> = None;
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('[') && line.ends_with(']') && line != "[global]" {
                    if let Some(share) = current.take() {
                        self.shares.push(share);
                    }
                    let name = line[1..line.len()-1].to_string();
                    current = Some(Share {
                        name,
                        path: String::new(),
                        comment: String::new(),
                        read_only: false,
                        guest_ok: false,
                    });
                } else if let Some(ref mut share) = current {
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim();
                        match key {
                            "path" => share.path = value.to_string(),
                            "comment" => share.comment = value.to_string(),
                            "read only" => share.read_only = value == "yes",
                            "guest ok" => share.guest_ok = value == "yes",
                            _ => {}
                        }
                    }
                }
            }
            if let Some(share) = current {
                if !share.path.is_empty() {
                    self.shares.push(share);
                }
            }
        }
    }

    fn save_smb_conf(&self) {
        let mut conf = String::from(
            "[global]\n\
             workgroup = WORKGROUP\n\
             server string = Cosmic Samba Manager\n\
             security = user\n\
             map to guest = Bad User\n\
             guest account = nobody\n\n"
        );

        for share in &self.shares {
            conf.push_str(&format!(
                "[{}]\n\
                 path = {}\n\
                 browseable = yes\n\
                 read only = {}\n\
                 guest ok = {}\n\
                 comment = {}\n\n",
                share.name,
                share.path,
                if share.read_only { "yes" } else { "no" },
                if share.guest_ok { "yes" } else { "no" },
                share.comment
            ));
        }

        if Path::new("/etc/samba").exists() {
            let _ = fs::write(&self.config_path, conf);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default().size(cosmic::iced::Size::new(680.0, 520.0));
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}
