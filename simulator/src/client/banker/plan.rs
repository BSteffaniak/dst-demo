use std::time::Duration;

use dst_demo_server::bank::{Transaction, TransactionId};
use rust_decimal::Decimal;
use simvar::{
    plan::InteractionPlan,
    switchy::random::{
        rand::rand::{Rng, seq::IteratorRandom as _},
        rng,
    },
};
use strum::{EnumDiscriminants, EnumIter, IntoEnumIterator as _};

pub struct InteractionPlanContext {
    curr_id: TransactionId,
    transactions: Vec<Transaction>,
}

impl Default for InteractionPlanContext {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractionPlanContext {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            curr_id: 1,
            transactions: vec![],
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn, unused)]
    pub fn get_transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    fn get_random_existing_transaction(&self, rng: &mut impl Rng) -> Option<&Transaction> {
        self.transactions.iter().choose(&mut *rng)
    }

    fn get_random_existing_transaction_id(&self, rng: &mut impl Rng) -> Option<TransactionId> {
        self.get_random_existing_transaction(rng).map(|x| x.id)
    }

    #[allow(unused)]
    fn clear(&mut self) {
        self.transactions.clear();
        self.curr_id = 1;
    }
}

pub struct BankerInteractionPlan {
    pub context: InteractionPlanContext,
    pub step: u64,
    pub plan: Vec<Interaction>,
}

impl Default for BankerInteractionPlan {
    fn default() -> Self {
        Self::new()
    }
}

impl BankerInteractionPlan {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            context: InteractionPlanContext::new(),
            step: 0,
            plan: vec![],
        }
    }
}

#[derive(Clone, Debug, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
#[strum_discriminants(name(InteractionType))]
pub enum Interaction {
    Sleep(Duration),
    ListTransactions,
    GetTransaction { id: TransactionId },
    CreateTransaction { amount: Decimal },
    VoidTransaction { id: TransactionId },
    GetBalance,
}

impl InteractionPlan<Interaction> for BankerInteractionPlan {
    fn step(&mut self) -> Option<&Interaction> {
        #[allow(clippy::cast_possible_truncation)]
        if let Some(item) = self.plan.get(self.step as usize) {
            self.step += 1;
            log::trace!("step: {}", self.step);
            Some(item)
        } else {
            None
        }
    }

    fn gen_interactions(&mut self, count: u64) {
        let len = self.plan.len() as u64;

        let mut rng = rng();

        for i in 1..=count {
            let interaction_type = InteractionType::iter().choose(&mut rng).unwrap();
            log::trace!(
                "gen_interactions: generating interaction {i}/{count} ({}) interaction_type={interaction_type:?}",
                i + len
            );
            match interaction_type {
                InteractionType::Sleep => {
                    self.add_interaction(Interaction::Sleep(Duration::from_millis(
                        rng.gen_range(0..100_000),
                    )));
                }
                InteractionType::ListTransactions => {
                    self.add_interaction(Interaction::ListTransactions);
                }
                InteractionType::GetTransaction => {
                    let id = self
                        .context
                        .get_random_existing_transaction_id(&mut rng)
                        .unwrap_or_else(|| rng.r#gen());

                    self.add_interaction(Interaction::GetTransaction { id });
                }
                InteractionType::CreateTransaction => {
                    const RANGE: f64 = 100_000_000_000.0;
                    let amount = rng.gen_range(-RANGE..RANGE);
                    let amount = amount.try_into().unwrap();

                    self.add_interaction(Interaction::CreateTransaction { amount });
                }
                InteractionType::VoidTransaction => {
                    let id = self
                        .context
                        .get_random_existing_transaction_id(&mut rng)
                        .unwrap_or_else(|| rng.r#gen());

                    self.add_interaction(Interaction::VoidTransaction { id });
                }
                InteractionType::GetBalance => {
                    self.add_interaction(Interaction::GetBalance);
                }
            }
        }
        drop(rng);
    }

    fn add_interaction(&mut self, interaction: Interaction) {
        log::trace!("add_interaction: adding interaction interaction={interaction:?}");
        match &interaction {
            Interaction::Sleep(..)
            | Interaction::ListTransactions
            | Interaction::GetBalance
            | Interaction::GetTransaction { .. } => {}
            Interaction::CreateTransaction { amount } => {
                self.context.transactions.push(Transaction {
                    id: self.context.curr_id,
                    amount: *amount,
                    created_at: 0,
                });
                self.context.curr_id += 1;
            }
            Interaction::VoidTransaction { id } => {
                if let Some(existing) = self.context.transactions.iter().find(|x| x.id == *id) {
                    self.context.transactions.push(Transaction {
                        id: self.context.curr_id,
                        amount: existing.amount,
                        created_at: 0,
                    });
                    self.context.curr_id += 1;
                }
            }
        }
        self.plan.push(interaction);
    }
}
