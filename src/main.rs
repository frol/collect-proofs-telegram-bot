use teloxide::{
    dispatching2::dialogue::{serializer::Json, RedisStorage, Storage},
    macros::DialogueState,
    payloads::SendMessageSetters,
    prelude2::*,
    types::Me,
    utils::command::BotCommand,
    RequestError,
};
use thiserror::Error;

type MyDialogue = Dialogue<State, RedisStorage<Json>>;
type StorageError = <RedisStorage<Json> as Storage<State>>::Error;

const FORWARD_REPORTS_TO_CHAT_ID: i64 = -1001648966128;

const HELP_TEXT: &str = r#"Миру нам всім!

Бот розроблено Харківським ІТ суспілсьтвом разом із волонтерами задля збору медіа руйнувань Харкову. Ця інформація буде використовуватися  задля донесення до усього світу та Російських громадян, як Росія знищує Харків.

Наразі бот має лише одну команду:
/add - Додати докази (відео та фото фіксації) 📷"#;

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

    #[handler(handle_ready_to_receive_media)]
    ReadyToReceiveMedia { contact: teloxide::types::Contact },

    #[handler(handle_ready_to_receive_comment)]
    ReadyToReceiveComment {
        contact: teloxide::types::Contact,
        media_msg_ids: Vec<i32>,
    },

    #[handler(handle_awaiting_confirmation)]
    AwaitingConfirmation {
        contact: teloxide::types::Contact,
        media_msg_ids: Vec<i32>,
        comment: String,
    },
}

impl Default for State {
    fn default() -> Self {
        Self::Start
    }
}

