#![forbid(unsafe_code)]

pub mod telegram;
pub mod web;

use std::collections::HashMap;
use std::sync::Arc;

use secrecy::SecretString;

use frankclaw_core::channel::ChannelPlugin;
use frankclaw_core::config::FrankClawConfig;
use frankclaw_core::error::{FrankClawError, Result};
use frankclaw_core::types::ChannelId;

pub struct ChannelSet {
    channels: HashMap<ChannelId, Arc<dyn ChannelPlugin>>,
    web: Option<Arc<web::WebChannel>>,
}

impl ChannelSet {
    pub fn channels(&self) -> &HashMap<ChannelId, Arc<dyn ChannelPlugin>> {
        &self.channels
    }

    pub fn get(&self, id: &ChannelId) -> Option<&Arc<dyn ChannelPlugin>> {
        self.channels.get(id)
    }

    pub fn web(&self) -> Option<Arc<web::WebChannel>> {
        self.web.clone()
    }
}

pub fn load_from_config(config: &FrankClawConfig) -> Result<ChannelSet> {
    let mut channels: HashMap<ChannelId, Arc<dyn ChannelPlugin>> = HashMap::new();
    let mut web_handle = None;

    for (channel_id, channel_config) in &config.channels {
        if !channel_config.enabled {
            continue;
        }

        match channel_id.as_str() {
            "web" => {
                let web = Arc::new(web::WebChannel::new());
                channels.insert(channel_id.clone(), web.clone());
                web_handle = Some(web);
            }
            "telegram" => {
                let account = channel_config.accounts.first().ok_or_else(|| {
                    FrankClawError::ConfigValidation {
                        msg: "telegram channel requires at least one account".into(),
                    }
                })?;
                let bot_token = resolve_channel_secret(
                    account,
                    &["bot_token", "token"],
                    &["bot_token_env", "token_env"],
                    "TELEGRAM_BOT_TOKEN",
                    "telegram",
                )?;
                let telegram = Arc::new(telegram::TelegramChannel::new(bot_token));
                channels.insert(channel_id.clone(), telegram);
            }
            other => {
                return Err(FrankClawError::ConfigValidation {
                    msg: format!(
                        "unsupported enabled channel '{}'; currently supported: web, telegram",
                        other
                    ),
                });
            }
        }
    }

    Ok(ChannelSet {
        channels,
        web: web_handle,
    })
}

fn resolve_channel_secret(
    account: &serde_json::Value,
    inline_keys: &[&str],
    env_keys: &[&str],
    default_env: &str,
    channel: &str,
) -> Result<SecretString> {
    for key in inline_keys {
        if let Some(value) = account.get(*key).and_then(|value| value.as_str()) {
            if !value.trim().is_empty() {
                return Ok(SecretString::from(value.to_string()));
            }
        }
    }

    for key in env_keys {
        if let Some(env_name) = account.get(*key).and_then(|value| value.as_str()) {
            return resolve_env_secret(env_name, channel);
        }
    }

    resolve_env_secret(default_env, channel)
}

fn resolve_env_secret(env_name: &str, channel: &str) -> Result<SecretString> {
    let env_name = env_name.trim();
    if env_name.is_empty() {
        return Err(FrankClawError::ConfigValidation {
            msg: format!("channel '{}' references an empty secret environment variable", channel),
        });
    }

    let value = std::env::var(env_name).map_err(|_| FrankClawError::ConfigValidation {
        msg: format!(
            "channel '{}' references missing environment variable '{}'",
            channel, env_name
        ),
    })?;

    if value.trim().is_empty() {
        return Err(FrankClawError::ConfigValidation {
            msg: format!(
                "channel '{}' environment variable '{}' is empty",
                channel, env_name
            ),
        });
    }

    Ok(SecretString::from(value))
}
