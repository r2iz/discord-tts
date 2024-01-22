use serenity::{
    all::{CommandInteraction, CommandOptionType},
    builder::{CreateCommand, CreateCommandOption},
    client::Context,
};

use crate::db::PERSISTENT_DB;

use super::simple_resp_helper;

pub fn register(prefix: &str) -> CreateCommand {
    CreateCommand::new(format!("{prefix}dict"))
        .description("Dictionary utils")
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "add",
                "Add a word to the dictionary",
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "word",
                    "word before replacement",
                )
                .required(true),
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "replacement",
                    "word after replacement",
                )
                .required(true),
            ),
        )
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "remove",
                "Remove a word from the dictionary",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "word", "Word to remove")
                    .required(true),
            ),
        )
}

pub async fn run(ctx: &Context, interaction: CommandInteraction) {
    // let a = CommandDataOption {
    //     name: "add",
    //     value: SubCommand([
    //         CommandDataOption {
    //             name: "word",
    //             value: String("a"),
    //         },
    //         CommandDataOption {
    //             name: "replacement",
    //             value: String("b"),
    //         },
    //     ]),
    // };

    let option = &interaction.data.options.first().unwrap();

    match option.name.as_str() {
        "add" => match &option.value {
            serenity::all::CommandDataOptionValue::SubCommand(c) => {
                let mut data = c.iter();
                let key = data.next().unwrap().value.as_str().unwrap();
                let value = data.next().unwrap().value.as_str().unwrap();
                PERSISTENT_DB.store_dictionary_word(key, value);
                simple_resp_helper(
                    &interaction,
                    ctx,
                    format!("Added {key} => {value}").as_str(),
                    false,
                )
                .await;
            }
            _ => simple_resp_helper(&interaction, ctx, "Unknown Error", true).await,
        },
        "remove" => match &option.value {
            serenity::all::CommandDataOptionValue::SubCommand(c) => {
                let key = c.first().unwrap().value.as_str().unwrap();
                PERSISTENT_DB.remove_dictionary_word(key);
                simple_resp_helper(&interaction, ctx, format!("Removed {key}").as_str(), false)
                    .await;
            }
            _ => simple_resp_helper(&interaction, ctx, "Unknown Error", true).await,
        },
        _ => simple_resp_helper(&interaction, ctx, "Unknown Error", true).await,
    }
}
