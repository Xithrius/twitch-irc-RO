mod badges;
pub mod channels;
mod connection;
pub mod oauth;

use std::{collections::HashMap, hash::BuildHasher};

use channels::StreamingUser;
use color_eyre::Result;
use futures::StreamExt;
use irc::{
    client::prelude::Capability,
    proto::{Command, Message},
};
use log::{debug, info};
use tokio::sync::{broadcast::Receiver, mpsc::Sender};

use crate::{
    emotes::{get_twitch_emote, DownloadedEmotes},
    handlers::{
        config::CompleteConfig,
        data::{DataBuilder, TwitchToTerminalAction},
        state::State,
    },
    twitch::{
        badges::retrieve_user_badges,
        connection::{client_stream_reconnect, wait_client_stream},
    },
    utils::{
        emotes::emotes_enabled,
        text::{clean_message, parse_message_action},
    },
};

#[derive(Debug, Clone)]
pub enum TwitchAction {
    Privmsg(String),
    Join(String),
    ClearMessages,
}

pub async fn twitch_irc(
    mut config: CompleteConfig,
    tx: Sender<TwitchToTerminalAction>,
    mut rx: Receiver<TwitchAction>,
) {
    info!("Spawned Twitch IRC thread.");

    // If the dashboard is the start state, wait until the user has selected
    // a channel before connecting to Twitch's IRC.
    if config.terminal.first_state == State::Dashboard {
        debug!("Waiting for user to select channel from debug screen");

        loop {
            if let Ok(TwitchAction::Join(channel)) = rx.recv().await {
                config.twitch.channel = channel;

                debug!("User has selected channel from start screen");
                break;
            }
        }
    }

    let enable_emotes = emotes_enabled(&config.frontend);

    let data_builder = DataBuilder::new(&config.frontend.datetime_format);
    let mut room_state_startup = false;

    let (mut client, mut stream) =
        wait_client_stream(tx.clone(), data_builder, config.clone()).await;

    let sender = client.sender();

    // Request commands capabilities
    if client
        .send_cap_req(&[
            Capability::Custom("twitch.tv/commands"),
            Capability::Custom("twitch.tv/tags"),
        ])
        .is_err()
    {
        tx.send(
            data_builder.system(
                "Unable to request commands/tags capability, certain features may be affected."
                    .to_string(),
            ),
        )
        .await
        .unwrap();
    }

    let mut connected = true;

    loop {
        tokio::select! {
            biased;

            Ok(action) = rx.recv() => {
                let current_channel = format!("#{}", config.twitch.channel);

                match action {
                    TwitchAction::Privmsg(message) => {
                        debug!("Sending message to Twitch: {}", message);

                        client
                            .send_privmsg(current_channel, message)
                            .unwrap();
                    }
                    TwitchAction::Join(channel) => {
                        debug!("Switching to channel {channel}");

                        let channel_list = format!("#{channel}");

                        // Leave previous channel
                        if let Err(err) = sender.send_part(current_channel) {
                            tx.send(data_builder.twitch(err.to_string())).await.unwrap();
                        }

                        // Join specified channel
                        if let Err(err) = sender.send_join(&channel_list) {
                            tx.send(data_builder.twitch(err.to_string())).await.unwrap();
                        }


                        // Set old channel to new channel
                        config.twitch.channel = channel;
                    }
                    TwitchAction::ClearMessages => {
                        client.send(Command::Raw("CLEARCHAT".to_string(), vec![])).unwrap();
                    }
                }
            }
            Some(message) = stream.next() => {
                match message {
                    Ok(message) => {
                        if !connected {
                            tx.send(data_builder.system("Reconnect succcessful.".to_string())).await.unwrap();
                            connected = true;
                        }

                        if let Some(b) = handle_message_command(message, tx.clone(), data_builder, config.frontend.badges, room_state_startup, enable_emotes).await {
                            room_state_startup = b;
                        }
                    }
                    Err(err) => {
                        connected = false;

                        debug!("Twitch connection error encountered: {}, attempting to reconnect.", err);

                        (client, stream) = client_stream_reconnect(err, tx.clone(), data_builder, &config).await;

                    }
                }
            }
            else => {}
        };
    }
}

/// Emotes comming from twitch arrive in the `emote` tag.
/// They have the format `<emote-id1>:<start>-<end>,.../<emote-id2>:...`
/// This functions returns a list of emote name and id,
/// using the first position of each emote to get the emote name from the message.
/// <https://dev.twitch.tv/docs/irc/tags/> (emotes tag in PRIVMSG tags section)
fn retrieve_twitch_emotes(message: &str, emotes: &str) -> Vec<(String, String)> {
    emotes
        .split('/')
        .filter_map(|e| {
            let (id, pos) = e.split_once(':')?;

            let pos = pos.split(',').next()?;
            let (s, e) = pos.split_once('-')?;

            let (start, end): (usize, usize) = (s.parse().ok()?, e.parse().ok()?);

            Some((
                message.chars().skip(start).take(end - start + 1).collect(),
                id.to_string(),
            ))
        })
        .collect()
}

