use std::time::Duration;

use dst_demo_server::bank::TransactionId;
use rust_decimal::Decimal;
use strum::{EnumDiscriminants, EnumIter};

#[derive(Clone, Debug, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(InteractionType))]
pub enum Interaction {
    Sleep(Duration),
    ListTransactions,
    GetTransaction { id: TransactionId },
    CreateTransaction { amount: Decimal },
    VoidTransaction { id: TransactionId },
}
