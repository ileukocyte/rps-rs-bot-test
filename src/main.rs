use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::sync::Mutex;
use std::time::Duration;

use lazy_static::lazy_static;

use serenity::async_trait;
use serenity::Client;
use serenity::client::{Context, EventHandler};
use serenity::futures::StreamExt;
use serenity::model::application::command::{Command, CommandOptionType};
use serenity::model::application::component::ButtonStyle;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId, MessageId};
use serenity::model::prelude::component::ComponentType;
use serenity::model::prelude::interaction::application_command::CommandDataOptionValue;
use serenity::prelude::{GatewayIntents, Mentionable};
use serenity::utils::Color;

use tracing::info;

const SUCCESS_COLOR: Color = Color::from_rgb(140, 190, 218);
const FAILURE_COLOR: Color = Color::from_rgb(239, 67, 63);
const CONFIRMATION_COLOR: Color = Color::from_rgb(118, 255, 3);
const WARNING_COLOR: Color = Color::from_rgb(255, 242, 54);

lazy_static! {
    pub static ref SESSIONS: Mutex<HashSet<(u64, u64)>> = Mutex::new(HashSet::new());
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: ChannelId,
        id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
        let mut sessions = SESSIONS.lock().unwrap();

        for session in sessions.clone().iter().filter(|(_, m)| m == id.as_u64()) {
            sessions.remove(session);
        }
    }

    async fn ready(&self, ctx: Context, _ready: Ready) {
        if !ctx.http.get_global_application_commands().await.unwrap().iter().any(|cmd| cmd.name == "tic-tac-toe") {
            Command::create_global_application_command(&ctx.http, |cmd| {
                cmd
                    .name("tic-tac-toe")
                    .description("Starts the tic-tac-toe game against the specified user")
                    .create_option(|option| {
                        option
                            .name("opponent")
                            .description("The user to play tic-tac-toe against")
                            .kind(CommandOptionType::User)
                            .required(true)
                    })
            }).await.expect("The tic-tac-toe command could not have been registered!");
        }

        info!("Connected to Discord!");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(cmd) = interaction {
            if cmd.data.name == "tic-tac-toe" {
                let option = &cmd.data.options[0];

                if let Some(CommandDataOptionValue::User(opponent, _)) = &option.resolved {
                    if opponent.bot || opponent.id == cmd.user.id {
                        if let Err(_) = cmd.create_interaction_response(&ctx.http, |response| {
                            response
                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|msg| {
                                    msg
                                        .ephemeral(true)
                                        .embed(|embed| {
                                            embed
                                                .author(|a| a.name("Failure!"))
                                                .color(FAILURE_COLOR)
                                                .description("You cannot play against the specified user!")
                                        })
                                })
                        }).await {}

                        return;
                    }

                    if SESSIONS.lock().unwrap().iter().any(|(u, _)| u == cmd.user.id.as_u64() || u == opponent.id.as_u64()) {
                        if let Err(_) = cmd.create_interaction_response(&ctx.http, |response| {
                            response
                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                .interaction_response_data(|msg| {
                                    msg
                                        .ephemeral(true)
                                        .embed(|embed| {
                                            embed
                                                .author(|a| a.name("Failure!"))
                                                .color(FAILURE_COLOR)
                                                .description("Either user is already playing tic-tac-toe!")
                                        })
                                })
                        }).await {}

                        return;
                    }

                    if let Err(_) = cmd.create_interaction_response(&ctx.http, |response| {
                        response
                            .kind(InteractionResponseType::ChannelMessageWithSource)
                            .interaction_response_data(|msg| {
                                msg
                                    .content(opponent.mention())
                                    .embed(|embed| {
                                        embed
                                            .author(|a| a.name("Confirmation!"))
                                            .color(CONFIRMATION_COLOR)
                                            .description(
                                                format!("Do you want to play tic-tac-toe against {}?", cmd.user.mention())
                                            )
                                    })
                                    .components(|comp| {
                                        comp.create_action_row(|row| {
                                            row
                                                .create_button(|button| {
                                                    button
                                                        .label("Yes")
                                                        .custom_id("play")
                                                        .style(ButtonStyle::Secondary)
                                                })
                                                .create_button(|button| {
                                                    button
                                                        .label("No")
                                                        .custom_id("deny")
                                                        .style(ButtonStyle::Danger)
                                                })
                                        })
                                    })
                            })
                    }).await { return; }

                    if let Ok(response) = cmd.get_interaction_response(&ctx.http).await {
                        SESSIONS.lock().unwrap().extend([
                            (*cmd.user.id.as_u64(), *response.id.as_u64()),
                            (*opponent.id.as_u64(), *response.id.as_u64()),
                        ]);

                        let mut interaction_stream = response.await_component_interactions(&ctx)
                            .filter(|i| i.data.component_type == ComponentType::Button)
                            .timeout(Duration::from_secs(60 * 5))
                            .build();

                        while let Some(interaction) = interaction_stream.next().await {
                            let mut round_counter = 1usize;
                            let id: Vec<_> = interaction.data.custom_id.split("-").collect();
                            let suffix = *id.last().unwrap();

                            match suffix {
                                "play" | "deny" => {
                                    if interaction.user.id == opponent.id {
                                        if suffix == "play" {
                                            if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                response
                                                    .kind(InteractionResponseType::UpdateMessage)
                                                    .interaction_response_data(|msg| {
                                                        msg
                                                            .components(|comp| {
                                                                comp.create_action_row(|row| {
                                                                    row
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Secondary)
                                                                                .emoji('\u{270A}')
                                                                                .custom_id(format!("{}-rock", cmd.user.id))
                                                                        })
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Secondary)
                                                                                .emoji('\u{270B}')
                                                                                .custom_id(format!("{}-paper", cmd.user.id))
                                                                        })
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Secondary)
                                                                                .emoji('\u{270C}')
                                                                                .custom_id(format!("{}-scissors", cmd.user.id))
                                                                        })
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Danger)
                                                                                .label("Exit")
                                                                                .custom_id("stop")
                                                                        })
                                                                })
                                                            })
                                                            .content("")
                                                            .embed(|embed| {
                                                                embed
                                                                    .color(SUCCESS_COLOR)
                                                                    .author(|author| {
                                                                        author
                                                                            .name(format!("Round #{}!", round_counter))
                                                                            .icon_url(
                                                                                cmd.user.avatar_url()
                                                                                    .unwrap_or_else(|| cmd.user.default_avatar_url())
                                                                            )
                                                                    })
                                                                    .description(format!("It is {}'s turn!", cmd.user.mention()))
                                                            })
                                                    })
                                            }).await {}
                                        } else {
                                            if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                response
                                                    .kind(InteractionResponseType::UpdateMessage)
                                                    .interaction_response_data(|msg| {
                                                        msg
                                                            .components(|comp| comp)
                                                            .content(cmd.user.mention())
                                                            .embed(|embed| {
                                                                embed
                                                                    .author(|a| a.name("Failure!"))
                                                                    .color(FAILURE_COLOR)
                                                                    .description(format!("{} has denied your invitation!", opponent.mention()))
                                                            })
                                                    })
                                            }).await {}

                                            let mut sessions = SESSIONS.lock().unwrap();

                                            sessions.remove(&(*opponent.id.as_u64(), *response.id.as_u64()));
                                            sessions.remove(&(*cmd.user.id.as_u64(), *response.id.as_u64()));

                                            break;
                                        }
                                    } else {
                                        if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                            response
                                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                                .interaction_response_data(|msg| {
                                                    msg
                                                        .ephemeral(true)
                                                        .embed(|embed| {
                                                            embed
                                                                .author(|a| a.name("Failure!"))
                                                                .color(FAILURE_COLOR)
                                                                .description("You are not the user who has to reply to the command!")
                                                        })
                                                })
                                        }).await {}
                                    }
                                },
                                "rock" | "paper" | "scissors" => {
                                    if interaction.user.id.to_string().as_str() == id[0] {
                                        if interaction.user.id == cmd.user.id {
                                            if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                response
                                                    .kind(InteractionResponseType::UpdateMessage)
                                                    .interaction_response_data(|msg| {
                                                        msg
                                                            .components(|comp| {
                                                                comp.create_action_row(|row| {
                                                                    row
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Secondary)
                                                                                .emoji('\u{270A}')
                                                                                .custom_id(format!("{}-{}-rock", opponent.id, suffix))
                                                                        })
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Secondary)
                                                                                .emoji('\u{270B}')
                                                                                .custom_id(format!("{}-{}-paper", opponent.id, suffix))
                                                                        })
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Secondary)
                                                                                .emoji('\u{270C}')
                                                                                .custom_id(format!("{}-{}-scissors", opponent.id, suffix))
                                                                        })
                                                                        .create_button(|button| {
                                                                            button
                                                                                .style(ButtonStyle::Danger)
                                                                                .label("Exit")
                                                                                .custom_id("stop")
                                                                        })
                                                                })
                                                            })
                                                            .embed(|embed| {
                                                                embed
                                                                    .color(SUCCESS_COLOR)
                                                                    .author(|author| {
                                                                        author
                                                                            .name(format!("Round #{}!", round_counter))
                                                                            .icon_url(
                                                                                opponent.avatar_url()
                                                                                    .unwrap_or_else(|| opponent.default_avatar_url())
                                                                            )
                                                                    })
                                                                    .description(format!("It is {}'s turn!", opponent.mention()))
                                                            })
                                                    })
                                            }).await {}
                                        } else {
                                            let starter_turn = id[1];
                                            let winner = match starter_turn {
                                                "rock" => match suffix {
                                                    "rock" => None,
                                                    "paper" => Some(opponent),
                                                    _ => Some(&cmd.user),
                                                },
                                                "paper" => match suffix {
                                                    "rock" => Some(&cmd.user),
                                                    "paper" => None,
                                                    _ => Some(opponent),
                                                },
                                                _ => match suffix {
                                                    "rock" => Some(opponent),
                                                    "paper" => Some(&cmd.user),
                                                    _ => None,
                                                },
                                            };

                                            if let Some(winner) = winner {
                                                if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                    response
                                                        .kind(InteractionResponseType::UpdateMessage)
                                                        .interaction_response_data(|msg| {
                                                            let winner_turn = if winner.id == cmd.user.id {
                                                                starter_turn
                                                            } else {
                                                                suffix
                                                            };

                                                            let loser_turn = if winner.id == cmd.user.id {
                                                                suffix
                                                            } else {
                                                                starter_turn
                                                            };

                                                            let winner_turn = match winner_turn {
                                                                "rock" => "\u{270A} Rock",
                                                                "paper" => "\u{270B} Paper",
                                                                _ => "\u{270C} Scissors",
                                                            };

                                                            let loser_turn = match loser_turn {
                                                                "rock" => "\u{270A} Rock",
                                                                "paper" => "\u{270B} Paper",
                                                                _ => "\u{270C} Scissors",
                                                            };

                                                            msg
                                                                .components(|comp| comp)
                                                                .embed(|embed| {
                                                                    embed
                                                                        .color(SUCCESS_COLOR)
                                                                        .author(|author| {
                                                                            author
                                                                                .name("Congratulations!")
                                                                                .icon_url(
                                                                                    winner.avatar_url()
                                                                                        .unwrap_or_else(|| winner.default_avatar_url())
                                                                                )
                                                                        })
                                                                        .description(format!("{} wins!", winner.mention()))
                                                                        .field("Winner's Turn", winner_turn, true)
                                                                        .field("Loser's Turn", loser_turn, true)
                                                                })
                                                        })
                                                }).await {}

                                                let mut sessions = SESSIONS.lock().unwrap();

                                                sessions.remove(&(*opponent.id.as_u64(), *response.id.as_u64()));
                                                sessions.remove(&(*cmd.user.id.as_u64(), *response.id.as_u64()));

                                                break;
                                            } else {
                                                round_counter += 1;

                                                if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                    response
                                                        .kind(InteractionResponseType::UpdateMessage)
                                                        .interaction_response_data(|msg| {
                                                            msg
                                                                .components(|comp| {
                                                                    comp.create_action_row(|row| {
                                                                        row
                                                                            .create_button(|button| {
                                                                                button
                                                                                    .style(ButtonStyle::Secondary)
                                                                                    .emoji('\u{270A}')
                                                                                    .custom_id(format!("{}-rock", cmd.user.id))
                                                                            })
                                                                            .create_button(|button| {
                                                                                button
                                                                                    .style(ButtonStyle::Secondary)
                                                                                    .emoji('\u{270B}')
                                                                                    .custom_id(format!("{}-paper", cmd.user.id))
                                                                            })
                                                                            .create_button(|button| {
                                                                                button
                                                                                    .style(ButtonStyle::Secondary)
                                                                                    .emoji('\u{270C}')
                                                                                    .custom_id(format!("{}-scissors", cmd.user.id))
                                                                            })
                                                                            .create_button(|button| {
                                                                                button
                                                                                    .style(ButtonStyle::Danger)
                                                                                    .label("Exit")
                                                                                    .custom_id("stop")
                                                                            })
                                                                    })
                                                                })
                                                                .embed(|embed| {
                                                                    embed
                                                                        .color(SUCCESS_COLOR)
                                                                        .author(|author| {
                                                                            author
                                                                                .name(format!("Round #{}!", round_counter))
                                                                                .icon_url(
                                                                                    cmd.user.avatar_url()
                                                                                        .unwrap_or_else(|| cmd.user.default_avatar_url())
                                                                                )
                                                                        })
                                                                        .description(format!("It is {}'s turn!", cmd.user.mention()))
                                                                })
                                                        })
                                                }).await {}
                                            }
                                        }
                                    } else {
                                        if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                            response
                                                .kind(InteractionResponseType::ChannelMessageWithSource)
                                                .interaction_response_data(|msg| {
                                                    msg
                                                        .ephemeral(true)
                                                        .embed(|embed| {
                                                            embed
                                                                .author(|a| a.name("Failure!"))
                                                                .color(FAILURE_COLOR)
                                                                .description(if id[0] != cmd.user.id.to_string().as_str()
                                                                    && id[0] != opponent.id.to_string().as_str()
                                                                {
                                                                    "You did not invoke the initial command!"
                                                                } else {
                                                                    "It is not your turn at the moment!"
                                                                })
                                                        })
                                                })
                                        }).await {}
                                    }
                                },
                                _ => {
                                    if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                        response
                                            .kind(InteractionResponseType::UpdateMessage)
                                            .interaction_response_data(|msg| {
                                                msg
                                                    .components(|comp| comp)
                                                    .embed(|embed| {
                                                        embed
                                                            .author(|a| a.name("Warning!"))
                                                            .color(WARNING_COLOR)
                                                            .description(format!("{} has terminated the session!", interaction.user.mention()))
                                                    })
                                            })
                                    }).await {}

                                    let mut sessions = SESSIONS.lock().unwrap();

                                    sessions.remove(&(*opponent.id.as_u64(), *response.id.as_u64()));
                                    sessions.remove(&(*cmd.user.id.as_u64(), *response.id.as_u64()));

                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    {
        env::set_var("RUST_LOG", "DEBUG");

        tracing_subscriber::fmt::init();

        info!("Starting!");
    }

    let token = env::var("DISCORD_TOKEN")?;
    let intents = GatewayIntents::all();

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await?;

    if let Err(err) = client.start().await {
        println!("An error occurred while running the client: {:?}", err);
    }

    Ok(())
}