#[derive(BotCommand)]
#[command(rename = "lowercase", description = "Допустимі команди для бота:")]
pub enum Command {
    #[command(description = "Почніть роботу з ботом")]
    Start,
    #[command(description = "Додати матеріали про нові руйнування")]
    Add,
    #[command(description = "Почати знов")]
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
    log::info!("start: {:#?}", msg);
    if !msg.chat.is_private() {
        return Ok(());
    }
    match msg.contact() {
        Some(contact) => {
            if contact.user_id.map(i64::from) != Some(msg.chat.id) {
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
                .update(State::ReadyToReceiveMedia {
                    contact: contact.clone(),
                })
                .await?;
            bot.send_message(
                msg.chat.id,
                format!("Ваш номер {} підтверджено. Надсилайте нам відео та фото фіксації руйнуваннь внаслідок агресії РФ. В комментарі зазначте район (не треба вказувати точну адресу!)", contact.phone_number),
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

async fn handle_ready_to_receive_media(
    bot: AutoSend<Bot>,
    msg: Message,
    dialogue: MyDialogue,
    (contact,): (teloxide::types::Contact,),
    me: Me,
) -> anyhow::Result<()> {
    if let Some(text_msg) = msg.text() {
        let bot_name = me.user.username.unwrap();

        match Command::parse(text_msg, bot_name) {
            Ok(Command::Start) => {
                bot.send_message(msg.chat.id, HELP_TEXT).await?;
                return Ok(());
            }
            Ok(Command::Add | Command::Reset) => {
                bot.send_message(
                    msg.chat.id,
                    "Надсилайте нам відео та фото фіксації руйнуваннь внаслідок агресії РФ.",
                )
                .await?;
                return Ok(());
            }
            Err(_) => {}
        }
    }
    log::info!("ready_to_receive_media: {:?}: {:#?}", contact, msg);

    /*
    if let Some(last_post) = last_post {
        if chrono::Utc::now().signed_duration_since(last_post) < chrono::Duration::minutes(1) {
            bot.send_message(msg.chat.id, "Не треба відправляти повідомлення частіше ніш раз на хвилину. Слава Україні!").reply_to_message_id(msg.id).await?;
            return Ok(());
        }
    }
    */

    match msg.kind {
        teloxide::types::MessageKind::Common(teloxide::types::MessageCommon {
            media_kind:
                teloxide::types::MediaKind::Video(teloxide::types::MediaVideo { .. })
                | teloxide::types::MediaKind::Photo(teloxide::types::MediaPhoto { .. }),
            ..
        }) => {
            dialogue
                .update(State::ReadyToReceiveComment {
                    contact: contact.clone(),
                    media_msg_ids: vec![msg.id],
                })
                .await?;

            bot.send_message(msg.chat.id, "Відправте текстове повідомлення з комментарем з зазначенням району (не треба вказувати точну адресу!)")
                .reply_to_message_id(msg.id)
                .await?;
        }
        _ => {
            bot.send_message(msg.chat.id, "Відправляйте нам лише фото або відео.")
                .reply_to_message_id(msg.id)
                .await?;
        }
    }
    Ok(())
}

async fn handle_ready_to_receive_comment(
    bot: AutoSend<Bot>,
    msg: Message,
    dialogue: MyDialogue,
    (contact, mut media_msg_ids): (teloxide::types::Contact, Vec<i32>),
    me: Me,
) -> anyhow::Result<()> {
    if let Some(text_msg) = msg.text() {
        let bot_name = me.user.username.unwrap();

        match Command::parse(text_msg, bot_name) {
            Ok(Command::Start) => {
                bot.send_message(msg.chat.id, HELP_TEXT).await?;
                return Ok(());
            }
            Ok(Command::Add) => {
                bot.send_message(
                    msg.chat.id,
                    "Надсилайте нам відео та фото фіксації руйнуваннь внаслідок агресії РФ.",
                )
                .await?;
                return Ok(());
            }
            Ok(Command::Reset) => {
                bot.send_message(
                    msg.chat.id,
                    "Надсилайте нам відео та фото фіксації руйнуваннь внаслідок агресії РФ.",
                )
                .await?;
                dialogue
                    .update(State::ReadyToReceiveMedia { contact })
                    .await?;
                return Ok(());
            }
            Err(_) => {}
        }
    }
    log::info!("read_to_receive_comment: {:?}: {:#?}", contact, msg);

    /*
    if let Some(last_post) = last_post {
        if chrono::Utc::now().signed_duration_since(last_post) < chrono::Duration::minutes(1) {
            bot.send_message(msg.chat.id, "Не треба відправляти повідомлення частіше ніш раз на хвилину. Слава Україні!").reply_to_message_id(msg.id).await?;
            return Ok(());
        }
    }
    */

    match msg.kind {
        teloxide::types::MessageKind::Common(teloxide::types::MessageCommon {
            media_kind: teloxide::types::MediaKind::Video(_) | teloxide::types::MediaKind::Photo(_),
            ..
        }) => {
            media_msg_ids.push(msg.id);
            dialogue
                .update(State::ReadyToReceiveComment {
                    contact: contact.clone(),
                    media_msg_ids,
                })
                .await?;

            bot.send_message(msg.chat.id, "Відправте текстове повідомлення з комментарем з зазначенням району (не треба вказувати точну адресу!)")
                .reply_to_message_id(msg.id)
                .await?;
        }
        teloxide::types::MessageKind::Common(teloxide::types::MessageCommon {
            media_kind: teloxide::types::MediaKind::Text(teloxide::types::MediaText { text, .. }),
            ..
        }) => {
            dialogue
                .update(State::AwaitingConfirmation {
                    contact: contact.clone(),
                    media_msg_ids,
                    comment: text,
                })
                .await?;

            bot.send_message(
                msg.chat.id,
                "Відправити додані фото/відео та ваш комментар на перевірку?",
            )
            .reply_to_message_id(msg.id)
            .reply_markup(teloxide::types::KeyboardMarkup::new(vec![vec![
                teloxide::types::KeyboardButton::new("Так, відправте мої фото/відео на перевірку"),
                teloxide::types::KeyboardButton::new("Ні, почати знов"),
            ]]))
            .await?;
        }
        _ => {
            bot.send_message(
                msg.chat.id,
                "Відправляйте нам лише фото або відео або коментар текстовим повідомленням.",
            )
            .reply_to_message_id(msg.id)
            .await?;
        }
    }
    Ok(())
}

async fn handle_awaiting_confirmation(
    bot: AutoSend<Bot>,
    msg: Message,
    dialogue: MyDialogue,
    (contact, media_msg_ids, comment): (teloxide::types::Contact, Vec<i32>, String),
    me: Me,
) -> anyhow::Result<()> {
    if let Some(text_msg) = msg.text() {
        let bot_name = me.user.username.unwrap();

        match Command::parse(text_msg, bot_name) {
            Ok(Command::Start) => {
                bot.send_message(msg.chat.id, HELP_TEXT).await?;
                return Ok(());
            }
            Ok(Command::Add) => {
                bot.send_message(
                    msg.chat.id,
                    "Надсилайте нам відео та фото фіксації руйнуваннь внаслідок агресії РФ.",
                )
                .await?;
                return Ok(());
            }
            Ok(Command::Reset) => {
                bot.send_message(
                    msg.chat.id,
                    "Надсилайте нам відео та фото фіксації руйнуваннь внаслідок агресії РФ.",
                )
                .await?;
                dialogue
                    .update(State::ReadyToReceiveMedia { contact })
                    .await?;
                return Ok(());
            }
            Err(_) => {}
        }
    }
    log::info!("awaiting_for_confirmation: {:?}: {:#?}", contact, msg);

    /*
    if let Some(last_post) = last_post {
        if chrono::Utc::now().signed_duration_since(last_post) < chrono::Duration::minutes(1) {
            bot.send_message(msg.chat.id, "Не треба відправляти повідомлення частіше ніш раз на хвилину. Слава Україні!").reply_to_message_id(msg.id).await?;
            return Ok(());
        }
    }
    */

    match msg.text() {
        Some("Так, відправте мої фото/відео на перевірку") => {
            bot.send_message(
                FORWARD_REPORTS_TO_CHAT_ID,
                format!("Reported by {:?}:\n{}", contact, comment),
            )
            .await?;
            for media_msg_id in media_msg_ids {
                bot.forward_message(FORWARD_REPORTS_TO_CHAT_ID, msg.chat.id, media_msg_id)
                    .await?;
            }

            dialogue
                .update(State::ReadyToReceiveMedia { contact })
                .await?;

            bot.send_message(
                msg.chat.id,
                "Ми отримали інформацію! Слава Україні! Щоб додати ще, відправте /add",
            )
            .reply_to_message_id(msg.id)
            .reply_markup(teloxide::types::KeyboardRemove::new())
            .await?;
        }

        Some("Ні, почати знов") => {
            dialogue
                .update(State::ReadyToReceiveMedia { contact })
                .await?;
            bot.send_message(
                msg.chat.id,
                "Надсилайте нам відео та фото фіксації руйнуваннь внаслідок агресії РФ.",
            )
            .reply_to_message_id(msg.id)
            .reply_markup(teloxide::types::KeyboardRemove::new())
            .await?;
        }

        _ => {
            bot.send_message(
                msg.chat.id,
                "Відправте \"Так, відправте мої фото/відео на перевірку\" чи \"Ні, почати знов\"",
            )
            .reply_to_message_id(msg.id)
            .await?;
        }
    }

    Ok(())
}
