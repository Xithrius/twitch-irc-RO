use color_eyre::eyre::{bail, Error, Result};
use serde::{Deserialize, Serialize};
use serde_with::DeserializeFromStr;
use std::{
    cell::RefCell,
    env,
    fs::{create_dir_all, read_to_string, File},
    io::Write,
    path::Path,
    rc::Rc,
    str::FromStr,
};
use tokio::{runtime::Handle, task};
use tui::widgets::BorderType;

use crate::{
    emotes::support_graphics_protocol,
    handlers::{
        args::{merge_args_into_config, Cli},
        interactive::interactive_config,
        state::State,
    },
    utils::{
        emotes::emotes_enabled,
        pathing::{cache_path, config_path},
    },
};

pub type SharedCompleteConfig = Rc<RefCell<CompleteConfig>>;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct CompleteConfig {
    /// Connecting to Twitch.
    pub twitch: TwitchConfig,
    /// Internal functionality.
    pub terminal: TerminalConfig,
    /// If anything should be recorded for future use.
    pub storage: StorageConfig,
    /// Filtering out messages.
    pub filters: FiltersConfig,
    /// How everything looks to the user.
    pub frontend: FrontendConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TwitchConfig {
    /// The current user's name.
    pub username: String,
    /// The streamer's channel name.
    pub channel: String,
    /// The IRC channel to connect to.
    pub server: String,
    /// The authentication token for the IRC.
    pub token: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct TerminalConfig {
    /// The delay in milliseconds between terminal updates.
    pub delay: u64,
    /// The maximum amount of messages before truncation.
    pub maximum_messages: usize,
    /// The file path to log to.
    pub log_file: Option<String>,
    /// if debug logging should be enabled.
    pub verbose: bool,
    /// What state the application should start in.
    pub first_state: State,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct StorageConfig {
    /// If previous channels switched to should be tracked.
    pub channels: bool,
    /// If previous username mentions should be tracked.
    pub mentions: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(default)]
pub struct FiltersConfig {
    /// If filters should be enabled at all.
    pub enabled: bool,
    /// If the regex filters should be reversed.
    pub reversed: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct FrontendConfig {
    /// If the time and date is to be shown.
    pub show_datetimes: bool,
    /// Play stream with `streamlink` upon join a channel.
    pub auto_start_streamlink: bool,
    /// Only shows currently streaming channels instead of all following channels
    pub only_show_live_channels: bool,
    /// The format of string that will show up in the terminal.
    pub datetime_format: String,
    /// If the username should be shown.
    pub username_shown: bool,
    /// The color palette.
    pub palette: Palette,
    /// Show Title with time and channel.
    pub title_shown: bool,
    /// The amount of space between the chat window and the terminal border.
    pub margin: u16,
    /// Show twitch badges next to usernames.
    pub badges: bool,
    /// Theme, being either light or dark.
    pub theme: Theme,
    /// If the username should be highlighted when it appears in chat.
    pub username_highlight: bool,
    /// If there should be state tabs shown on the bottom of the terminal.
    pub state_tabs: bool,
    /// The shape of the cursor in insert boxes.
    pub cursor_shape: CursorType,
    /// If the cursor should be blinking.
    pub blinking_cursor: bool,
    /// If the scrolling should be inverted.
    pub inverted_scrolling: bool,
    /// If scroll offset integer should be shown.
    pub show_scroll_offset: bool,
    /// If twitch emotes should be displayed (requires kitty terminal).
    pub twitch_emotes: bool,
    /// If betterttv emotes should be displayed (requires kitty terminal).
    pub betterttv_emotes: bool,
    /// If 7tv emotes should be displayed (requires kitty terminal).
    pub seventv_emotes: bool,
    /// If frankerfacez emotes should be displayed (requires kitty terminal).
    pub frankerfacez_emotes: bool,
    /// Channels to always be displayed in the start screen.
    pub favorite_channels: Vec<String>,
    /// The amount of recent channels that should be shown on the start screen.
    pub recent_channel_count: u16,
    /// A border wrapper around [`BorderType`].
    pub border_type: Border,
    /// If chat border should be hidden
    pub hide_chat_border: bool,
    /// If the usernames should be aligned to the right.
    pub right_align_usernames: bool,
    /// Do not display the window size warning.
    pub show_unsupported_screen_size: bool,
}

impl Default for TwitchConfig {
    fn default() -> Self {
        Self {
            username: String::new(),
            channel: String::new(),
            server: "irc.chat.twitch.tv".to_string(),
            token: None,
        }
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            delay: 30,
            maximum_messages: 500,
            log_file: None,
            verbose: false,
            first_state: State::default(),
        }
    }
}

impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            show_datetimes: true,
            only_show_live_channels: true,
            auto_start_streamlink: false,
            datetime_format: "%a %b %e %T %Y".to_string(),
            username_shown: true,
            palette: Palette::default(),
            title_shown: true,
            margin: 0,
            badges: false,
            theme: Theme::default(),
            username_highlight: true,
            state_tabs: false,
            cursor_shape: CursorType::default(),
            blinking_cursor: false,
            inverted_scrolling: false,
            show_scroll_offset: false,
            twitch_emotes: false,
            betterttv_emotes: false,
            seventv_emotes: false,
            frankerfacez_emotes: false,
            favorite_channels: vec![],
            recent_channel_count: 5,
            border_type: Border::default(),
            hide_chat_border: false,
            right_align_usernames: false,
            show_unsupported_screen_size: true,
        }
    }
}

