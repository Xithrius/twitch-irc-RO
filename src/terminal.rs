use log::{debug, info, warn};
use std::time::Duration;
use tokio::sync::{broadcast::Sender, mpsc::Receiver};

use crate::{
    commands::{init_terminal, quit_terminal, reset_terminal},
    emotes::{display_emote, query_emotes, ApplyCommand, DecodedEmote},
    handlers::{
        app::App,
        config::CompleteConfig,
        data::{MessageData, TwitchToTerminalAction},
        state::State,
        user_input::events::{Config, Events, Key},
    },
    twitch::{oauth::get_twitch_client_id, TwitchAction},
    utils::emotes::emotes_enabled,
};

pub enum TerminalAction<T> {
    Quit,
    BackOneLayer,
    SwitchState(State),
    ClearMessages,
    Enter(T),
}

impl<T> TerminalAction<T> {
    pub fn map_enter<B, F: Fn(&T) -> B>(&self, a_map: F) -> TerminalAction<B> {
        match self {
            TerminalAction::Quit => TerminalAction::Quit,
            TerminalAction::BackOneLayer => TerminalAction::BackOneLayer,
            TerminalAction::SwitchState(s) => TerminalAction::SwitchState(*s),
            TerminalAction::ClearMessages => TerminalAction::ClearMessages,
            TerminalAction::Enter(a) => TerminalAction::Enter(a_map(a)),
        }
    }
}

pub async fn ui_driver(
    config: CompleteConfig,
    mut app: App,
    tx: Sender<TwitchAction>,
    mut rx: Receiver<TwitchToTerminalAction>,
    mut drx: Option<Receiver<Result<DecodedEmote, String>>>,
) {
    info!("Started UI driver.");

    let original_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic| {
        debug!("Panic hook hit.");

        reset_terminal();
        original_hook(panic);
    }));

    let mut events = Events::with_config(Config {
        exit_key: Key::Null,
        tick_rate: Duration::from_millis(config.terminal.delay),
    });

    let mut erx = query_emotes(&config, config.twitch.channel.clone());

    let mut terminal = init_terminal(&config.frontend);

    terminal.clear().unwrap();

    let is_emotes_enabled = emotes_enabled(&config.frontend);

    loop {
        if is_emotes_enabled {
            // Check if we have received any emotes
            if let Ok((user_emotes, global_emotes)) = erx.try_recv() {
                *app.emotes.user_emotes.borrow_mut() = user_emotes;
                *app.emotes.global_emotes.borrow_mut() = global_emotes;

                for message in &mut *app.messages.borrow_mut() {
                    message.reparse_emotes(&app.emotes, is_emotes_enabled);
                }
            };

            // Check if we need to load a decoded emote
            if let Some(rx) = &mut drx {
                if let Ok(r) = rx.try_recv() {
                    match r {
                        Ok(d) => {
                            if let Err(e) = d.apply() {
                                warn!("Unable to send command to load emote. {e}");
                            } else if let Err(e) = display_emote(d.id(), 1, d.cols()) {
                                warn!("Unable to send command to display emote. {e}");
                            }
                        }
                        Err(name) => {
                            warn!("Unable to load emote: {name}.");
                            app.emotes.user_emotes.borrow_mut().remove(&name);
                            app.emotes.global_emotes.borrow_mut().remove(&name);
                            app.emotes.info.borrow_mut().remove(&name);
                        }
                    }
                }
            }
        }

        if let Ok(msg) = rx.try_recv() {
            match msg {
                TwitchToTerminalAction::Message(m) => {
                    app.messages
                        .borrow_mut()
                        .push_front(MessageData::from_twitch_message(
                            m,
                            &app.emotes,
                            is_emotes_enabled,
                        ));

                    // If scrolling is enabled, pad for more messages.
                    if app.components.chat.scroll_offset.get_offset() > 0 {
                        app.components.chat.scroll_offset.up();
                    }
                }
                TwitchToTerminalAction::ClearChat(user_id) => {
                    if let Some(user) = user_id {
                        app.purge_user_messages(user.as_str());
                    } else {
                        app.clear_messages();
                    }
                }
                TwitchToTerminalAction::DeleteMessage(message_id) => {
                    app.remove_message_with(message_id.as_str());
                }
            }
        }

        if let Some(event) = events.next().await {
            if let Some(action) = app.event(&event).await {
                match action {
                    TerminalAction::Quit => {
                        // Emotes need to be unloaded before we exit the alternate screen
                        app.emotes.unload();
                        quit_terminal(terminal);

                        break;
                    }
                    TerminalAction::BackOneLayer => {
                        if let Some(previous_state) = app.get_previous_state() {
                            app.set_state(previous_state);
                        } else {
                            app.set_state(config.terminal.first_state.clone());
                        }
                    }
                    TerminalAction::SwitchState(state) => {
                        if state == State::Normal {
                            app.clear_messages();
                        }

                        app.set_state(state);
                    }
                    TerminalAction::ClearMessages => {
                        app.clear_messages();

                        tx.send(TwitchAction::ClearMessages).unwrap();
                    }
                    TerminalAction::Enter(action) => match action {
                        TwitchAction::Privmsg(message) => {
                            const ME_COMMAND: &str = "/me ";

                            let (msg, highlight) = message.strip_prefix(ME_COMMAND).map_or_else(
                                || (message.clone(), false),
                                |msg| (msg.to_string(), true),
                            );

                            let user_id = get_twitch_client_id(config.twitch.token.as_deref())
                                .await
                                .map(|x| x.user_id.clone())
                                .ok();

                            let message_data = MessageData::new_user_message(
                                config.twitch.username.to_string(),
                                user_id,
                                false,
                                msg,
                                None,
                                highlight,
                                &app.emotes,
                                is_emotes_enabled,
                            );

                            app.messages.borrow_mut().push_front(message_data);

                            tx.send(TwitchAction::Privmsg(message)).unwrap();
                        }
                        TwitchAction::Join(channel) => {
                            app.clear_messages();
                            app.emotes.unload();

                            tx.send(TwitchAction::Join(channel.clone())).unwrap();

                            if config.frontend.auto_start_streamlink {
                                app.open_stream(channel.as_str());
                            }

                            erx = query_emotes(&config, channel);

                            app.set_state(State::Normal);
                        }

                        TwitchAction::ClearMessages => {}
                    },
                }
            }
        }

        terminal.draw(|f| app.draw(f)).unwrap();
    }

    app.cleanup();

    reset_terminal();
}
