#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::{
    io::{Read as _, Write},
    path::PathBuf,
    sync::{Arc, Mutex, RwLock, RwLockReadGuard},
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
    fn list_transactions(&self) -> Result<RwLockReadGuard<Vec<Transaction>>, Error>;

    /// # Errors
    ///
    /// * If the `Bank` implementation fails to get the `Transaction`
    fn get_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, Error>;

    /// # Errors
    ///
    /// * If the `Bank` implementation fails to create the `Transaction`
    fn create_transaction(&self, amount: Decimal) -> Result<Transaction, Error>;

    /// # Errors
    ///
    /// * If the `Bank` implementation fails to void the `Transaction`
    fn void_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, Error>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: TransactionId,
    pub amount: Decimal,
    pub created_at: CreateTime,
}

impl std::fmt::Display for Transaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "id={} created_at={} amount=${:.2}",
            self.id, self.created_at, self.amount
        ))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionFromStrError {
    #[error("Missing id")]
    MissingId,
    #[error("Missing created_at")]
    MissingCreatedAt,
    #[error("Missing amount")]
    MissingAmount,
    #[error(transparent)]
    ParseInt(#[from] std::num::ParseIntError),
    #[error(transparent)]
    FromStrDecimal(#[from] rust_decimal::Error),
}

impl std::str::FromStr for Transaction {
    type Err = TransactionFromStrError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut components = s.split(' ');

        let id = components
            .next()
            .ok_or(TransactionFromStrError::MissingId)?;
        let id = &id["id=".len()..];
        let id = id.parse::<TransactionId>()?;

        let created_at = components
            .next()
            .ok_or(TransactionFromStrError::MissingCreatedAt)?;
        let created_at = &created_at["created_at=".len()..];
        let created_at = created_at.parse::<CreateTime>()?;

        let amount = components
            .next()
            .ok_or(TransactionFromStrError::MissingCreatedAt)?;
        let amount = &amount["amount=$".len()..];
        let amount = Decimal::from_str(amount)?;

        Ok(Self {
            id,
            amount,
            created_at,
        })
    }
}

#[derive(Clone)]
pub struct LocalBank {
    file: Arc<Mutex<File>>,
    transactions: Arc<RwLock<Vec<Transaction>>>,
    current_id: Arc<RwLock<TransactionId>>,
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
            file: Arc::new(Mutex::new(file)),
            current_id: Arc::new(RwLock::new(transactions.last().map_or(1, |x| x.id + 1))),
            transactions: Arc::new(RwLock::new(transactions)),
        })
    }
}

impl Bank for LocalBank {
    fn list_transactions(&self) -> Result<RwLockReadGuard<Vec<Transaction>>, Error> {
        Ok(self.transactions.read().unwrap())
    }

    fn get_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, Error> {
        Ok(self
            .transactions
            .read()
            .unwrap()
            .iter()
            .find(|x| x.id == id)
            .cloned())
    }

    fn create_transaction(&self, amount: Decimal) -> Result<Transaction, Error> {
        let id = {
            let mut binding = self.current_id.write().unwrap();
            let id = *binding;
            *binding += 1;
            id
        };
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
        {
            let binding = self.transactions.read().unwrap();
            let last_transaction = binding.last();
            assert!(
                last_transaction.is_none_or(|x| transaction.id == x.id + 1),
                "expected id to be last transaction.id + 1 last_transaction.id={} to transaction_id={}",
                last_transaction.unwrap().id,
                transaction.id,
            );
            drop(binding);
        }
        {
            let current_id = *self.current_id.read().unwrap();
            assert!(
                current_id > transaction.id,
                "id went backwards from current_id={current_id} to {}",
                transaction.id,
            );
        }
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
        self.file.lock().unwrap().write_all(serialized.as_bytes())?;

        self.transactions.write().unwrap().push(transaction.clone());
        Ok(transaction)
    }

    fn void_transaction(&self, id: TransactionId) -> Result<Option<Transaction>, Error> {
        let Some(existing) = self
            .transactions
            .read()
            .unwrap()
            .iter()
            .find(|x| x.id == id)
            .cloned()
        else {
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
