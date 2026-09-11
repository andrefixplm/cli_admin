use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VmStatus {
    Running,
    Stopped,
    Paused,
    Offline,
    Unknown,
}

impl std::fmt::Display for VmStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmStatus::Running => write!(f, "RUNNING"),
            VmStatus::Stopped => write!(f, "STOPPED"),
            VmStatus::Paused => write!(f, "PAUSED"),
            VmStatus::Offline => write!(f, "OFFLINE"),
            VmStatus::Unknown => write!(f, "UNKNOWN"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartMode {
    Gui,
    Nogui,
}

impl StartMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            StartMode::Gui => "gui",
            StartMode::Nogui => "nogui",
        }
    }
}

impl std::str::FromStr for StartMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gui" => Ok(StartMode::Gui),
            "nogui" => Ok(StartMode::Nogui),
            other => Err(format!("Modo de inicialização inválido: {}. Use 'gui' ou 'nogui'", other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopMode {
    Soft,
    Hard,
}

impl StopMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            StopMode::Soft => "soft",
            StopMode::Hard => "hard",
        }
    }
}

impl std::str::FromStr for StopMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "soft" => Ok(StopMode::Soft),
            "hard" => Ok(StopMode::Hard),
            other => Err(format!("Modo de parada inválido: {}. Use 'soft' ou 'hard'", other)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub alias: Option<String>,
    pub name: String,
    pub path: PathBuf,
    pub status: VmStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub name: String,
}
