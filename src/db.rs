use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use mongodb::{
    Client, Collection, Database, IndexModel,
    bson::{doc, oid::ObjectId},
    options::{ClientOptions, IndexOptions},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod bson_datetime {
    use super::*;

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    enum StoredDateTime {
        Bson(mongodb::bson::DateTime),
        Millis(i64),
    }

    pub fn serialize<S>(value: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        mongodb::bson::DateTime::from_chrono(*value).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match StoredDateTime::deserialize(deserializer)? {
            StoredDateTime::Bson(value) => Ok(value.to_chrono()),
            StoredDateTime::Millis(value) => Utc
                .timestamp_millis_opt(value)
                .single()
                .ok_or_else(|| serde::de::Error::custom("invalid unix timestamp in milliseconds")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Db {
    inner: Database,
}

impl Db {
    pub async fn connect(uri: &str, db_name: &str) -> Result<Self> {
        let mut options = ClientOptions::parse(uri)
            .await
            .with_context(|| format!("invalid MONGODB_URI `{uri}`"))?;
        options.app_name = Some("win-win".to_owned());

        let client = Client::with_options(options).context("failed to build mongo client")?;
        let inner = client.database(db_name);
        let db = Self { inner };
        db.ensure_indexes().await?;
        Ok(db)
    }

    pub fn users(&self) -> Collection<UserDoc> {
        self.inner.collection::<UserDoc>("users")
    }

    pub fn sessions(&self) -> Collection<SessionDoc> {
        self.inner.collection::<SessionDoc>("sessions")
    }

    pub fn conversations(&self) -> Collection<ConversationDoc> {
        self.inner.collection::<ConversationDoc>("conversations")
    }

    pub fn chat_groups(&self) -> Collection<ChatGroupDoc> {
        self.inner.collection::<ChatGroupDoc>("chat_groups")
    }

    pub fn messages(&self) -> Collection<MessageDoc> {
        self.inner.collection::<MessageDoc>("messages")
    }

    pub fn company_overviews(&self) -> Collection<CompanyOverviewDoc> {
        self.inner.collection::<CompanyOverviewDoc>("company_overviews")
    }

    pub fn trigger_events(&self) -> Collection<TriggerEventDoc> {
        self.inner.collection::<TriggerEventDoc>("trigger_events")
    }

    async fn ensure_indexes(&self) -> Result<()> {
        let unique = IndexOptions::builder().unique(true).build();

        self.users()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "username_lower": 1 })
                    .options(unique.clone())
                    .build(),
                None,
            )
            .await
            .context("failed to create users.username_lower index")?;

        self.sessions()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "token": 1 })
                    .options(unique)
                    .build(),
                None,
            )
            .await
            .context("failed to create sessions.token index")?;

        self.conversations()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "user_id": 1, "updated_at": -1 })
                    .build(),
                None,
            )
            .await
            .context("failed to create conversations index")?;

        self.conversations()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "user_id": 1, "group_id": 1, "updated_at": -1 })
                    .build(),
                None,
            )
            .await
            .context("failed to create conversations.group_id index")?;

        self.chat_groups()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "user_id": 1, "updated_at": -1 })
                    .build(),
                None,
            )
            .await
            .context("failed to create chat_groups index")?;

        self.messages()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "conversation_id": 1, "created_at": 1 })
                    .build(),
                None,
            )
            .await
            .context("failed to create messages.conversation_id index")?;

        self.company_overviews()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "user_id": 1, "company_id": 1 })
                    .options(IndexOptions::builder().unique(true).build())
                    .build(),
                None,
            )
            .await
            .context("failed to create company_overviews.user_id/company_id index")?;

        self.trigger_events()
            .create_index(
                IndexModel::builder()
                    .keys(doc! { "user_id": 1, "company_id": 1, "created_at": -1 })
                    .build(),
                None,
            )
            .await
            .context("failed to create trigger_events.user_id/company_id index")?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub username: String,
    pub username_lower: String,
    pub display_name: String,
    pub password_hash: String,
    #[serde(with = "bson_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub token: String,
    pub user_id: ObjectId,
    #[serde(with = "bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson_datetime")]
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<ObjectId>,
    pub title: String,
    #[serde(with = "bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatGroupDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    pub name: String,
    pub data_text: String,
    #[serde(with = "bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub conversation_id: ObjectId,
    pub role: String, // "user" or "assistant"
    pub body: String,
    #[serde(with = "bson_datetime")]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyOverviewDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    pub company_id: ObjectId,
    pub company_name: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "option_bson_datetime")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "option_bson_datetime")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub discovered_competitors: Vec<crate::overview::OverviewCompetitor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<crate::overview::CompanyOverviewSummary>,
    #[serde(default)]
    pub markdown_brief: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(with = "bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson_datetime")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerEventDoc {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user_id: ObjectId,
    pub company_id: ObjectId,
    pub company_name: String,
    pub trigger_name: String,
    pub trigger_kind: String,
    pub title: String,
    pub body: String,
    pub severity: String,
    #[serde(default)]
    pub delivered_channels: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    #[serde(default)]
    pub payload: mongodb::bson::Document,
    #[serde(with = "bson_datetime")]
    pub created_at: DateTime<Utc>,
}

mod option_bson_datetime {
    use super::*;

    pub fn serialize<S>(value: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(value) => mongodb::bson::DateTime::from_chrono(*value).serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<mongodb::bson::DateTime>::deserialize(deserializer)
            .map(|value| value.map(|item| item.to_chrono()))
    }
}
