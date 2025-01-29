use std::{
    fmt::Display,
    string::{String, ToString},
    vec::Vec,
};

use color_eyre::Result;
use futures::TryFutureExt;
use serde::Deserialize;

use crate::{
    handlers::config::TwitchConfig,
    ui::components::utils::{SearchItemGetter, ToQueryString},
};

use super::oauth::{get_twitch_client, get_twitch_client_id};

const FOLLOWER_COUNT: usize = 100;

#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)]
pub struct FollowingUser {
    broadcaster_id: String,
    pub broadcaster_login: String,
    pub broadcaster_name: String,
    followed_at: String,
}

impl ToQueryString for FollowingUser {
    fn to_query_string(&self) -> String {
        self.broadcaster_name.clone()
    }
}

// "id": "42170724654",
// "user_id": "132954738",
// "user_login": "aws",
// "user_name": "AWS",
// "game_id": "417752",
// "game_name": "Talk Shows & Podcasts",
// "type": "live",
// "title": "AWS Howdy Partner! Y'all welcome ExtraHop to the show!",
// "viewer_count": 20,
// "started_at": "2021-03-31T20:57:26Z",
// "language": "en",
// "thumbnail_url": "https://static-cdn.jtvnw.net/previews-ttv/live_user_aws-{width}x{height}.jpg",
// "tag_ids": [],
// "tags": ["English"]
#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)]
pub struct StreamingUser {
    pub user_login: String,
    pub game_name: String,
    pub title: String,
}

impl Display for StreamingUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            user_login,
            game_name,
            title,
        } = self;
        let fmt_game = format!("[{game_name:.22}]");
        write!(f, "{user_login:<16.16}: {fmt_game:<24} {title}",)
    }
}

impl ToQueryString for StreamingUser {
    fn to_query_string(&self) -> String {
        self.user_login.clone()
    }
}

#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)]
pub struct StreamingList {
    pub data: Vec<StreamingUser>,
    pagination: Pagination,
}

impl Display for FollowingUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.broadcaster_login)
    }
}

// data 	Object[] 	The list of streams.
// pagination 	Object 	The information used to page through the list of results. The object is empty if there are no more pages left to page through. Read More
//    cursor 	String 	The cursor used to get the next page of results. Set the request’s after or before query parameter to this value depending on whether you’re paging forwards or backwards.
#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)]
struct Pagination {
    cursor: Option<String>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[allow(dead_code)]
pub struct FollowingList {
    pub total: u64,
    pub data: Vec<FollowingUser>,
    pagination: Pagination,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Following {
    // TODO: Don't re-create client on new requests
    // client: &Client,
    twitch_config: TwitchConfig,
    list: FollowingList,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FollowingStreaming {
    // TODO: Don't re-create client on new requests
    // client: &Client,
    twitch_config: TwitchConfig,
    list: StreamingList,
}

// https://dev.twitch.tv/docs/api/reference/#get-followed-channels
pub async fn get_following(twitch_config: &TwitchConfig) -> Result<FollowingList> {
    let client = get_twitch_client(twitch_config.token.as_deref()).await?;
    let user_id = &get_twitch_client_id(None).await?.user_id;

    Ok(client
        .get(format!(
            "https://api.twitch.tv/helix/channels/followed?user_id={user_id}&first={FOLLOWER_COUNT}",
        ))
        .send()
        .await?
        .error_for_status()?
        .json::<FollowingList>()
        .await?)
}

// https://dev.twitch.tv/docs/api/reference/#get-followed-streams
pub async fn get_streams(twitch_config: &TwitchConfig) -> Result<StreamingList> {
    let client = get_twitch_client(twitch_config.token.as_deref()).await?;
    let user_id = &get_twitch_client_id(None).await?.user_id;

    let res = client
        .clone()
        .get(format!(
            "https://api.twitch.tv/helix/streams/followed?user_id={user_id}&first={FOLLOWER_COUNT}",
        ))
        .send()
        .await?
        .error_for_status()?;

    Ok(res.json::<StreamingList>().await?)
}

impl FollowingStreaming {
    pub fn new(twitch_config: TwitchConfig) -> Self {
        Self {
            twitch_config,
            list: StreamingList::default(),
        }
    }
}

impl SearchItemGetter<StreamingUser> for FollowingStreaming {
    async fn get_items(&mut self) -> Result<Vec<StreamingUser>> {
        get_streams(&self.twitch_config).await.map(|x| x.data)
    }
}

impl Following {
    pub fn new(twitch_config: TwitchConfig) -> Self {
        Self {
            twitch_config,
            list: FollowingList::default(),
        }
    }
}

impl SearchItemGetter<FollowingUser> for Following {
    async fn get_items(&mut self) -> Result<Vec<FollowingUser>> {
        get_following(&self.twitch_config).await.map(|v| v.data)
    }
}
