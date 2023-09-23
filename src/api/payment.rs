use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentDetails {
    Bolt11 {
        payment_hash: String,
        payment_request: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaymentState {
    Created,
    PayInit(Sats),
    Settled(Sats),
    Failed,
}
