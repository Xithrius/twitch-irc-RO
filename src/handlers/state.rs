use std::{
    cmp::{Eq, PartialEq},
    fmt::Display,
    str::FromStr,
};

use color_eyre::eyre::{bail, Error, Result};
use serde::Serialize;
use serde_with::DeserializeFromStr;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, DeserializeFromStr)]
pub enum NormalMode {
    Insert,
    Search,
}

impl Display for NormalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Insert => "insert",
                Self::Search => "search",
            }
        )
    }
}

impl FromStr for NormalMode {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "insert" | "input" => Ok(Self::Insert),
            "search" => Ok(Self::Search),
            _ => bail!("Normal mode '{}' cannot be deserialized", s),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, DeserializeFromStr)]
pub enum State {
    Dashboard,
    Normal,
    Help,
}

impl Default for State {
    fn default() -> Self {
        Self::Dashboard
    }
}

impl Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Dashboard => "dashboard",
                Self::Normal => "normal",
                Self::Help => "help",
            }
        )
    }
}

impl FromStr for State {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "normal" | "default" | "chat" => Ok(Self::Normal),
            "dashboard" | "dash" | "start" => Ok(Self::Dashboard),
            "help" | "commands" => Ok(Self::Help),
            _ => bail!("State '{}' cannot be deserialized", s),
        }
    }
}
