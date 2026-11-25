use cosmic::app::{Core, Settings, Task};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, text, button};
use cosmic::{Application, Element, executor};
use std::process::Command;

#[derive(Debug, Clone)]
pub enum Message {
    StartSamba,
    StopSamba,
    AddShare,
    StatusCheck,
}

pub struct App {
    core: Core,
    status: String,
    shares: Vec<String>,
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.krmobdev.CosmicSamba";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let app = App {
            core,
            status: "Остановлен".to_string(),
            shares: vec!["/home/user/shared".to_string()],
        };
        (app, Task::none())
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::StartSamba => {
                // Placeholder: in real app use systemctl or smbcontrol
                let _ = Command::new("systemctl").args(["start", "smbd"]).status();
                self.status = "Запущен".to_string();
            }
            Message::StopSamba => {
                let _ = Command::new("systemctl").args(["stop", "smbd"]).status();
                self.status = "Остановлен".to_string();
            }
            Message::AddShare => {
                self.shares.push("/new/share".to_string());
            }
            Message::StatusCheck => {
                // Check if smbd is running
                let output = Command::new("systemctl").args(["is-active", "smbd"]).output();
                if let Ok(out) = output {
                    if out.stdout == b"active\n" {
                        self.status = "Запущен".to_string();
                    } else {
                        self.status = "Остановлен".to_string();
                    }
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let mut col = widget::column()
            .push(text::title1("Cosmic Samba Manager ❤️"))
            .push(text::body(format!("Статус: {}", self.status)))
            .push(
                widget::row()
                    .push(button::suggested("Старт").on_press(Message::StartSamba))
                    .push(button::destructive("Стоп").on_press(Message::StopSamba))
                    .push(button::standard("Проверить").on_press(Message::StatusCheck))
                    .spacing(10),
            )
            .push(text::heading("Шара:"))
            .spacing(16)
            .padding(20)
            .align_x(Alignment::Center);

        for share in &self.shares {
            col = col.push(text::body(share));
        }

        col.push(button::suggested("Добавить шару").on_press(Message::AddShare))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::default().size(cosmic::iced::Size::new(500.0, 400.0));
    cosmic::app::run::<App>(settings, ())?;
    Ok(())
}
