use anyhow::Context;
use serde::{
    de::{self, Error},
    Deserialize,
};
use std::{fs, path::PathBuf};

use crate::payload::MethodWrapper;

pub enum Validate {
    Dont,
    Command(String),
    GitHub(String),
}

impl<'de> de::Deserialize<'de> for Validate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum AllPossible {
            Bool(bool),
            Str(String),
            Pair([String; 2]),
        }
        let s: AllPossible = de::Deserialize::deserialize(deserializer)?;
        match s {
            AllPossible::Str(s) => Ok(Validate::Command(s)),
            AllPossible::Bool(b) => match b {
                false => Ok(Validate::Dont),
                true => Err(D::Error::invalid_value(
                    de::Unexpected::Bool(true),
                    &"the only valid values are `false`, a preset validation method, or a string",
                )),
            },
            AllPossible::Pair([method, key]) => match method.as_str() {
                "github" => Ok(Validate::GitHub(key)),
                _ => Err(D::Error::invalid_value(
                    de::Unexpected::Str(&method),
                    &"The only preset validation method supported so far is GitHub",
                )),
            },
        }
    }
}

#[derive(Deserialize)]
pub struct Catcher {
    pub path: String,
    pub run: String,
    pub methods: Vec<MethodWrapper>,
    pub validate: Validate,
}

#[derive(Deserialize)]
pub struct Config {
    pub port: u16,
    pub catchers: Vec<Catcher>,
}

const CONFIG_FILE: &str = "config.json";

pub fn get(config_path: Option<PathBuf>) -> anyhow::Result<Config> {
    let cfg_path;
    if let Some(config_path) = config_path {
        cfg_path = config_path;
    } else {
        let cfg_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::Error::msg("Could not get config directory"))?;
        cfg_path = cfg_dir.join(crate::PROJ_NAME).join(CONFIG_FILE);
    }
    let cfg_file = fs::File::open(cfg_path).context("Could not open configuration file")?;
    serde_json::from_reader(cfg_file).context("JSON did not fit data format")
}
