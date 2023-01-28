use crate::platform::{MarketError, MResult, Sats};

pub type Wallet = String;
pub type WalletAccess = String;
pub type Invoice = String;

pub enum FundingSource {
    Test,
    LNBits,
}
impl FundingSource {
    pub fn new_wallet(&self) -> Wallet {
        match self {
            Self::Test => Wallet::default(),
            _ => todo!(),
        }
    }
    pub fn send_to_wallet(
        &self,
        from: &WalletAccess,
        wallet: &Wallet,
        amount: Sats,
    ) -> Result<(), MarketError> {
        match self {
            Self::Test => Ok(()),
            _ => todo!(),
        }
    }
    pub fn create_invoice(&self, wallet: &Wallet) -> MResult<Invoice> {
        match self {
            Self::Test => Ok(String::from("test")),
            _ => todo!(),
        }
    }
    pub fn invoice_is_paid(&self, invoice: &Invoice) -> MResult<bool> {
        match self {
            Self::Test => Ok(true),
            _ => todo!(),
        }
    }
}