async fn handle_message_command(
    message: Message,
    tx: Sender<TwitchToTerminalAction>,
    data_builder: DataBuilder<'_>,
    badges: bool,
    room_state_startup: bool,
    enable_emotes: bool,
) -> Option<bool> {
    let mut tags: HashMap<&str, &str> = HashMap::new();

    if let Some(ref ref_tags) = message.tags {
        for tag in ref_tags {
            if let Some(ref tag_value) = tag.1 {
                tags.insert(&tag.0, tag_value);
            }
        }
    }

    match message.command {
        Command::PRIVMSG(ref _target, ref msg) => {
            // Detects if the message contains an IRC CTCP Action, and return the message content.
            // WARNING: Emote parsing needs to be done *after* the message has been extracted from the action,
            // but *before* problematic unicode characters have been removed from it.
            let (msg, highlight) = parse_message_action(msg);

            // Parse emotes from message tags
            let emotes = enable_emotes
                .then(|| tags.get("emotes").map(|&e| retrieve_twitch_emotes(msg, e)))
                .unwrap_or_default()
                .unwrap_or_default();

            // Download emotes if they are not downloaded yet.
            // Small optimisation of starting the download here, and only await this block at the last moment.
            let emotes =
                futures::stream::iter(emotes.into_iter().map(|(name, filename)| async move {
                    get_twitch_emote(&filename).await?;
                    Ok((name, (filename, false)))
                }))
                .buffer_unordered(10)
                .collect::<Vec<Result<(String, (String, bool))>>>();

            // lowercase username from message
            let mut name = message.source_nickname().unwrap().to_string();

            retrieve_user_badges(&mut name, &message, badges);

            // Remove invalid unicode characters from the message.
            let cleaned_message = clean_message(msg);

            let message_id = tags.get("id").map(|&s| s.to_string());
            let user_id = tags.get("user-id").map(|&s| s.to_string());

            debug!("Message received from twitch: {name} - {cleaned_message:?}");

            let emotes = emotes.await.into_iter().flatten().collect();

            tx.send(DataBuilder::user(
                name,
                user_id,
                cleaned_message,
                emotes,
                message_id,
                highlight,
            ))
            .await
            .unwrap();
        }
        Command::NOTICE(ref _target, ref msg) => {
            tx.send(data_builder.twitch(msg.to_string())).await.unwrap();
        }
        Command::JOIN(ref channel, _, _) => {
            tx.send(data_builder.twitch(format!("Joined {}", *channel)))
                .await
                .unwrap();
        }
        Command::Raw(ref cmd, ref _items) => {
            match cmd.as_ref() {
                // https://dev.twitch.tv/docs/irc/tags/#roomstate-tags
                "ROOMSTATE" => {
                    // Only display roomstate on startup, since twitch
                    // sends a NOTICE whenever roomstate changes.
                    if !room_state_startup {
                        handle_roomstate(&tx, &tags).await;
                    }

                    return Some(true);
                }
                // https://dev.twitch.tv/docs/irc/tags/#usernotice-tags
                "USERNOTICE" => {
                    if let Some(value) = tags.get("system-msg") {
                        tx.send(data_builder.twitch((*value).to_string()))
                            .await
                            .unwrap();
                    }
                }
                // https://dev.twitch.tv/docs/irc/tags/#clearchat-tags
                "CLEARCHAT" => {
                    let user_id = tags.get("target-user-id").map(|&s| s.to_string());

                    tx.send(TwitchToTerminalAction::ClearChat(user_id.clone()))
                        .await
                        .unwrap();

                    // User was either timed out or banned
                    if user_id.is_some() {
                        let ban_duration = tags.get("ban-duration").map(|&s| s.to_string());

                        // TODO: In both cases of this branch, replace "User" with the username that the punishment was inflicted upon

                        // User was timed out
                        if let Some(duration) = ban_duration {
                            tx.send(
                                data_builder
                                    .twitch(format!("User was timed out for {duration} seconds")),
                            )
                            .await
                            .unwrap();
                        }
                        // User was banned
                        else {
                            tx.send(data_builder.twitch("User banned".to_string()))
                                .await
                                .unwrap();
                        }
                    } else {
                        tx.send(data_builder.twitch("Chat cleared by a moderator.".to_string()))
                            .await
                            .unwrap();
                    }
                }
                // https://dev.twitch.tv/docs/irc/tags/#clearmsg-tags
                "CLEARMSG" => {
                    if let Some(id) = tags.get("target-msg-id") {
                        tx.send(TwitchToTerminalAction::DeleteMessage((*id).to_string()))
                            .await
                            .unwrap();
                    }
                }
                _ => (),
            }
        }
        _ => (),
    }

    None
}

pub async fn handle_roomstate<S: BuildHasher>(
    tx: &Sender<TwitchToTerminalAction>,
    tags: &HashMap<&str, &str, S>,
) {
    let mut room_state = String::new();

    for (name, value) in tags {
        match *name {
            "emote-only" if *value == "1" => {
                room_state.push_str("The channel is emote-only.\n");
            }
            "followers-only" if *value != "-1" => {
                room_state.push_str("The channel is followers-only.\n");
            }
            "subs-only" if *value == "1" => {
                room_state.push_str("The channel is subscribers-only.\n");
            }
            "slow" if *value != "0" => {
                room_state.push_str("The channel has a ");
                room_state.push_str(value);
                room_state.push_str("s slowmode.\n");
            }
            _ => (),
        }
    }

    // Trim last newline
    room_state.pop();

    if room_state.is_empty() {
        return;
    }

    let message_id = tags.get("target-msg-id").map(|&s| s.to_string());

    tx.send(DataBuilder::user(
        String::from("Info"),
        None,
        room_state,
        DownloadedEmotes::default(),
        message_id,
        false,
    ))
    .await
    .unwrap();
}
