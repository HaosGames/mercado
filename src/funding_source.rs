use crate::api::{Invoice, InvoiceState, Sats};
use crate::mercado::MercadoError;
use anyhow::{bail, Result};
use async_trait::async_trait;
use secp256k1::{generate_keypair, rand};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
        match outgoing.get(invoice) {
            None => {
                outgoing.insert(invoice.clone(), InvoiceState::Settled(amount));
            }
            Some(state) => match state {
                InvoiceState::Created | InvoiceState::Failed => {
                    outgoing.insert(invoice.clone(), InvoiceState::Settled(amount));
                }
                state => return Ok(state.clone()),
            },
        }
        Ok(InvoiceState::Settled(amount))
    }
    async fn check_invoice(&self, invoice: &Invoice) -> Result<InvoiceState> {
        let outgoing = self.outgoing.lock().unwrap();
        if let Some(state) = outgoing.get(invoice) {
            Ok(state.clone())
        } else {
            bail!("Invoice doesn't exist")
        }
    }
}
