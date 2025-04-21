#![allow(clippy::cast_possible_truncation)]

use std::time::SystemTime;

use rust_decimal::Decimal;

#[derive(Debug, thiserror::Error)]
pub enum Error {}

pub trait Bank: Send + Sync {
    fn list_transactions(&self) -> Result<&[Transaction], Error>;
    fn get_transaction(&self, id: i32) -> Result<Option<&Transaction>, Error>;
    fn create_transaction(&mut self, amount: Decimal) -> Result<&Transaction, Error>;
    fn void_transaction(&mut self, id: i32) -> Result<Option<&Transaction>, Error>;
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: i32,
    pub amount: Decimal,
    pub created_at: i32,
}

pub struct LocalBank {
    transactions: Vec<Transaction>,
    current_id: i32,
}

impl LocalBank {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            transactions: vec![],
            current_id: 1,
        }
    }
}

impl Bank for LocalBank {
    fn list_transactions(&self) -> Result<&[Transaction], Error> {
        Ok(&self.transactions)
    }

    fn get_transaction(&self, id: i32) -> Result<Option<&Transaction>, Error> {
        Ok(self.transactions.iter().find(|x| x.id == id))
    }

    fn create_transaction(&mut self, amount: Decimal) -> Result<&Transaction, Error> {
        let id = self.current_id;
        self.current_id += 1;
        let transaction = Transaction {
            id,
            amount,
            created_at: dst_demo_time::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i32,
        };
        self.transactions.push(transaction);
        Ok(self.transactions.last().unwrap())
    }

    fn void_transaction(&mut self, id: i32) -> Result<Option<&Transaction>, Error> {
        let Some(existing) = self.transactions.iter().find(|x| x.id == id) else {
            return Ok(None);
        };

        let originally_created_at = existing.created_at;

        let new_transaction = self.create_transaction(-existing.amount)?;

        assert!(
            new_transaction.created_at >= originally_created_at,
            "Time went backwards"
        );

        Ok(Some(new_transaction))
    }
}