#[derive(Serialize, DeserializeFromStr, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Palette {
    Pastel,
    Vibrant,
    Warm,
    Cool,
}

impl Default for Palette {
    fn default() -> Self {
        Self::Pastel
    }
}

impl FromStr for Palette {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pastel" => Ok(Self::Pastel),
            "vibrant" => Ok(Self::Vibrant),
            "warm" => Ok(Self::Warm),
            "cool" => Ok(Self::Cool),
            _ => bail!("Palette '{}' cannot be deserialized", s),
        }
    }
}

#[derive(Serialize, DeserializeFromStr, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Dark,
    Light,

    #[allow(dead_code)]
    Custom,
}

impl Default for Theme {
    fn default() -> Self {
        Self::Dark
    }
}

impl FromStr for Theme {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            _ => bail!("Theme '{}' cannot be deserialized", s),
        }
    }
}

#[derive(Serialize, DeserializeFromStr, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CursorType {
    User,
    Line,
    Block,
    UnderScore,
}

impl Default for CursorType {
    fn default() -> Self {
        Self::User
    }
}

impl FromStr for CursorType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "user" => Ok(Self::User),
            "line" => Ok(Self::Line),
            "underscore" => Ok(Self::UnderScore),
            "block" => Ok(Self::Block),
            _ => bail!("Cursor type of '{}' cannot be deserialized", s),
        }
    }
}

#[derive(Serialize, DeserializeFromStr, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Border {
    Plain,
    Rounded,
    Double,
    Thick,
}

impl Default for Border {
    fn default() -> Self {
        Self::Plain
    }
}

impl FromStr for Border {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "plain" => Ok(Self::Plain),
            "rounded" => Ok(Self::Rounded),
            "double" => Ok(Self::Double),
            "thick" => Ok(Self::Thick),
            _ => bail!("Border '{}' cannot be deserialized", s),
        }
    }
}

impl From<Border> for BorderType {
    fn from(val: Border) -> Self {
        match val {
            Border::Plain => Self::Plain,
            Border::Rounded => Self::Rounded,
            Border::Double => Self::Double,
            Border::Thick => Self::Thick,
        }
    }
}

pub trait ToVec<T> {
    fn to_vec(&self) -> Vec<T>;
}

impl ToVec<(String, String)> for TwitchConfig {
    fn to_vec(&self) -> Vec<(String, String)> {
        vec![
            ("Username".to_string(), self.username.to_string()),
            ("Channel".to_string(), self.channel.to_string()),
            ("Server".to_string(), self.server.to_string()),
        ]
    }
}

impl ToVec<(String, String)> for TerminalConfig {
    fn to_vec(&self) -> Vec<(String, String)> {
        vec![
            ("Current channel".to_string(), self.delay.to_string()),
            (
                "Max messages".to_string(),
                self.maximum_messages.to_string(),
            ),
            (
                "Log file".to_string(),
                self.log_file.clone().map_or("None".to_string(), |f| f),
            ),
            ("First state".to_string(), self.first_state.to_string()),
        ]
    }
}

impl ToVec<(String, String)> for StorageConfig {
    fn to_vec(&self) -> Vec<(String, String)> {
        vec![
            ("Channels enabled".to_string(), self.channels.to_string()),
            ("Mentions enabled".to_string(), self.mentions.to_string()),
        ]
    }
}

impl ToVec<(String, String)> for FiltersConfig {
    fn to_vec(&self) -> Vec<(String, String)> {
        vec![
            ("Enabled".to_string(), self.enabled.to_string()),
            ("Reversed".to_string(), self.reversed.to_string()),
        ]
    }
}

