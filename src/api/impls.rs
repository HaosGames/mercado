use std::{
    fmt::{Display, Formatter},
    str::FromStr,
};

use anyhow::bail;

use super::*;

impl Display for JudgeState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::Nominated => "Nominated".into(),
            Self::Accepted => "Accepted".into(),
            Self::Refused => "Refused".into(),
            Self::Resolved(decision) => format!("Resolved({})", decision),
        };
        write!(f, "{}", output)
    }
}
impl Display for MarketState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::WaitingForJudges => "WaitingForJudges".into(),
            Self::Trading => "Trading".into(),
            Self::Stopped => "Stopped".into(),
            Self::WaitingForDecision => "WaitingForDecision".into(),
            Self::Resolved(outcome) => format!("Resolved({})", outcome),
            Self::Refunded(reason) => format!("Refunded({})", reason),
        };
        write!(f, "{}", output)
    }
}
impl Display for RefundReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::Insolvency => "Insolvency",
            Self::TimeForDecisionRanOut => "TimeForDecisionRanOut",
            Self::Tie => "Tie",
        };
        write!(f, "{}", output)
    }
}
impl Display for UserRole {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::User => "User",
            Self::Admin => "Admin",
            Self::Root => "Root",
        };
        write!(f, "{}", output)
    }
}
impl FromStr for UserRole {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "User" => Ok(Self::User),
            "Admin" => Ok(Self::Admin),
            "Root" => Ok(Self::Root),
            e => bail!("Couldn't deserialize to UserRole: {}", e),
        }
    }
}
impl Default for UserRole {
    fn default() -> Self {
        UserRole::User
    }
}
