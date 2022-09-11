use std::collections::HashSet;
use std::error::Error;
use std::sync::Mutex;
use std::time::Duration;

use lazy_static::lazy_static;

use serenity::async_trait;
use serenity::builder::{CreateActionRow, CreateEmbed};
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
use serenity::model::user::User;
use serenity::prelude::{GatewayIntents, Mentionable};
use serenity::utils::Color;

use tracing::{error, info};

const ROCK: char = '\u{270A}';
const PAPER: char = '\u{270B}';
const SCISSORS: char = '\u{270C}';

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
        if !ctx.http.get_global_application_commands().await.unwrap().iter().any(|cmd| cmd.name == "rps") {
            Command::create_global_application_command(&ctx.http, |cmd| {
                cmd
                    .name("rps")
                    .description("Starts the rock-paper-scissors game against the specified user")
                    .create_option(|option| {
                        option
                            .name("opponent")
                            .description("The user to play rock-paper-scissors against")
                            .kind(CommandOptionType::User)
                            .required(true)
                    })
            }).await.expect("The rock-paper-scissors command could not have been registered!");

            info!("The rock-paper-scissors command has been registered!");
        }

        info!("Connected to Discord!");
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(cmd) = interaction {
            if cmd.data.name == "rps" {
                let option = &cmd.data.options[0];

                if let Some(CommandDataOptionValue::User(opponent, _)) = &option.resolved {
                    let starter = &cmd.user;

                    if opponent.bot || opponent.id == starter.id {
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

                    if SESSIONS.lock().unwrap().iter().any(|(u, _)| u == starter.id.as_u64() || u == opponent.id.as_u64()) {
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
                                                .description("Either user is already playing rock-paper-scissors!")
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
                                                format!("Do you want to play rock-paper-scissors against {}?", starter.mention())
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
                            (*starter.id.as_u64(), *response.id.as_u64()),
                            (*opponent.id.as_u64(), *response.id.as_u64()),
                        ]);

                        let mut round_counter = 1usize;

                        let round_embed = |user: &User, round_counter: usize| {
                            let mut embed = CreateEmbed::default();

                            embed
                                .color(SUCCESS_COLOR)
                                .author(|author| {
                                    author
                                        .name(format!("Round #{}", round_counter))
                                        .icon_url(
                                            user.avatar_url()
                                                .unwrap_or_else(|| user.default_avatar_url())
                                        )
                                })
                                .description(format!("It is {}'s turn!", user.mention()));

                            embed
                        };

                        let turn_action_row = |users: Vec<String>| {
                            let ids_joined = users.join("-");

                            let mut row = CreateActionRow::default();

                            row
                                .create_button(|button| {
                                    button
                                        .style(ButtonStyle::Secondary)
                                        .emoji(ROCK)
                                        .custom_id(format!("{}-rock", ids_joined))
                                })
                                .create_button(|button| {
                                    button
                                        .style(ButtonStyle::Secondary)
                                        .emoji(PAPER)
                                        .custom_id(format!("{}-paper", ids_joined))
                                })
                                .create_button(|button| {
                                    button
                                        .style(ButtonStyle::Secondary)
                                        .emoji(SCISSORS)
                                        .custom_id(format!("{}-scissors", ids_joined))
                                })
                                .create_button(|button| {
                                    button
                                        .style(ButtonStyle::Danger)
                                        .label("Exit")
                                        .custom_id("stop")
                                });

                            row
                        };

                        let mut interaction_stream = response.await_component_interactions(&ctx)
                            .filter(|i| i.data.component_type == ComponentType::Button)
                            .timeout(Duration::from_secs(60 * 5))
                            .build();

                        while let Some(interaction) = interaction_stream.next().await {
                            let id: Vec<_> = interaction.data.custom_id.split('-').collect();
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
                                                            .components(|comp| comp.set_action_row(turn_action_row(vec![starter.id.to_string()])))
                                                            .content("")
                                                            .set_embed(round_embed(starter, round_counter))
                                                    })
                                            }).await {}
                                        } else {
                                            if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                response
                                                    .kind(InteractionResponseType::UpdateMessage)
                                                    .interaction_response_data(|msg| {
                                                        msg
                                                            .components(|comp| comp)
                                                            .content(starter.mention())
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
                                            sessions.remove(&(*starter.id.as_u64(), *response.id.as_u64()));

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
                                        if interaction.user.id == starter.id {
                                            if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                response
                                                    .kind(InteractionResponseType::UpdateMessage)
                                                    .interaction_response_data(|msg| {
                                                        msg
                                                            .components(|comp| comp.set_action_row(turn_action_row(vec![opponent.id.to_string(), suffix.to_string()])))
                                                            .set_embed(round_embed(opponent, round_counter))
                                                    })
                                            }).await {}
                                        } else {
                                            let starter_turn = id[1];
                                            let opponent_turn = suffix;

                                            let winner = match starter_turn {
                                                "rock" => match opponent_turn {
                                                    "rock" => None,
                                                    "paper" => Some(opponent),
                                                    _ => Some(starter),
                                                },
                                                "paper" => match opponent_turn {
                                                    "rock" => Some(starter),
                                                    "paper" => None,
                                                    _ => Some(opponent),
                                                },
                                                _ => match opponent_turn {
                                                    "rock" => Some(opponent),
                                                    "paper" => Some(starter),
                                                    _ => None,
                                                },
                                            };

                                            if let Some(winner) = winner {
                                                if let Err(_) = interaction.create_interaction_response(&ctx.http, |response| {
                                                    response
                                                        .kind(InteractionResponseType::UpdateMessage)
                                                        .interaction_response_data(|msg| {
                                                            let winner_turn = if winner.id == starter.id {
                                                                starter_turn
                                                            } else {
                                                                opponent_turn
                                                            };

                                                            let (loser, loser_turn) = if winner.id == starter.id {
                                                                (opponent, opponent_turn)
                                                            } else {
                                                                (starter, starter_turn)
                                                            };

                                                            let formatted_turn = |turn| match turn {
                                                                "rock" => format!("{} Rock", ROCK),
                                                                "paper" => format!("{} Paper", PAPER),
                                                                _ => format!("{} Scissors", SCISSORS),
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
                                                                        .description(format!("{} defeats {}!", winner.mention(), loser.mention()))
                                                                        .field("Winner's Turn", formatted_turn(winner_turn), false)
                                                                        .field("Loser's Turn", formatted_turn(loser_turn), false)
                                                                })
                                                        })
                                                }).await {}

                                                let mut sessions = SESSIONS.lock().unwrap();

                                                sessions.remove(&(*opponent.id.as_u64(), *response.id.as_u64()));
                                                sessions.remove(&(*starter.id.as_u64(), *response.id.as_u64()));

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
                                                                                    .emoji(ROCK)
                                                                                    .custom_id(format!("{}-rock", starter.id))
                                                                            })
                                                                            .create_button(|button| {
                                                                                button
                                                                                    .style(ButtonStyle::Secondary)
                                                                                    .emoji(PAPER)
                                                                                    .custom_id(format!("{}-paper", starter.id))
                                                                            })
                                                                            .create_button(|button| {
                                                                                button
                                                                                    .style(ButtonStyle::Secondary)
                                                                                    .emoji(SCISSORS)
                                                                                    .custom_id(format!("{}-scissors", starter.id))
                                                                            })
                                                                            .create_button(|button| {
                                                                                button
                                                                                    .style(ButtonStyle::Danger)
                                                                                    .label("Exit")
                                                                                    .custom_id("stop")
                                                                            })
                                                                    })
                                                                })
                                                                .set_embed(round_embed(starter, round_counter))
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
                                                                .description(if id[0] != starter.id.to_string().as_str()
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
                                    sessions.remove(&(*starter.id.as_u64(), *response.id.as_u64()));

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
        std::env::set_var("RUST_LOG", "DEBUG");

        tracing_subscriber::fmt::init();

        info!("Starting!");
    }

    let token = std::env::var("DISCORD_TOKEN")?;
    let intents = GatewayIntents::all();

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .await?;

    if let Err(err) = client.start().await {
        error!("An error occurred while running the client: {:?}", err);
    }

    Ok(())
}