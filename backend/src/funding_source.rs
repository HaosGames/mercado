use crate::mercado::{MercadoError, Sats};
use anyhow::{bail, Result};
use async_trait::async_trait;
use secp256k1::{generate_keypair, rand};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub type Invoice = String;
#[derive(Debug, Clone)]
pub enum InvoiceState {
    Created,
    PayInit(Sats),
    Settled(Sats),
    Failed,
}
#[async_trait]
pub trait FundingSource {
    async fn create_invoice(&self) -> Result<Invoice>;
    async fn pay_invoice(&self, invoice: &Invoice, amount: Sats) -> Result<InvoiceState>;
    async fn check_invoice(&self, invoice: &Invoice) -> Result<InvoiceState>;
}
#[derive(Debug, Default)]
pub struct TestFundingSource {
    incoming: Arc<Mutex<HashMap<Invoice, InvoiceState>>>,
    outgoing: Arc<Mutex<HashMap<Invoice, InvoiceState>>>,
}
#[async_trait]
impl FundingSource for TestFundingSource {
    async fn create_invoice(&self) -> Result<Invoice> {
        let (_, key) = generate_keypair(&mut rand::thread_rng());
        let invoice = key.to_string();
        self.incoming
            .lock()
            .unwrap()
            .insert(invoice.clone(), InvoiceState::Created);
        Ok(invoice)
    }
    async fn pay_invoice(&self, invoice: &Invoice, amount: Sats) -> Result<InvoiceState> {
        let mut outgoing = self.outgoing.lock().unwrap();
        if let None = outgoing.get(invoice) {
            outgoing.insert(invoice.clone(), InvoiceState::Settled(amount));
        }
        Ok(InvoiceState::Settled(amount))
    }
    async fn check_invoice(&self, invoice: &Invoice) -> Result<InvoiceState> {
        let outgoing = self.outgoing.lock().unwrap();
        if let Some(state) = outgoing.get(invoice) {
            Ok(state.clone())
        } else {
            bail!(MercadoError::Other("Invoice doesn't exist"))
        }
    }
}
