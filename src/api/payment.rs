use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::*;

pub type Invoice = String;
pub type PaymentHash = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tx {
    pub user: UserPubKey,
    pub initiated: DateTime<Utc>,
    pub direction: TxDirection,
    pub tx_type: TxType,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxType {
    Bolt11 {
        details: TxDetailsBolt11,
        state: TxStateBolt11,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TxDetailsBolt11 {
    pub payment_hash: PaymentHash,
    pub payment_request: Invoice,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxStateBolt11 {
    Created,
    PayInit(Sats),
    Settled(Sats),
    Failed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxDirection {
    Deposit,
    Withdrawal,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TxTypes {
    Bolt11,
}