impl ToVec<(String, String)> for FrontendConfig {
    fn to_vec(&self) -> Vec<(String, String)> {
        vec![
            (
                "Show datetimes".to_string(),
                self.show_datetimes.to_string(),
            ),
            (
                "Datetime format".to_string(),
                self.datetime_format.to_string(),
            ),
            (
                "Username shown".to_string(),
                self.username_shown.to_string(),
            ),
            // ("".to_string(), self.palette.to_string()),
            ("Title shown".to_string(), self.title_shown.to_string()),
            ("Margin".to_string(), self.margin.to_string()),
            ("Badges".to_string(), self.badges.to_string()),
            // ("".to_string(), self.theme.to_string()),
            (
                "Username highlight".to_string(),
                self.username_highlight.to_string(),
            ),
            ("State tabs".to_string(), self.state_tabs.to_string()),
            // ("".to_string(), self.cursor_shape.to_string()),
            (
                "Blinking cursor".to_string(),
                self.blinking_cursor.to_string(),
            ),
            (
                "Inverted scrolling".to_string(),
                self.inverted_scrolling.to_string(),
            ),
            (
                "Scroll offset shown".to_string(),
                self.show_scroll_offset.to_string(),
            ),
            ("Twitch emotes".to_string(), self.twitch_emotes.to_string()),
            (
                "BetterTTV emotes".to_string(),
                self.betterttv_emotes.to_string(),
            ),
            (
                "SevenTV emotes".to_string(),
                self.seventv_emotes.to_string(),
            ),
            (
                "FrankerFacez emotes".to_string(),
                self.frankerfacez_emotes.to_string(),
            ),
            // ("".to_string(), self.favorite_channels.to_string()),
            (
                "Recent channel count".to_string(),
                self.recent_channel_count.to_string(),
            ),
            // ("".to_string(), self.border_type.to_string()),
            (
                "Right aligned usernames".to_string(),
                self.right_align_usernames.to_string(),
            ),
        ]
    }
}

fn persist_config(path: &Path, config: &CompleteConfig) -> Result<()> {
    let toml_string = toml::to_string(&config)?;
    let mut file = File::create(path)?;

    file.write_all(toml_string.as_bytes())?;
    drop(file);

    Ok(())
}

const RAW_DEFAULT_CONFIG_URL: &str =
    "https://raw.githubusercontent.com/Xithrius/twitch-tui/main/default-config.toml";

fn persist_default_config(path: &Path) {
    let default_config = task::block_in_place(move || {
        Handle::current().block_on(async move {
            reqwest::get(RAW_DEFAULT_CONFIG_URL)
                .await
                .unwrap()
                .text()
                .await
                .unwrap()
        })
    });

    let mut file = File::create(path).unwrap();

    file.write_all(default_config.as_bytes()).unwrap();
    drop(file);
}

impl CompleteConfig {
    pub fn new(cli: Cli) -> Result<Self, Error> {
        let path_str = cache_path("");

        let p = Path::new(&path_str);
        if !p.exists() {
            create_dir_all(p).unwrap();
        }

        let path_str = config_path("config.toml");

        let p = Path::new(&path_str);

        if !p.exists() {
            create_dir_all(p.parent().unwrap()).unwrap();

            if let Some(config) = interactive_config() {
                persist_config(p, &config)?;
                Ok(config)
            } else {
                persist_default_config(p);
                bail!("Default configuration was generated at {path_str}, please fill it out with necessary information.")
            }
        } else if let Ok(file_content) = read_to_string(p) {
            let mut config: Self = match toml::from_str(&file_content) {
                Ok(c) => c,
                Err(err) => bail!("Config could not be processed. Error: {:?}", err.message()),
            };

            merge_args_into_config(&mut config, cli);

            let token = env::var("TWT_TOKEN").ok();
            if let Some(env_token) = token {
                if !env_token.is_empty() {
                    config.twitch.token = Some(env_token);
                }
            }

            {
                let t = &config.twitch;

                let check_token = t.token.as_ref().map_or("", |t| t);

                if t.username.is_empty() || t.channel.is_empty() || check_token.is_empty() {
                    bail!("Twitch config section is missing one or more of the following: username, channel, token.");
                }

                if emotes_enabled(&config.frontend) && !support_graphics_protocol().unwrap_or(false)
                {
                    eprintln!("This terminal does not support the graphics protocol.\nUse a terminal such as kitty, or disable emotes.");
                    std::process::exit(1);
                }
            }

            // Channel names for the IRC connection can only be in lowercase.
            config.twitch.channel = config.twitch.channel.to_lowercase();

            Ok(config)
        } else {
            bail!(
                "Configuration could not be read correctly. See the following link for the example config: {}",
                format!("{}/blob/main/default-config.toml", env!("CARGO_PKG_REPOSITORY"))
            )
        }
    }
}
