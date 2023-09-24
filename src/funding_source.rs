use crate::api::{Payment, PaymentDetails, PaymentState, Sats};
use anyhow::{bail, Result};
use async_trait::async_trait;
use secp256k1::{generate_keypair, rand};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[async_trait]
pub trait FundingSource {
    async fn create_payment(&self) -> Result<Payment>;
    async fn pay(&self, payment: &Payment, amount: Sats) -> Result<PaymentState>;
    async fn check_payment(&self, payment: &Payment) -> Result<PaymentState>;
    async fn get_payment_details(&self, payment: Payment) -> Result<PaymentDetails>;
}
#[derive(Debug, Default)]
pub struct TestFundingSource {
    incoming: Arc<Mutex<HashMap<Payment, PaymentState>>>,
    outgoing: Arc<Mutex<HashMap<Payment, PaymentState>>>,
}
#[async_trait]
impl FundingSource for TestFundingSource {
    async fn create_payment(&self) -> Result<Payment> {
        let (_, key) = generate_keypair(&mut rand::thread_rng());
        let invoice = key.to_string();
        self.incoming
            .lock()
            .unwrap()
            .insert(invoice.clone(), PaymentState::Created);
        Ok(invoice)
    }
    async fn pay(&self, payment: &Payment, amount: Sats) -> Result<PaymentState> {
        let mut outgoing = self.outgoing.lock().unwrap();
        match outgoing.get(payment) {
            None => {
                outgoing.insert(payment.clone(), PaymentState::Settled(amount));
            }
            Some(state) => match state {
                PaymentState::Created | PaymentState::Failed => {
                    outgoing.insert(payment.clone(), PaymentState::Settled(amount));
                }
                state => return Ok(state.clone()),
            },
        }
        Ok(PaymentState::Settled(amount))
    }
    async fn check_payment(&self, payment: &Payment) -> Result<PaymentState> {
        let outgoing = self.outgoing.lock().unwrap();
        if let Some(state) = outgoing.get(payment) {
            Ok(state.clone())
        } else {
            bail!("Invoice doesn't exist")
        }
    }
    async fn get_payment_details(&self, payment: Payment) -> Result<PaymentDetails> {
        todo!()
    }
}
