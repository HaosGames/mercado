use crate::market::{MercadoError, Sats};
use surrealdb::{
    sql::{parse, Id},
    Datastore, Session,
};

pub struct DB {
    db: Datastore,
    session: Session,
}
impl DB {
    pub async fn new() -> Self {
        Self {
            db: Datastore::new("memory").await.unwrap(),
            session: Session::for_kv(),
        }
    }
    fn add_user(&self, user: String) {
        let query = parse().unwrap();
    }
    fn add_funds(&self, user: Id, amount: Sats) {
        let query = parse().unwrap();
    }
    fn remove_funds(&self, user: Id, amount: Sats) -> Result<(), MercadoError> {
        todo!()
    }
}
