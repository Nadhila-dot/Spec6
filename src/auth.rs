use crate::db::{Db, SessionDoc, UserDoc};
use anyhow::{Context, Result, anyhow};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum_extra::extract::cookie::CookieJar;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use mongodb::bson::{doc, oid::ObjectId};
use rand::RngCore;
use serde::Serialize;

const SESSION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
}

impl From<&UserDoc> for AuthUser {
    fn from(doc: &UserDoc) -> Self {
        Self {
            id: doc.id.map(|id| id.to_hex()).unwrap_or_default(),
            username: doc.username.clone(),
            display_name: doc.display_name.clone(),
        }
    }
}

#[derive(Debug)]
pub enum AuthError {
    UsernameTaken,
    InvalidCredentials,
    BadInput(String),
    Internal(anyhow::Error),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UsernameTaken => write!(f, "username already taken"),
            Self::InvalidCredentials => write!(f, "invalid username or password"),
            Self::BadInput(msg) => write!(f, "{msg}"),
            Self::Internal(err) => write!(f, "{err}"),
        }
    }
}

impl From<anyhow::Error> for AuthError {
    fn from(err: anyhow::Error) -> Self {
        Self::Internal(err)
    }
}

pub fn validate_username(value: &str) -> Result<String, AuthError> {
    let trimmed = value.trim();
    if trimmed.len() < 3 {
        return Err(AuthError::BadInput(
            "Username must be at least 3 characters".into(),
        ));
    }
    if trimmed.len() > 24 {
        return Err(AuthError::BadInput(
            "Username must be at most 24 characters".into(),
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(AuthError::BadInput(
            "Username can only contain letters, numbers, _ - or .".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

pub fn validate_password(value: &str) -> Result<(), AuthError> {
    if value.len() < 8 {
        return Err(AuthError::BadInput(
            "Password must be at least 8 characters".into(),
        ));
    }
    if value.len() > 256 {
        return Err(AuthError::BadInput("Password is too long".into()));
    }
    Ok(())
}

pub fn validate_display_name(value: &str) -> Result<String, AuthError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AuthError::BadInput("Display name cannot be empty".into()));
    }
    if trimmed.chars().count() > 48 {
        return Err(AuthError::BadInput(
            "Display name must be at most 48 characters".into(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn hash_password(plain: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(plain.as_bytes(), &salt)
        .map_err(|e| anyhow!("argon2 hash failed: {e}"))?
        .to_string();
    Ok(hash)
}

fn verify_password(plain: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok()
}

fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub async fn signup(
    db: &Db,
    username: &str,
    display_name: &str,
    password: &str,
) -> Result<(AuthUser, String), AuthError> {
    let username = validate_username(username)?;
    let display_name = validate_display_name(display_name)?;
    validate_password(password)?;

    let username_lower = username.to_lowercase();
    let existing = db
        .users()
        .find_one(doc! { "username_lower": &username_lower }, None)
        .await
        .context("failed to check username")?;
    if existing.is_some() {
        return Err(AuthError::UsernameTaken);
    }

    let password_hash = hash_password(password)?;
    let user_doc = UserDoc {
        id: None,
        username: username.clone(),
        username_lower,
        display_name,
        password_hash,
        created_at: Utc::now(),
    };

    let inserted = db
        .users()
        .insert_one(&user_doc, None)
        .await
        .context("failed to insert user")?;
    let id = inserted
        .inserted_id
        .as_object_id()
        .ok_or_else(|| anyhow!("inserted user missing id"))?;

    let mut stored = user_doc;
    stored.id = Some(id);

    let token = issue_session(db, id).await?;
    Ok((AuthUser::from(&stored), token))
}

pub async fn login(
    db: &Db,
    username: &str,
    password: &str,
) -> Result<(AuthUser, String), AuthError> {
    let username = username.trim();
    if username.is_empty() || password.is_empty() {
        return Err(AuthError::InvalidCredentials);
    }
    let username_lower = username.to_lowercase();

    let user = db
        .users()
        .find_one(doc! { "username_lower": &username_lower }, None)
        .await
        .context("failed to fetch user")?
        .ok_or(AuthError::InvalidCredentials)?;

    if !verify_password(password, &user.password_hash) {
        return Err(AuthError::InvalidCredentials);
    }

    let id = user.id.ok_or_else(|| anyhow!("stored user missing id"))?;
    let token = issue_session(db, id).await?;
    Ok((AuthUser::from(&user), token))
}

async fn issue_session(db: &Db, user_id: ObjectId) -> Result<String> {
    let token = new_session_token();
    let now = Utc::now();
    let session = SessionDoc {
        id: None,
        token: token.clone(),
        user_id,
        created_at: now,
        expires_at: now + Duration::days(SESSION_DAYS),
    };
    db.sessions()
        .insert_one(&session, None)
        .await
        .context("failed to insert session")?;
    Ok(token)
}

pub async fn logout(db: &Db, token: &str) -> Result<()> {
    db.sessions()
        .delete_one(doc! { "token": token }, None)
        .await
        .context("failed to delete session")?;
    Ok(())
}

pub async fn user_from_token(db: &Db, token: &str) -> Result<Option<AuthUser>> {
    let now = Utc::now();
    let Some(session) = db
        .sessions()
        .find_one(doc! { "token": token }, None)
        .await
        .context("failed to look up session")?
    else {
        return Ok(None);
    };

    if session.expires_at < now {
        let _ = db
            .sessions()
            .delete_one(doc! { "_id": session.id }, None)
            .await;
        return Ok(None);
    }

    let user = db
        .users()
        .find_one(doc! { "_id": session.user_id }, None)
        .await
        .context("failed to load session user")?;

    Ok(user.as_ref().map(AuthUser::from))
}

pub async fn user_from_cookies(
    db: &Db,
    cookies: &CookieJar,
    cookie_name: &str,
) -> Result<Option<AuthUser>> {
    let Some(cookie) = cookies.get(cookie_name) else {
        return Ok(None);
    };
    user_from_token(db, cookie.value()).await
}

pub fn require_object_id(user: &AuthUser) -> Result<ObjectId> {
    ObjectId::parse_str(&user.id).map_err(|_| anyhow!("invalid user id in session"))
}

pub fn session_days() -> i64 {
    SESSION_DAYS
}
