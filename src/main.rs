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
}

#[derive(Debug, Clone)]
struct Share {
    name: String,
    path: String,
    comment: String,
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
                    .spacing(12),
            )
            .spacing(16)
            .padding(20);

        content = content
            .push(text::heading("Добавить шару"))
            .push(
                widget::row()
                    .push(text_input("Имя", &self.new_name).on_input(Message::ShareNameChanged))
                    .push(text_input("Путь", &self.new_path).on_input(Message::SharePathChanged))
                    .push(button::suggested("Добавить").on_press(Message::AddShare))
                    .spacing(8),
            );

        content = content.push(text::heading("Текущие шары"));
        for (i, share) in self.shares.iter().enumerate() {
            content = content.push(
                widget::row()
                    .push(text::body(format!("{} → {}", share.name, share.path)))
                    .push(button::destructive("Удалить").on_press(Message::RemoveShare(i)))
                    .spacing(12)
                    .align_y(Alignment::Center),
            );
        }

        content = content.push(button::suggested("Сохранить и перезагрузить").on_press(Message::SaveConfig));

        content.width(Length::Fill).height(Length::Fill).into()
    }
}

impl App {
    fn load_shares(&mut self) {
        self.shares.push(Share {
            name: "shared".into(),
            path: "/home/user/shared".into(),
            comment: "Пример".into(),
        });
    }

    fn save_smb_conf(&self) {
        let mut conf = String::from(
            "[global]\n\
             workgroup = WORKGROUP\n\
             server string = Cosmic Samba\n\
             security = user\n\
             map to guest = Bad User\n\n"
        );

        for share in &self.shares {
            conf.push_str(&format!(
                "[{}]\n\
                 path = {}\n\
                 browseable = yes\n\
                 read only = no\n\
                 guest ok = yes\n\
                 comment = {}\n\n",
                share.name, share.path, share.comment
            ));
        }

        if Path::new("/etc/samba").exists() {
            let _ = fs::write("/etc/samba/smb.conf", conf);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default().size(cosmic::iced::Size::new(620.0, 480.0));
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}
