use crate::api::{
    Invoice, Payment, PaymentHash, RowId, Sats, TxDirection, TxStateBolt11, TxType, TxTypes,
};
use anyhow::{bail, Result};
use async_trait::async_trait;
use secp256k1::{generate_keypair, rand};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[async_trait]
pub trait FundingSource {
    async fn create_bolt11(&self, amount: Sats) -> Result<(PaymentHash, Invoice)>;
    async fn pay_bolt11(&self, invoice: Invoice, amount: Sats) -> Result<PaymentHash>;
    async fn check_bolt11(&self, hash: PaymentHash) -> Result<TxStateBolt11>;
}
#[derive(Debug, Default)]
pub struct TestFundingSource {
    bolt11: Arc<Mutex<HashMap<PaymentHash, TxStateBolt11>>>,
}
#[async_trait]
impl FundingSource for TestFundingSource {
    async fn create_bolt11(&self, amount: Sats) -> Result<(PaymentHash, Invoice)> {
        let (_, hash) = generate_keypair(&mut rand::thread_rng());
        let (_, invoice) = generate_keypair(&mut rand::thread_rng());
        let invoice = invoice.to_string();
        let hash = hash.to_string();
        self.bolt11
            .lock()
            .unwrap()
            .insert(hash.clone(), TxStateBolt11::Settled(amount));
        Ok((hash, invoice))
    }
    async fn pay_bolt11(&self, _invoice: Invoice, amount: Sats) -> Result<PaymentHash> {
        let (_, hash) = generate_keypair(&mut rand::thread_rng());
        let hash = hash.to_string();
        self.bolt11
            .lock()
            .unwrap()
            .insert(hash.clone(), TxStateBolt11::Settled(amount));
        Ok(hash)
    }
    async fn check_bolt11(&self, hash: PaymentHash) -> Result<TxStateBolt11> {
        let tx = self.bolt11.lock().unwrap();
        if let Some(state) = tx.get(&hash) {
            Ok(state.clone())
        } else {
            bail!("Invoice doesn't exist")
        }
    }
}
