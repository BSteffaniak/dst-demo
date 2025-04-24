#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    io::{Read as _, Write},
    path::PathBuf,
    time::SystemTime,
};

use dst_demo_fs::sync::{File, OpenOptions};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub type TransactionId = i32;
pub type CreateTime = i32;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub amount: Decimal,
    pub created_at: CreateTime,
}

pub struct LocalBank {
    file: File,
    transactions: Vec<Transaction>,
    current_id: TransactionId,
}

impl LocalBank {
    /// # Errors
    ///
    /// * If there is IO error reading existing transactions from the filesystem
    pub fn new() -> Result<Self, std::io::Error> {
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("transactions.db"))?;

        let mut transactions = String::new();
        file.read_to_string(&mut transactions)?;
        let transactions = transactions
            .split('\n')
            .filter(|x| !x.is_empty())
            .map(serde_json::from_str)
            .collect::<Result<Vec<Transaction>, _>>()?;

        Ok(Self {
            file,
            current_id: transactions.last().map_or(1, |x| x.id + 1),
            transactions,
        })
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
            self.transactions
                .last()
                .is_none_or(|x| transaction.id > x.id),
            "id went backwards from last_transaction.id={} to {}",
            self.transactions.last().unwrap().id,
            transaction.id,
        );
        assert!(
            self.current_id > transaction.id,
            "id went backwards from current_id={} to {}",
            self.current_id,
            transaction.id,
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

        let mut serialized = serde_json::to_string(&transaction)?;
        serialized.push('\n');
        self.file.write_all(serialized.as_bytes())?;

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
