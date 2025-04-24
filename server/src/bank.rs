#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::time::SystemTime;

use rust_decimal::Decimal;

pub type TransactionId = i32;
pub type CreateTime = i32;

#[derive(Debug, thiserror::Error)]
pub enum Error {}

pub trait Bank: Send + Sync {
    /// # Errors
    ///
    /// * If the `Bank` implementation fails to list the `Transaction`s
    fn list_transactions(&self) -> Result<&[Transaction], Error>;

    /// # Errors
    ///
    /// * If the `Bank` implementation fails to get the `Transaction`
    fn get_transaction(&self, id: TransactionId) -> Result<Option<&Transaction>, Error>;

    /// # Errors
    ///
    /// * If the `Bank` implementation fails to create the `Transaction`
    fn create_transaction(&mut self, amount: Decimal) -> Result<&Transaction, Error>;

    /// # Errors
    ///
    /// * If the `Bank` implementation fails to void the `Transaction`
    fn void_transaction(&mut self, id: TransactionId) -> Result<Option<&Transaction>, Error>;
}

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: TransactionId,
    pub amount: Decimal,
    pub created_at: CreateTime,
}

pub struct LocalBank {
    transactions: Vec<Transaction>,
    current_id: TransactionId,
}

impl Default for LocalBank {
    fn default() -> Self {
        Self::new()
    }
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

    fn get_transaction(&self, id: TransactionId) -> Result<Option<&Transaction>, Error> {
        Ok(self.transactions.iter().find(|x| x.id == id))
    }

    fn create_transaction(&mut self, amount: Decimal) -> Result<&Transaction, Error> {
        let id = self.current_id;
        self.current_id += 1;
        let now = dst_demo_time::now();
        let seconds_since_epoch = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let transaction = Transaction {
            id,
            amount,
            created_at: seconds_since_epoch as CreateTime,
        };
        assert!(
            self.current_id > transaction.id,
            "Invalid id={}",
            transaction.id
        );
        assert!(
            transaction.created_at > 0,
            "created_at={} must be > 0",
            transaction.created_at
        );
        assert!(
            seconds_since_epoch >= transaction.created_at as u64,
            "Time went backwards {now:?} seconds_since_epoch={seconds_since_epoch} created_at={}",
            transaction.created_at,
        );
        self.transactions.push(transaction);
        Ok(self.transactions.last().unwrap())
    }

    fn void_transaction(&mut self, id: TransactionId) -> Result<Option<&Transaction>, Error> {
        let Some(existing) = self.transactions.iter().find(|x| x.id == id) else {
            return Ok(None);
        };

        let originally_created_at = existing.created_at;

        let new_transaction = self.create_transaction(-existing.amount)?;

        assert!(
            new_transaction.created_at >= originally_created_at,
            "Time went backwards new_transaction.created_at={} originally_created_at={originally_created_at}",
            new_transaction.created_at
        );

        Ok(Some(new_transaction))
    }
}
