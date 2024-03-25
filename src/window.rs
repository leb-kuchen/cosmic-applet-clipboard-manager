use cosmic::app::Core;
use cosmic::iced::wayland::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{subscription, Command, Limits};
use cosmic::iced_futures::futures::{channel, SinkExt, StreamExt};
use cosmic::iced_futures::Subscription;
use cosmic::iced_runtime::core::window;
use cosmic::iced_style::application;
use cosmic::{widget, Apply};
use cosmic::{Element, Theme};
use std::fs;
use std::path::Path;
use std::sync::atomic::{self, AtomicBool};
use std::sync::Arc;

use crate::config::{Config, CONFIG_VERSION};
#[allow(unused_imports)]
use crate::fl;
use cosmic::cosmic_config;

use cosmic_time::{Duration, Timeline};

pub const ID: &str = "dev.dominiccgeh.CosmicAppletClipboardManager";
const ICON: &str = "display-symbolic";

pub static PRIVATE_MODE: AtomicBool = AtomicBool::new(false);
pub struct Window {
    core: Core,
    popup: Option<Id>,

    config: Config,
    #[allow(dead_code)]
    config_handler: Option<cosmic_config::Config>,
    clipboard_history: Vec<String>,
    timeline: Timeline,
    sender: Option<channel::mpsc::Sender<()>>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Config(Config),
    TogglePopup,
    PopupClosed(Id),
    ClipboardDone(String),
    ClipboardHistroy(Vec<String>),
    Channel(channel::mpsc::Sender<()>),
    Frame(std::time::Instant),
    Ignore,
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: Config,
}

