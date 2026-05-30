use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppEnv {
    Development,
    Production,
}

impl AppEnv {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    pub fn is_development(self) -> bool {
        matches!(self, Self::Development)
    }

    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceProvider {
    Gemini,
    Vultr,
}

impl InferenceProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gemini => "gemini",
            Self::Vultr => "vultr",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gemini => "Gemini",
            Self::Vultr => "Vultr",
        }
    }
}

impl TryFrom<&str> for InferenceProvider {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gemini" => Ok(Self::Gemini),
            "vultr" => Ok(Self::Vultr),
            other => Err(anyhow!(
                "INFERENCE_PROVIDER must be `gemini` or `vultr`, received `{other}`"
            )),
        }
    }
}

impl TryFrom<&str> for AppEnv {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "development" | "dev" => Ok(Self::Development),
            "production" | "prod" => Ok(Self::Production),
            other => Err(anyhow!(
                "APP_ENV must be `development` or `production`, received `{other}`"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub default_title: String,
    pub description: String,
    pub locale: String,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub env: AppEnv,
    pub host: String,
    pub port: u16,
    pub app_url: String,
    pub vite_dev_server_url: String,
    pub metadata: Metadata,
    pub mongodb_uri: String,
    pub mongodb_db: String,
    pub session_cookie: String,
    pub inference_provider: InferenceProvider,
    pub inference_model: String,
    pub inference_system_prompt: String,
    pub inference_max_output_tokens: u32,
    pub gemini_api_key: Option<String>,
    pub vultr_api_key: Option<String>,
    pub brightdata_api_key: Option<String>,
    pub brightdata_serp_zone: Option<String>,
    pub brightdata_web_unlocker_zone: Option<String>,
    pub brightdata_scraping_browser_auth: Option<String>,
    pub brightdata_proxy_url: Option<String>,
    pub brightdata_trustpilot_dataset_id: Option<String>,
    pub brightdata_g2_dataset_id: Option<String>,
    pub brightdata_capterra_dataset_id: Option<String>,
    pub brightdata_linkedin_company_dataset_id: Option<String>,

    /* Cognee knowledge-graph */
    pub cognee_url: Option<String>,
    pub cognee_email: Option<String>,
    pub cognee_password: Option<String>,

    /* Autonomous monitoring (Watchtower) */
    pub watchtower_enabled: bool,
    pub watchtower_interval_secs: u64,
    pub watchtower_first_delay_secs: u64,

    /* TriggerWare event-driven monitoring */
    pub triggerware_api_url: String,
    pub triggerware_api_key: Option<String>,
    pub triggerware_poll_interval_secs: u64,

    /* Outbound alert webhooks */
    pub slack_webhook_url: Option<String>,
    pub discord_webhook_url: Option<String>,

    /* Speechmatics realtime voice + spoken-web transcription */
    pub speechmatics_api_key: Option<String>,
    pub speechmatics_rt_url: String,
    pub speechmatics_mgmt_url: String,
    pub speechmatics_tts_url: String,
    pub speechmatics_tts_voice: String,
    pub speechmatics_batch_url: String,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let env = AppEnv::try_from(env_string("APP_ENV", "development").as_str())?;
        let host = env_string("APP_HOST", "127.0.0.1");
        let port = env_u16("APP_PORT", 3000)?;
        let app_url = env_string("APP_URL", &format!("http://{host}:{port}"));
        let vite_dev_server_url = env_string("VITE_DEV_SERVER_URL", "http://127.0.0.1:5173");
        let metadata = Metadata {
            default_title: env_string("META_DEFAULT_TITLE", ""),
            description: env_string("META_DESCRIPTION", ""),
            locale: env_string("META_LOCALE", "en"),
        };
        let mongodb_uri = env_string("MONGODB_URI", "mongodb://127.0.0.1:27017");
        let mongodb_db = env_string("MONGODB_DB", "win_win");
        let session_cookie = env_string("SESSION_COOKIE", "ww_session");
        let inference_provider =
            InferenceProvider::try_from(env_string("INFERENCE_PROVIDER", "gemini").as_str())?;
        let inference_model = env_string("INFERENCE_MODEL", default_model(inference_provider));
        let inference_system_prompt = env_string("INFERENCE_SYSTEM_PROMPT", "");
        let inference_max_output_tokens = env_u32("INFERENCE_MAX_OUTPUT_TOKENS", 4096)?;
        let gemini_api_key = env_optional_string("GEMINI_API_KEY");
        let vultr_api_key = env_optional_string("VULTR_INFERENCE_API_KEY");
        let brightdata_api_key = env_optional_string("BRIGHTDATA_API_KEY");
        let brightdata_serp_zone = env_optional_string("BRIGHTDATA_SERP_ZONE");
        let brightdata_web_unlocker_zone = env_optional_string("BRIGHTDATA_WEB_UNLOCKER_ZONE");
        let brightdata_scraping_browser_auth =
            env_optional_string("BRIGHTDATA_SCRAPING_BROWSER_AUTH");
        let brightdata_proxy_url = env_optional_string("BRIGHTDATA_PROXY_URL");
        let brightdata_trustpilot_dataset_id =
            env_optional_string("BRIGHTDATA_TRUSTPILOT_DATASET_ID");
        let brightdata_g2_dataset_id = env_optional_string("BRIGHTDATA_G2_DATASET_ID");
        let brightdata_capterra_dataset_id =
            env_optional_string("BRIGHTDATA_CAPTERRA_DATASET_ID");
        let brightdata_linkedin_company_dataset_id =
            env_optional_string("BRIGHTDATA_LINKEDIN_COMPANY_DATASET_ID");
        let cognee_url = env_optional_string("COGNEE_URL");
        let cognee_email = env_optional_string("COGNEE_EMAIL");
        let cognee_password = env_optional_string("COGNEE_PASSWORD");

        // Autonomous monitoring ("Watchtower"). Defaults to ON with a 6h cadence
        // and a short post-boot delay so the first patrol is demonstrable.
        let watchtower_enabled = env::var("WATCHTOWER_ENABLED")
            .ok()
            .map(|value| {
                let v = value.trim().to_ascii_lowercase();
                !(v == "0" || v == "false" || v == "no" || v == "off")
            })
            .unwrap_or(true);
        let watchtower_interval_secs = env::var("WATCHTOWER_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(21_600);
        let watchtower_first_delay_secs = env::var("WATCHTOWER_FIRST_DELAY_SECS")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(90);

        let triggerware_api_url = env_string("TRIGGERWARE_API_URL", "https://api.triggerware.com");
        let triggerware_api_key = env_optional_string("TRIGGERWARE_API_KEY");
        let triggerware_poll_interval_secs = env::var("TRIGGERWARE_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(900);

        let slack_webhook_url = env_optional_string("SLACK_WEBHOOK_URL");
        let discord_webhook_url = env_optional_string("DISCORD_WEBHOOK_URL");

        let speechmatics_api_key = env_optional_string("SPEECHMATICS_API_KEY");
        let speechmatics_rt_url =
            env_string("SPEECHMATICS_RT_URL", crate::speechmatics::DEFAULT_RT_URL);
        let speechmatics_mgmt_url =
            env_string("SPEECHMATICS_MGMT_URL", crate::speechmatics::DEFAULT_MGMT_URL);
        let speechmatics_tts_url =
            env_string("SPEECHMATICS_TTS_URL", crate::speechmatics::DEFAULT_TTS_URL);
        let speechmatics_tts_voice =
            env_string("SPEECHMATICS_TTS_VOICE", crate::speechmatics::DEFAULT_TTS_VOICE);
        let speechmatics_batch_url =
            env_string("SPEECHMATICS_BATCH_URL", crate::speechmatics::DEFAULT_BATCH_URL);

        Ok(Self {
            env,
            host,
            port,
            app_url,
            vite_dev_server_url,
            metadata,
            mongodb_uri,
            mongodb_db,
            session_cookie,
            inference_provider,
            inference_model,
            inference_system_prompt,
            inference_max_output_tokens,
            gemini_api_key,
            vultr_api_key,
            brightdata_api_key,
            brightdata_serp_zone,
            brightdata_web_unlocker_zone,
            brightdata_scraping_browser_auth,
            brightdata_proxy_url,
            brightdata_trustpilot_dataset_id,
            brightdata_g2_dataset_id,
            brightdata_capterra_dataset_id,
            brightdata_linkedin_company_dataset_id,
            cognee_url,
            cognee_email,
            cognee_password,
            watchtower_enabled,
            watchtower_interval_secs,
            watchtower_first_delay_secs,
            triggerware_api_url,
            triggerware_api_key,
            triggerware_poll_interval_secs,
            slack_webhook_url,
            discord_webhook_url,
            speechmatics_api_key,
            speechmatics_rt_url,
            speechmatics_mgmt_url,
            speechmatics_tts_url,
            speechmatics_tts_voice,
            speechmatics_batch_url,
        })
    }

    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn inference_api_key(&self, provider: InferenceProvider) -> Option<&str> {
        match provider {
            InferenceProvider::Gemini => self.gemini_api_key.as_deref(),
            InferenceProvider::Vultr => self.vultr_api_key.as_deref(),
        }
    }

    pub fn inference_provider_enabled(&self, provider: InferenceProvider) -> bool {
        self.inference_api_key(provider).is_some()
    }

    pub fn available_inference_providers(&self) -> Vec<InferenceProvider> {
        [InferenceProvider::Gemini, InferenceProvider::Vultr]
            .into_iter()
            .filter(|provider| self.inference_provider_enabled(*provider))
            .collect()
    }

    pub fn default_inference_provider(&self) -> Option<InferenceProvider> {
        if self.inference_provider_enabled(self.inference_provider) {
            Some(self.inference_provider)
        } else {
            self.available_inference_providers().into_iter().next()
        }
    }

    pub fn configured_or_default_model(&self, provider: InferenceProvider) -> String {
        if provider == self.inference_provider {
            self.inference_model.clone()
        } else {
            default_model(provider).to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;

    #[test]
    fn brightdata_overview_keys_are_independent_from_inference_keys() {
        let config = AppConfig {
            env: super::AppEnv::Development,
            host: "127.0.0.1".to_owned(),
            port: 3000,
            app_url: "http://127.0.0.1:3000".to_owned(),
            vite_dev_server_url: "http://127.0.0.1:5173".to_owned(),
            metadata: super::Metadata {
                default_title: String::new(),
                description: String::new(),
                locale: "en".to_owned(),
            },
            mongodb_uri: "mongodb://127.0.0.1:27017".to_owned(),
            mongodb_db: "win_win".to_owned(),
            session_cookie: "ww_session".to_owned(),
            inference_provider: super::InferenceProvider::Vultr,
            inference_model: "kimi".to_owned(),
            inference_system_prompt: String::new(),
            inference_max_output_tokens: 4096,
            gemini_api_key: None,
            vultr_api_key: Some("vultr".to_owned()),
            brightdata_api_key: None,
            brightdata_serp_zone: None,
            brightdata_web_unlocker_zone: None,
            brightdata_scraping_browser_auth: None,
            brightdata_proxy_url: None,
            brightdata_trustpilot_dataset_id: None,
            brightdata_g2_dataset_id: None,
            brightdata_capterra_dataset_id: None,
            brightdata_linkedin_company_dataset_id: None,
            cognee_url: None,
            cognee_email: None,
            cognee_password: None,
            watchtower_enabled: false,
            watchtower_interval_secs: 21_600,
            watchtower_first_delay_secs: 90,
            triggerware_api_url: "https://api.triggerware.com".to_owned(),
            triggerware_api_key: None,
            triggerware_poll_interval_secs: 900,
            slack_webhook_url: None,
            discord_webhook_url: None,
            speechmatics_api_key: None,
            speechmatics_rt_url: crate::speechmatics::DEFAULT_RT_URL.to_owned(),
            speechmatics_mgmt_url: crate::speechmatics::DEFAULT_MGMT_URL.to_owned(),
            speechmatics_tts_url: crate::speechmatics::DEFAULT_TTS_URL.to_owned(),
            speechmatics_tts_voice: crate::speechmatics::DEFAULT_TTS_VOICE.to_owned(),
            speechmatics_batch_url: crate::speechmatics::DEFAULT_BATCH_URL.to_owned(),
        };

        assert_eq!(config.vultr_api_key.as_deref(), Some("vultr"));
        assert!(config.brightdata_api_key.is_none());
    }
}

fn default_model(provider: InferenceProvider) -> &'static str {
    match provider {
        InferenceProvider::Gemini => "gemini-2.5-flash",
        InferenceProvider::Vultr => "moonshotai/kimi-k2-instruct",
    }
}

fn env_string(key: &str, fallback: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

fn env_optional_string(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

fn env_u16(key: &str, fallback: u16) -> Result<u16> {
    let raw = env::var(key).unwrap_or_else(|_| fallback.to_string());
    raw.parse::<u16>()
        .with_context(|| format!("{key} must be a valid u16 value"))
}

fn env_u32(key: &str, fallback: u32) -> Result<u32> {
    let raw = env::var(key).unwrap_or_else(|_| fallback.to_string());
    raw.parse::<u32>()
        .with_context(|| format!("{key} must be a valid u32 value"))
}
