use std::sync::Arc;

use crate::{
    api::{Payment, PaymentDetails, PaymentState, Sats},
    db::DB,
    funding_source::FundingSource,
};
use anyhow::Result;

use super::client::LnBitsWallet;

pub struct LnbitsFundingSource {
    db: Arc<DB>,
    wallet: LnBitsWallet,
}
impl LnbitsFundingSource {
    pub async fn new_test(db: Arc<DB>, url: String) -> Result<Self> {
        let funding_source = Self {
            db,
            wallet: LnBitsWallet::new(url).await?,
        };
        funding_source.wallet.is_reachable().await?;
        Ok(funding_source)
    }
    pub async fn new(
        db: Arc<DB>,
        url: String,
        wallet_id: String,
        usr: String,
        api_key: String,
    ) -> Result<Self> {
        let funding_source = Self {
            db,
            wallet: LnBitsWallet::existing(url, wallet_id, usr, api_key),
        };
        funding_source.wallet.is_reachable().await?;
        Ok(funding_source)
    }
}
#[async_trait::async_trait]
impl FundingSource for LnbitsFundingSource {
    async fn create_payment(&self) -> Result<Payment> {
        todo!()
    }
    async fn pay(&self, payment: &Payment, amount: Sats) -> Result<PaymentState> {
        todo!()
    }
    async fn check_payment(&self, payment: &Payment) -> Result<PaymentState> {
        todo!()
    }
    async fn get_payment_details(&self, payment: Payment) -> Result<PaymentDetails> {
        todo!()
    }
}
