use crate::api::{Payment, PaymentState, Sats};
use crate::mercado::MercadoError;
use anyhow::{bail, Result};
use async_trait::async_trait;
use secp256k1::{generate_keypair, rand};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[async_trait]
pub trait FundingSource {
    async fn create_invoice(&self) -> Result<Payment>;
    async fn pay_invoice(&self, invoice: &Payment, amount: Sats) -> Result<PaymentState>;
    async fn check_invoice(&self, invoice: &Payment) -> Result<PaymentState>;
}
#[derive(Debug, Default)]
pub struct TestFundingSource {
    incoming: Arc<Mutex<HashMap<Payment, PaymentState>>>,
    outgoing: Arc<Mutex<HashMap<Payment, PaymentState>>>,
}
#[async_trait]
impl FundingSource for TestFundingSource {
    async fn create_invoice(&self) -> Result<Payment> {
        let (_, key) = generate_keypair(&mut rand::thread_rng());
        let invoice = key.to_string();
        self.incoming
            .lock()
            .unwrap()
            .insert(invoice.clone(), PaymentState::Created);
        Ok(invoice)
    }
    async fn pay_invoice(&self, invoice: &Payment, amount: Sats) -> Result<PaymentState> {
        let mut outgoing = self.outgoing.lock().unwrap();
        match outgoing.get(invoice) {
            None => {
                outgoing.insert(invoice.clone(), PaymentState::Settled(amount));
            }
            Some(state) => match state {
                PaymentState::Created | PaymentState::Failed => {
                    outgoing.insert(invoice.clone(), PaymentState::Settled(amount));
                }
                state => return Ok(state.clone()),
            },
        }
        Ok(PaymentState::Settled(amount))
    }
    async fn check_invoice(&self, invoice: &Payment) -> Result<PaymentState> {
        let outgoing = self.outgoing.lock().unwrap();
        if let Some(state) = outgoing.get(invoice) {
            Ok(state.clone())
        } else {
            bail!("Invoice doesn't exist")
        }
    }
}
