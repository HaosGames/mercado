use std::borrow::Cow;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Client, Response, StatusCode, Url};
use serde::{Deserialize, Serialize};

use crate::api::{Invoice, Payment, Sats};

pub type PaymentHash = String;

const SUPER_USER: &str = "3a98ce0c0b88486987d67cff59ef6094";
const SUPER_USER_API_KEY: &str = "2321e4bf8ffe420ebff7d50102fd3f59";
#[cfg(test)]
pub async fn read_super_user() -> Result<String> {
    let mut file = tokio::fs::File::open("lnbits/data/.super_user").await?;
    let mut contents = vec![];
    tokio::io::AsyncReadExt::read_to_end(&mut file, &mut contents).await?;
    Ok(String::from_utf8(contents)?)
}

#[derive(Debug, Clone)]
pub struct LnBitsWallet {
    client: Client,
    pub url: String,
    pub api_key: String,
    pub wallet_id: String,
    pub usr: String,
}

impl LnBitsWallet {
    pub fn existing(url: String, wallet_id: String, usr: String, api_key: String) -> Self {
        let client = reqwest::Client::new();
        Self {
            url,
            client,
            api_key,
            wallet_id,
            usr,
        }
    }
    pub async fn new(url: String) -> Result<Self> {
        let client = reqwest::Client::new();
        let response = client.get(url.clone() + "/wallet").send().await?;
        let response_url = response.url();
        let usr = Self::extract_usr(response_url)?;
        let response_text = response.text().await?;
        Ok(Self {
            client,
            url,
            api_key: Self::extract_api_key(&response_text)?,
            wallet_id: Self::extract_wallet_id(&response_text)?,
            usr,
        })
    }
    fn extract_usr(url: &Url) -> Result<String> {
        let query_pairs = url.query_pairs();
        let vec = query_pairs
            .filter_map(|(key, value)| {
                if let Cow::Borrowed("usr") = key {
                    Some(value.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<String>>();
        Ok(vec
            .first()
            .ok_or(anyhow!("No usr found in URL Query String"))?
            .to_string())
    }
    pub async fn query_super_user_api_key(&self, usr: String) -> Result<String> {
        let response = self
            .get("/wallet?usr=".to_string() + usr.as_str(), StatusCode::OK)
            .await
            .context("couldn't query lnbits for the super_user wallet")?;
        let text = response.text().await?;
        let api_key =
            Self::extract_api_key(&text).context("couldn't extract api key from response text")?;
        Ok(api_key)
    }
    fn extract_api_key(response_text: &String) -> Result<String> {
        let api_key_lines = response_text
            .lines()
            .filter_map(|line| {
                if line.contains("Admin key:") {
                    return Some(line);
                } else {
                    None
                }
            })
            .collect::<Vec<&str>>();
        let key_line = api_key_lines
            .first()
            .ok_or(anyhow!("No api key found in response body"))?;
        let api_key = key_line
            .trim_end_matches("</em><br />")
            .trim_start_matches("    <strong>Admin key: </strong><em>")
            .to_string();
        if api_key.len() != 32 {
            bail!("Extracted Api Admin Key ({}) has the wrong length", api_key);
        }
        Ok(api_key)
    }
    fn extract_wallet_id(response_text: &String) -> Result<String> {
        let wallet_id_lines = response_text
            .lines()
            .filter_map(|line| {
                if line.contains("Wallet ID:") {
                    return Some(line);
                } else {
                    None
                }
            })
            .collect::<Vec<&str>>();
        let id_line = wallet_id_lines
            .first()
            .ok_or(anyhow!("No wallet id found in response body"))?;
        let wallet_id = id_line
            .trim_end_matches("</em><br />")
            .trim_start_matches("    <strong>Wallet ID: </strong><em>")
            .to_string();
        if wallet_id.len() != 32 {
            bail!("Extracted Wallet ID ({}) has the wrong length", wallet_id);
        }
        Ok(wallet_id)
    }
    pub async fn top_up_wallet(
        &self,
        super_user: String,
        amount: u32,
        super_user_api_key: String,
    ) -> Result<()> {
        let request = TopUpRequest {
            id: self.wallet_id.clone(),
            amount,
        };
        let response = self
            .client
            .put(self.url.clone() + "/admin/api/v1/topup/?usr=" + super_user.as_str())
            .header("X-Api-Key", super_user_api_key)
            .json(&request)
            .send()
            .await?;
        if let StatusCode::OK = response.status() {
            Ok(())
        } else {
            bail!("Couldn't top up wallet: {}", response.text().await?)
        }
    }
    async fn post(
        &self,
        path: String,
        request: impl Serialize,
        expexted_code: StatusCode,
    ) -> Result<Response> {
        let response = self
            .client
            .post(self.url.clone() + path.as_str())
            .header("X-Api-Key", self.api_key.clone())
            .json(&request)
            .send()
            .await?;
        crate::client::bail_if_err(response, expexted_code).await
    }
    async fn get(&self, path: String, expexted_code: StatusCode) -> Result<Response> {
        let response = self
            .client
            .get(self.url.clone() + path.as_str())
            .header("X-Api-Key", self.api_key.clone())
            .send()
            .await?;
        crate::client::bail_if_err(response, expexted_code).await
    }
    pub async fn create_invoice(&self, amount: Sats) -> Result<(PaymentHash, Invoice)> {
        let request = CreateInvoiceRequest {
            out: false,
            memo: "".to_string(),
            amount: amount as u32,
        };
        let response = self
            .post("/api/v1/payments".to_string(), request, StatusCode::CREATED)
            .await?;
        let json = response.json::<CreateInvoiceResponse>().await?;
        Ok((json.payment_hash, json.payment_request))
    }
    pub async fn pay_invoice(&self, invoice: Invoice) -> Result<PaymentHash> {
        let request = PayInvoiceRequest {
            out: true,
            bolt11: invoice,
        };
        let response = self
            .post("/api/v1/payments".to_string(), request, StatusCode::CREATED)
            .await?;
        let json = response.json::<PayInvoiceResponse>().await?;
        Ok(json.payment_hash)
    }
    pub async fn is_payed(&self, payment_hash: PaymentHash) -> Result<bool> {
        let response = self
            .get(
                "/api/v1/payments/".to_string() + payment_hash.as_str(),
                StatusCode::OK,
            )
            .await?;
        let json = response.json::<CheckInvoiceResponse>().await?;
        Ok(json.paid)
    }
    pub async fn get_payment_amount(&self, payment_hash: PaymentHash) -> Result<Sats> {
        let response = self
            .get(
                "/api/v1/payments/".to_string() + payment_hash.as_str(),
                StatusCode::OK,
            )
            .await?;
        let json = response.json::<CheckInvoiceResponse>().await?;
        Ok(json.details.amount / 1000)
    }
    pub async fn decode_bolt11(&self, invoice: Invoice) -> Result<Sats> {
        let request = DecodeBolt11Request { data: invoice };
        let response = self
            .post(
                "/api/v1/payments/decode".to_string(),
                request,
                StatusCode::OK,
            )
            .await?;
        let json = response.json::<DecodeBolt11Response>().await?;
        Ok(json.amount_msat / 1000)
    }
    pub async fn is_reachable(&self) -> Result<()> {
        let response = self
            .get(
                "/wallet/?usr=".to_string() + self.usr.as_str() + "&wal=" + self.wallet_id.as_str(),
                StatusCode::OK,
            )
            .await
            .context("Configured lnbits instance is not reachable")?;
        let wallet_id = Self::extract_wallet_id(&response.text().await?)
            .context("Configured wallet is not reachable")?;
        if wallet_id == self.wallet_id {
            Ok(())
        } else {
            bail!("Connected to the wrong wallet")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TopUpRequest {
    id: String,
    amount: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateInvoiceRequest {
    out: bool,
    amount: u32,
    memo: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInvoiceResponse {
    payment_hash: String,
    payment_request: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayInvoiceRequest {
    out: bool,
    bolt11: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeBolt11Request {
    data: Invoice,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayInvoiceResponse {
    payment_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckInvoiceResponse {
    paid: bool,
    details: CheckInvoiceDetails,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckInvoiceDetails {
    amount: i64,
    bolt11: String,
    time: i64,
    fee: i64,
    memo: String,
    pending: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeBolt11Response {
    payment_hash: PaymentHash,
    amount_msat: Sats,
    description: String,
    payee: String,
}

#[cfg(test)]
mod test {
    use super::*;
    #[tokio::test]
    async fn create_new_wallet() {
        let wallet = LnBitsWallet::new("http://127.0.0.1:5000".to_string())
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn top_up_new_wallet() {
        let super_user = read_super_user().await.unwrap();
        let wallet = LnBitsWallet::new("http://127.0.0.1:5000".to_string())
            .await
            .unwrap();
        let super_user_api_key = wallet
            .query_super_user_api_key(super_user.clone())
            .await
            .unwrap();
        wallet
            .top_up_wallet(super_user.to_string(), 500, super_user_api_key.to_string())
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn create_and_pay_invoice() {
        let sender_wallet = LnBitsWallet::new("http://127.0.0.1:5000".to_string())
            .await
            .unwrap();
        let super_user = read_super_user().await.unwrap();
        let super_user_api_key = sender_wallet
            .query_super_user_api_key(super_user.clone())
            .await
            .unwrap();
        sender_wallet
            .top_up_wallet(super_user.to_string(), 500, super_user_api_key.to_string())
            .await
            .unwrap();
        let receiver_wallet = LnBitsWallet::new("http://127.0.0.1:5000".to_string())
            .await
            .unwrap();
        let (payment_hash, invoice) = receiver_wallet.create_invoice(100).await.unwrap();
        sender_wallet.pay_invoice(invoice).await.unwrap();
        let paid = receiver_wallet.is_payed(payment_hash).await.unwrap();
        assert_eq!(paid, true);
    }
}
