use crate::db::{ChatGroupDoc, ConversationDoc, Db, MessageDoc};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use futures::stream::TryStreamExt;
use mongodb::{
    bson::{doc, oid::ObjectId},
    options::FindOptions,
};
use serde::Serialize;

pub const MAX_BODY_CHARS: usize = 8000;
const HISTORY_LIMIT: i64 = 200;
const CONVERSATION_LIMIT: i64 = 100;
const GROUP_LIMIT: i64 = 100;
const MAX_GROUP_NAME_CHARS: usize = 80;
const MAX_DATA_TEXT_CHARS: usize = 50_000;

#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: String,
    pub group_id: Option<String>,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<ConversationDoc> for Conversation {
    type Error = anyhow::Error;

    fn try_from(doc: ConversationDoc) -> Result<Self> {
        Ok(Self {
            id: doc
                .id
                .ok_or_else(|| anyhow!("conversation missing id"))?
                .to_hex(),
            group_id: doc.group_id.map(|id| id.to_hex()),
            title: doc.title,
            created_at: doc.created_at,
            updated_at: doc.updated_at,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<MessageDoc> for Message {
    type Error = anyhow::Error;

    fn try_from(doc: MessageDoc) -> Result<Self> {
        Ok(Self {
            id: doc
                .id
                .ok_or_else(|| anyhow!("message missing id"))?
                .to_hex(),
            role: doc.role,
            body: doc.body,
            created_at: doc.created_at,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatGroup {
    pub id: String,
    pub name: String,
    pub data_text: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<ChatGroupDoc> for ChatGroup {
    type Error = anyhow::Error;

    fn try_from(doc: ChatGroupDoc) -> Result<Self> {
        Ok(Self {
            id: doc
                .id
                .ok_or_else(|| anyhow!("chat group missing id"))?
                .to_hex(),
            name: doc.name,
            data_text: doc.data_text,
            created_at: doc.created_at,
            updated_at: doc.updated_at,
        })
    }
}

pub async fn list_conversations(db: &Db, user_id: ObjectId) -> Result<Vec<Conversation>> {
    let options = FindOptions::builder()
        .sort(doc! { "updated_at": -1 })
        .limit(CONVERSATION_LIMIT)
        .build();
    let mut cursor = db
        .conversations()
        .find(doc! { "user_id": user_id }, options)
        .await
        .context("failed to list conversations")?;

    let mut out: Vec<Conversation> = Vec::new();
    while let Some(doc) = cursor.try_next().await.context("cursor error")? {
        out.push(Conversation::try_from(doc)?);
    }
    Ok(out)
}

pub async fn create_conversation(
    db: &Db,
    user_id: ObjectId,
    group_id: Option<ObjectId>,
) -> Result<Conversation> {
    let now = Utc::now();
    let doc = ConversationDoc {
        id: None,
        user_id,
        group_id,
        title: "New chat".to_owned(),
        created_at: now,
        updated_at: now,
    };
    let inserted = db
        .conversations()
        .insert_one(&doc, None)
        .await
        .context("failed to insert conversation")?;
    let id = inserted
        .inserted_id
        .as_object_id()
        .ok_or_else(|| anyhow!("inserted conversation missing id"))?;
    let mut stored = doc;
    stored.id = Some(id);
    Conversation::try_from(stored)
}

pub async fn list_chat_groups(db: &Db, user_id: ObjectId) -> Result<Vec<ChatGroup>> {
    let options = FindOptions::builder()
        .sort(doc! { "updated_at": -1 })
        .limit(GROUP_LIMIT)
        .build();
    let mut cursor = db
        .chat_groups()
        .find(doc! { "user_id": user_id }, options)
        .await
        .context("failed to list chat groups")?;

    let mut out = Vec::new();
    while let Some(doc) = cursor.try_next().await.context("cursor error")? {
        out.push(ChatGroup::try_from(doc)?);
    }
    Ok(out)
}

pub async fn load_chat_group(
    db: &Db,
    user_id: ObjectId,
    group_id: ObjectId,
) -> Result<Option<ChatGroup>> {
    let Some(group) = db
        .chat_groups()
        .find_one(doc! { "_id": group_id, "user_id": user_id }, None)
        .await
        .context("failed to load chat group")?
    else {
        return Ok(None);
    };

    ChatGroup::try_from(group).map(Some)
}

pub async fn create_chat_group(
    db: &Db,
    user_id: ObjectId,
    name: &str,
    data_text: &str,
) -> Result<ChatGroup> {
    let now = Utc::now();
    let doc = ChatGroupDoc {
        id: None,
        user_id,
        name: normalize_group_name(name),
        data_text: validate_data_text(data_text)?,
        created_at: now,
        updated_at: now,
    };
    let inserted = db
        .chat_groups()
        .insert_one(&doc, None)
        .await
        .context("failed to insert chat group")?;
    let id = inserted
        .inserted_id
        .as_object_id()
        .ok_or_else(|| anyhow!("inserted chat group missing id"))?;
    let mut stored = doc;
    stored.id = Some(id);
    ChatGroup::try_from(stored)
}

pub async fn update_chat_group(
    db: &Db,
    user_id: ObjectId,
    group_id: ObjectId,
    name: &str,
    data_text: &str,
) -> Result<Option<ChatGroup>> {
    let name = normalize_group_name(name);
    let data_text = validate_data_text(data_text)?;
    let now = Utc::now();
    let result = db
        .chat_groups()
        .update_one(
            doc! { "_id": group_id, "user_id": user_id },
            doc! {
                "$set": {
                    "name": &name,
                    "data_text": &data_text,
                    "updated_at": bson::DateTime::from_chrono(now),
                }
            },
            None,
        )
        .await
        .context("failed to update chat group")?;

    if result.matched_count == 0 {
        return Ok(None);
    }

    let Some(group) = db
        .chat_groups()
        .find_one(doc! { "_id": group_id, "user_id": user_id }, None)
        .await
        .context("failed to reload chat group")?
    else {
        return Ok(None);
    };

    ChatGroup::try_from(group).map(Some)
}

pub async fn delete_chat_group(db: &Db, user_id: ObjectId, group_id: ObjectId) -> Result<bool> {
    let result = db
        .chat_groups()
        .delete_one(doc! { "_id": group_id, "user_id": user_id }, None)
        .await
        .context("failed to delete chat group")?;
    if result.deleted_count == 0 {
        return Ok(false);
    }
    // Unlink any conversations that pointed at this group.
    let _ = db
        .conversations()
        .update_many(
            doc! { "user_id": user_id, "group_id": group_id },
            doc! { "$unset": { "group_id": "" } },
            None,
        )
        .await;
    Ok(true)
}

pub async fn set_conversation_group(
    db: &Db,
    user_id: ObjectId,
    conversation_id: ObjectId,
    group_id: Option<ObjectId>,
) -> Result<Option<Conversation>> {
    let now = Utc::now();
    let update = match group_id {
        Some(group_id) => doc! {
            "$set": {
                "group_id": group_id,
                "updated_at": bson::DateTime::from_chrono(now),
            }
        },
        None => doc! {
            "$unset": { "group_id": "" },
            "$set": { "updated_at": bson::DateTime::from_chrono(now) },
        },
    };

    let result = db
        .conversations()
        .update_one(
            doc! { "_id": conversation_id, "user_id": user_id },
            update,
            None,
        )
        .await
        .context("failed to set conversation group")?;
    if result.matched_count == 0 {
        return Ok(None);
    }

    let Some(convo) = db
        .conversations()
        .find_one(doc! { "_id": conversation_id, "user_id": user_id }, None)
        .await
        .context("failed to reload conversation")?
    else {
        return Ok(None);
    };

    Conversation::try_from(convo).map(Some)
}

pub async fn load_conversation(
    db: &Db,
    user_id: ObjectId,
    conversation_id: ObjectId,
) -> Result<Option<(Conversation, Vec<Message>)>> {
    let Some(convo) = db
        .conversations()
        .find_one(doc! { "_id": conversation_id, "user_id": user_id }, None)
        .await
        .context("failed to load conversation")?
    else {
        return Ok(None);
    };

    let options = FindOptions::builder()
        .sort(doc! { "created_at": 1 })
        .limit(HISTORY_LIMIT)
        .build();
    let mut cursor = db
        .messages()
        .find(doc! { "conversation_id": conversation_id }, options)
        .await
        .context("failed to load messages")?;

    let mut messages: Vec<Message> = Vec::new();
    while let Some(doc) = cursor.try_next().await.context("cursor error")? {
        messages.push(Message::try_from(doc)?);
    }

    Ok(Some((Conversation::try_from(convo)?, messages)))
}

pub async fn delete_conversation(
    db: &Db,
    user_id: ObjectId,
    conversation_id: ObjectId,
) -> Result<bool> {
    let result = db
        .conversations()
        .delete_one(doc! { "_id": conversation_id, "user_id": user_id }, None)
        .await
        .context("failed to delete conversation")?;
    if result.deleted_count == 0 {
        return Ok(false);
    }
    let _ = db
        .messages()
        .delete_many(doc! { "conversation_id": conversation_id }, None)
        .await;
    Ok(true)
}

pub async fn rename_conversation(
    db: &Db,
    user_id: ObjectId,
    conversation_id: ObjectId,
    title: &str,
) -> Result<bool> {
    let title = title.trim();
    if title.is_empty() {
        return Err(anyhow!("title cannot be empty"));
    }
    let title = if title.chars().count() > 80 {
        title.chars().take(80).collect::<String>()
    } else {
        title.to_owned()
    };
    let now = Utc::now();
    let result = db
        .conversations()
        .update_one(
            doc! { "_id": conversation_id, "user_id": user_id },
            doc! { "$set": { "title": &title, "updated_at": bson::DateTime::from_chrono(now) } },
            None,
        )
        .await
        .context("failed to rename conversation")?;
    Ok(result.matched_count > 0)
}

pub async fn append_message(
    db: &Db,
    conversation_id: ObjectId,
    role: &str,
    body: &str,
) -> Result<Message> {
    let doc = MessageDoc {
        id: None,
        conversation_id,
        role: role.to_owned(),
        body: body.to_owned(),
        created_at: Utc::now(),
    };
    let inserted = db
        .messages()
        .insert_one(&doc, None)
        .await
        .context("failed to insert message")?;
    let id = inserted
        .inserted_id
        .as_object_id()
        .ok_or_else(|| anyhow!("inserted message missing id"))?;

    let now = Utc::now();
    let _ = db
        .conversations()
        .update_one(
            doc! { "_id": conversation_id },
            doc! { "$set": { "updated_at": bson::DateTime::from_chrono(now) } },
            None,
        )
        .await;

    let mut stored = doc;
    stored.id = Some(id);
    Message::try_from(stored)
}

pub async fn set_generated_title_if_new(
    db: &Db,
    conversation_id: ObjectId,
    generated_title: &str,
) -> Result<Option<String>> {
    let existing = db
        .conversations()
        .find_one(doc! { "_id": conversation_id }, None)
        .await
        .context("failed to load conversation for generated title")?;
    let Some(c) = existing else { return Ok(None) };
    if c.title != "New chat" {
        return Ok(None);
    }

    let title = generated_title.trim();
    if title.is_empty() {
        return Ok(None);
    }

    let title = if title.chars().count() > 80 {
        title.chars().take(80).collect::<String>()
    } else {
        title.to_owned()
    };

    let _ = db
        .conversations()
        .update_one(
            doc! { "_id": conversation_id },
            doc! { "$set": { "title": &title } },
            None,
        )
        .await;

    Ok(Some(title))
}

pub fn validate_message_body(body: &str) -> Result<String> {
    let trimmed = body.trim().to_owned();
    if trimmed.is_empty() {
        return Err(anyhow!("message body cannot be empty"));
    }
    if trimmed.chars().count() > MAX_BODY_CHARS {
        return Err(anyhow!(
            "message too long (max {MAX_BODY_CHARS} characters)"
        ));
    }
    Ok(trimmed)
}

fn normalize_group_name(name: &str) -> String {
    let trimmed = name.trim();
    let value = if trimmed.is_empty() {
        "New company"
    } else {
        trimmed
    };
    value.chars().take(MAX_GROUP_NAME_CHARS).collect()
}

fn validate_data_text(data_text: &str) -> Result<String> {
    if data_text.chars().count() > MAX_DATA_TEXT_CHARS {
        return Err(anyhow!(
            "data_text too long (max {MAX_DATA_TEXT_CHARS} characters)"
        ));
    }
    Ok(data_text.to_owned())
}