impl cosmic::Application for Window {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(
        core: Core,
        flags: Self::Flags,
    ) -> (Self, Command<cosmic::app::Message<Self::Message>>) {
        let config = flags.config;
        PRIVATE_MODE.store(config.private_mode, atomic::Ordering::Relaxed);
        let window = Window {
            core,
            config,
            config_handler: flags.config_handler,
            clipboard_history: Vec::new(),
            popup: None,
            sender: None,

            timeline: Timeline::new(),
        };
        (window, Command::none())
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Command<cosmic::app::Message<Self::Message>> {
        // Helper for updating config values efficiently
        #[allow(unused_macros)]
        macro_rules! config_set {
            ($name: ident, $value: expr) => {
                match &self.config_handler {
                    Some(config_handler) => {
                        match paste::paste! { self.config.[<set_ $name>](config_handler, $value) } {
                            Ok(_) => {}
                            Err(err) => {
                                eprintln!("failed to save config {:?}: {}", stringify!($name), err);
                            }
                        }
                    }
                    None => {
                        self.config.$name = $value;
                        eprintln!(
                            "failed to save config {:?}: no config handler",
                            stringify!($name),
                        );
                    }
                }
            };
        }

        match message {
            Message::Config(config) => {
                if config == self.config {
                    return Command::none();
                }
                let private_mode_new = config.private_mode;
                self.config = config;

                let private_mode = PRIVATE_MODE.fetch_update(
                    atomic::Ordering::Relaxed,
                    atomic::Ordering::Relaxed,
                    |old| (old != private_mode_new).then_some(private_mode_new),
                );
                match (&self.sender, private_mode) {
                    (Some(sender), Ok(private_mode)) if private_mode => {
                        let mut sender = sender.clone();
                        return Command::perform(async move { sender.send(()).await }, |_| {
                            cosmic::app::message::app(Message::Ignore)
                        });
                    }
                    _ => {}
                }
            }

            Message::Frame(now) => self.timeline.now(now),

            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings =
                        self.core
                            .applet
                            .get_popup_settings(Id::MAIN, new_id, None, None, None);
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                }
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }

            Message::ClipboardDone(clip) => {
                self.clipboard_history.push(clip);
            }
            Message::ClipboardHistroy(clip_history) => self.clipboard_history = clip_history,
            Message::Channel(sender) => self.sender = Some(sender),
            Message::Ignore => {}
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        self.core
            .applet
            .icon_button(ICON)
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<Self::Message> {
        let cosmic::cosmic_theme::Spacing { space_xs, .. } =
            self.core.system_theme().cosmic().spacing;
        let mut content_list = widget::column::with_capacity(self.clipboard_history.len())
            .padding([8, 0])
            .spacing(space_xs);
        // collapse  or srollable for now?
        for (_idx, clip_entry) in self.clipboard_history.iter().rev().enumerate() {
            let row = widget::column::with_children(vec![
                widget::button(widget::text(clip_entry)).into(),
                widget::divider::horizontal::default().into(),
            ]);
            content_list = content_list.push(row);
        }
        let content_list = content_list.apply(widget::scrollable).height(500);
        self.core.applet.popup_container(content_list).into()
    }
    fn subscription(&self) -> Subscription<Self::Message> {
        struct ConfigSubscription;
        let config = cosmic_config::config_subscription(
            std::any::TypeId::of::<ConfigSubscription>(),
            Self::APP_ID.into(),
            CONFIG_VERSION,
        )
        .map(|update| {
            if !update.errors.is_empty() {
                eprintln!(
                    "errors loading config {:?}: {:?}",
                    update.keys, update.errors
                );
            }
            Message::Config(update.config)
        });
        let timeline = self
            .timeline
            .as_subscription()
            .map(|(_, now)| Message::Frame(now));

        Subscription::batch(vec![config, timeline, clipboard_chan()])
    }

    fn style(&self) -> Option<<Theme as application::StyleSheet>::Style> {
        Some(cosmic::applet::style())
    }
}
fn clipboard_chan() -> Subscription<Message> {
    use std::io::Read;
    use wl_clipboard_rs::paste::{get_contents, ClipboardType, Error, MimeType, Seat};

    struct ClipboardSubscription;
    let id = std::any::TypeId::of::<ClipboardSubscription>();
    return subscription::channel(id, 20, |mut output| async move {
        // opening sqlite connection, if fails, fall back to memmory, it this fails, do not save to storage
        let mut output1 = output.clone();
        let conntection_opt = tokio::task::spawn_blocking(move || {
            let data_local_dir = directories::BaseDirs::new();
            let data_local_dir = data_local_dir
                .as_ref()
                .map(|dir| dir.data_local_dir().join(ID).join("history"));
            dbg!(&data_local_dir);

            // creating share directory if it does not exists
            if let Some(path) = &data_local_dir {
                if let Some(dir) = path.parent() {
                    if !dir.exists() {
                        _ = fs::create_dir(dir).inspect_err(|e| {
                            eprintln!(
                                "error creating directory: {dir:?}, which does not exists: {e}"
                            )
                        });
                    }
                }
            }
            let open_memory = || {
                sqlite::Connection::open_thread_safe(":memory:").inspect_err(|e| {
                    eprintln!(
                    "Failed to open sqlite memory backup connection (cannot save clipboard): {e}"
                )
                })
            };
            let open_dir = |dir: &Path| {
                sqlite::Connection::open_thread_safe(dir).inspect_err(|e| {
                    eprintln!(
                        "Failed to open sqlite connection to {dir:?} (cannot save clipboard): {e}",
                    )
                })
            };
            let connection = match data_local_dir {
                Some(dir) => open_dir(&dir).or_else(|_| open_memory()),
                None => open_memory(),
            };
            // fetch rows on demand
            if let Ok(connection) = &connection {
                let query = "create table if not exists clipboard_history 
                (
                id INTEGER PRIMARY KEY,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                content TEXT
                )
                ";
                let mut statement = connection.prepare(query).unwrap();
                statement.next().unwrap();
                let query = "SELECT * FROM clipboard_history
                ORDER BY id DESC
                LIMIT 100;
                ";
                let mut clipboard_history = Vec::with_capacity(100);
                for (count, row) in connection
                    .prepare(query)
                    .unwrap()
                    .into_iter()
                    .map(|row| row.unwrap())
                    .enumerate()
                {
                    let content = row.read::<&str, _>("content");
                    println!("\nCount({count}) \n / content = {content}");
                    clipboard_history.push(content.to_owned());
                }
                _ = output1.try_send(Message::ClipboardHistroy(clipboard_history));
            }

            return connection.ok().map(Arc::new);
        })
        .await
        .unwrap();

        let (tx, mut rx) = channel::mpsc::channel(1);
        output.send(Message::Channel(tx)).await.unwrap();

        // reading clipboard every 500ms
        let mut inteval = tokio::time::interval(Duration::from_millis(500));
        let mut last_clipboard_entry: Option<Arc<str>> = None;

        loop {
            // let start = std::time::Instant::now();
            if PRIVATE_MODE.load(atomic::Ordering::Relaxed) {
                // potencial for concurrent writes
                rx.select_next_some().await;
            }
            inteval.tick().await;
            let last_clipboard_entry_clone = last_clipboard_entry.as_ref().map(Arc::clone);
            let clip_opt = tokio::task::spawn_blocking(move || {
                let result =
                    get_contents(ClipboardType::Regular, Seat::Unspecified, MimeType::Text);
                return match result {
                    Ok((mut pipe, _)) => {
                        let mut contents = vec![];
                        pipe.read_to_end(&mut contents)
                            .inspect_err(|e| eprintln!("Error piping to clipboard {e}"))
                            .ok()?;

                        let new_clip = String::from_utf8_lossy(&contents).to_string();
                        match last_clipboard_entry_clone {
                            Some(last) if *last == new_clip => None,
                            None | Some(_) => Some(new_clip),
                        }
                    }
                    //The clipboard is empty or doesn't contain text, nothing to worry about.
                    Err(Error::NoSeats) | Err(Error::ClipboardEmpty) | Err(Error::NoMimeType) => {
                        None
                    }

                    Err(err) => {
                        eprintln!("Error pasting to clipboard {err}");
                        None
                    }
                };
            })
            .await
            .unwrap();

            if let Some(clip) = clip_opt {
                _ = output
                    .send(Message::ClipboardDone(clip.clone()))
                    .await
                    .inspect_err(|e| eprintln!("Failed to send clipboard event: {e}"));
                dbg!(&clip);

                last_clipboard_entry.replace(Arc::from(clip.clone()));

                // saving to storage if there is a connection
                if conntection_opt.is_some() {
                    // when inserting to delete predictions, so it is not delted everytime
                    let connection = conntection_opt.as_ref().map(Arc::clone).unwrap();
                    tokio::task::spawn_blocking(move || {
                        let query = "insert into clipboard_history (content) values (?); ";
                        let mut statement = connection.prepare(query).unwrap();
                        statement.bind((1, clip.as_str())).unwrap();
                        statement.next().unwrap();
                    })
                    .await
                    .unwrap();
                }
            }
        }
    });
}
