use std::sync::Arc;

use crate::{
    api::{Invoice, Payment, PaymentHash, Sats, TxDetailsBolt11, TxStateBolt11},
    db::DB,
    funding_source::FundingSource,
};
use anyhow::Result;

use super::client::LnBitsWallet;

pub struct LnbitsFundingSource {
    wallet: LnBitsWallet,
}
impl LnbitsFundingSource {
    pub async fn new_test(url: String) -> Result<Self> {
        let funding_source = Self {
            wallet: LnBitsWallet::new(url).await?,
        };
        funding_source.wallet.is_reachable().await?;
        Ok(funding_source)
    }
    pub async fn new(url: String, wallet_id: String, usr: String, api_key: String) -> Result<Self> {
        let funding_source = Self {
            wallet: LnBitsWallet::existing(url, wallet_id, usr, api_key),
        };
        funding_source.wallet.is_reachable().await?;
        Ok(funding_source)
    }
}
#[async_trait::async_trait]
impl FundingSource for LnbitsFundingSource {
    async fn create_bolt11(&self, amount: Sats) -> Result<(PaymentHash, Invoice)> {
        self.wallet.create_invoice(amount).await
    }
    async fn pay_bolt11(&self, invoice: Invoice, _amount: Sats) -> Result<PaymentHash> {
        self.wallet.pay_invoice(invoice).await
    }
    async fn check_bolt11(&self, hash: PaymentHash) -> Result<TxStateBolt11> {
        let amount = self.wallet.get_payment_amount(hash.clone()).await?;
        if self.wallet.is_payed(hash).await? {
            Ok(TxStateBolt11::Settled(amount))
        } else {
            Ok(TxStateBolt11::PayInit(amount))
        }
    }
    async fn decode_bolt11(&self, invoice: Invoice) -> Result<Sats> {
        self.wallet.decode_bolt11(invoice).await
    }
}
