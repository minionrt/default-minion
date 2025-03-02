use serde::Deserialize;
use url::Url;

#[derive(Deserialize)]
pub struct Config {
    pub api_base_url: Option<Url>,
    pub api_token: Option<String>,
}

impl Config {
    pub fn load() -> Self {
        envy::prefixed("MINION_").from_env::<Config>().unwrap()
    }
}
