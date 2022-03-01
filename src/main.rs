use teloxide::{
    dispatching2::dialogue::{serializer::Json, RedisStorage, Storage},
    macros::DialogueState,
    payloads::{SendContact, SendMessageSetters},
    prelude2::*,
    types::{Contact, Me},
    utils::command::BotCommand,
    RequestError,
};
use thiserror::Error;

type MyDialogue = Dialogue<State, RedisStorage<Json>>;
type StorageError = <RedisStorage<Json> as Storage<State>>::Error;

const FORWARD_REPORTS_TO_CHAT_ID: i64 = -701411482;

#[derive(Debug, Error)]
enum Error {
    #[error("error from Telegram: {0}")]
    TelegramError(#[from] RequestError),

    #[error("error from storage: {0}")]
    StorageError(#[from] StorageError),
}

#[derive(DialogueState, Debug, Clone, serde::Serialize, serde::Deserialize)]
#[handler_out(anyhow::Result<()>)]
pub enum State {
    #[handler(handle_start)]
    Start,

    #[handler(handle_verified)]
    Verified {
        contact: teloxide::types::Contact,
        last_post: Option<chrono::DateTime<chrono::Utc>>,
    },
}

impl Default for State {
    fn default() -> Self {
        Self::Start
    }
}

#[derive(BotCommand)]
#[command(rename = "lowercase", description = "These commands are supported:")]
pub enum Command {
    #[command(description = "get your number.")]
    Get,
    #[command(description = "reset your number.")]
    Reset,
}
#[tokio::main]
async fn main() {
    teloxide::enable_logging!();
    log::info!("Starting dialogue_bot...");

    let bot = Bot::from_env().auto_send();
    // You can also choose serializer::JSON or serializer::CBOR
    // All serializers but JSON require enabling feature
    // "serializer-<name>", e. g. "serializer-cbor"
    // or "serializer-bincode"
    let storage = RedisStorage::open("redis://127.0.0.1:6379", Json)
        .await
        .unwrap();

    let handler = Update::filter_message()
        .enter_dialogue::<Message, RedisStorage<Json>, State>()
        .dispatch_by::<State>();

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![storage])
        .build()
        .setup_ctrlc_handler()
        .dispatch()
        .await;
}

fn request_phone_number_confirmation_keyboard() -> teloxide::types::KeyboardMarkup {
    teloxide::types::KeyboardMarkup::new(vec![vec![teloxide::types::KeyboardButton::new(
        "Підтвердити мій номер телефону",
    )
    .request(teloxide::types::ButtonRequest::Contact)]])
}

async fn handle_start(
    bot: AutoSend<Bot>,
    msg: Message,
    dialogue: MyDialogue,
) -> anyhow::Result<()> {
    if !msg.chat.is_private() {
        return Ok(());
    }
    println!("{:#?}", msg);
    match msg.contact() {
        Some(contact) => {
            if contact.user_id.map(|user_id| i64::from(user_id)) != Some(msg.chat.id) {
                bot.send_message(msg.chat.id, "Відправте свій контакт.")
                    .reply_markup(request_phone_number_confirmation_keyboard())
                    .await?;
                return Ok(());
            }
            if !contact.phone_number.starts_with("380") {
                bot.send_message(msg.chat.id, "Нажаль ми можемо підтвердити лише користувачів з українським номером телефону.").reply_markup(request_phone_number_confirmation_keyboard()).await?;
                return Ok(());
            }
            dialogue
                .update(State::Verified {
                    contact: contact.clone(),
                    last_post: None,
                })
                .await?;
            bot.send_message(
                msg.chat.id,
                format!("Ваш номер {} підтверджено. Надсилайте нам відео та фото фіксації руйнуваннь в наслідок агресії РФ. В комментарі зазначте район (не треба вказувати точну адресу!)", contact.phone_number),
            ).reply_markup(teloxide::types::KeyboardRemove::new())
            .await?;
        }
        _ => {
            bot.send_message(
                msg.chat.id,
                "Натисніть \"Підтвердити мій номер телефону\" щоб продовжити.",
            )
            .reply_markup(request_phone_number_confirmation_keyboard())
            .await?;
        }
    }

    Ok(())
}

async fn handle_verified(
    bot: AutoSend<Bot>,
    msg: Message,
    dialogue: MyDialogue,
    (contact, last_post): (
        teloxide::types::Contact,
        Option<chrono::DateTime<chrono::Utc>>,
    ),
    //me: Me,
) -> anyhow::Result<()> {
    println!("{:?}: {:#?}", contact, msg);

    /*
    if let Some(last_post) = last_post {
        if chrono::Utc::now().signed_duration_since(last_post) < chrono::Duration::minutes(1) {
            bot.send_message(msg.chat.id, "Не треба відправляти повідомлення частіше ніш раз на хвилину. Слава Україні!").reply_to_message_id(msg.id).await?;
            return Ok(());
        }
    }
    */

    let forwarded_msg = bot
        .forward_message(FORWARD_REPORTS_TO_CHAT_ID, msg.chat.id, msg.id)
        .await?;
    bot.send_message(
        FORWARD_REPORTS_TO_CHAT_ID,
        format!("Reported by {:?}", contact),
    )
    .reply_to_message_id(forwarded_msg.id)
    .await?;
    dialogue
        .update(State::Verified {
            contact,
            last_post: Some(chrono::Utc::now()),
        })
        .await?;
    bot.send_message(msg.chat.id, "Ми отримали інформацію! Слава Україні!")
        .reply_to_message_id(msg.id)
        .await?;

    Ok(())
}
