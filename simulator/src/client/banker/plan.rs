use std::{
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use dst_demo_server::bank::{Transaction, TransactionId};
use dst_demo_simulator_harness::{
    plan::InteractionPlan,
    random::{
        RNG,
        rand::rand::{Rng, seq::IteratorRandom as _},
    },
};
use rust_decimal::Decimal;
use strum::{EnumDiscriminants, EnumIter, IntoEnumIterator as _};

static CONTEXT: LazyLock<InteractionPlanContext> = LazyLock::new(InteractionPlanContext::new);

pub struct InteractionPlanContext {
    curr_id: Arc<RwLock<TransactionId>>,
    transactions: Arc<RwLock<Vec<Transaction>>>,
}

impl Default for InteractionPlanContext {
    fn default() -> Self {
        Self::new()
    }
}

impl InteractionPlanContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            curr_id: Arc::new(RwLock::new(1)),
            transactions: Arc::new(RwLock::new(vec![])),
        }
    }

    fn curr_id(&self) -> TransactionId {
        *self.curr_id.read().unwrap()
    }

    fn add_transaction(&self, transaction: Transaction) {
        self.transactions.write().unwrap().push(transaction);
        *self.curr_id.write().unwrap() += 1;
    }

    fn get_transaction(&self, id: TransactionId) -> Option<Transaction> {
        self.transactions
            .read()
            .unwrap()
            .iter()
            .find(|x| x.id == id)
            .cloned()
    }

    fn get_random_existing_transaction(&self, rng: &mut impl Rng) -> Option<Transaction> {
        self.transactions
            .read()
            .unwrap()
            .iter()
            .choose(&mut *rng)
            .cloned()
    }

    fn get_random_existing_transaction_id(&self, rng: &mut impl Rng) -> Option<TransactionId> {
        self.get_random_existing_transaction(rng).map(|x| x.id)
    }

    #[allow(unused)]
    fn clear(&self) {
        self.transactions.write().unwrap().clear();
        *self.curr_id.write().unwrap() = 1;
    }
}

pub struct BankerInteractionPlan {
    step: u64,
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
}

impl InteractionPlan<Interaction> for BankerInteractionPlan {
    fn step(&mut self) -> Option<&Interaction> {
        #[allow(clippy::cast_possible_truncation)]
        if let Some(item) = self.plan.get(self.step as usize) {
            self.step += 1;
            log::debug!("step: {}", self.step);
            Some(item)
        } else {
            None
        }
    }

    fn gen_interactions(&mut self, count: u64) {
        let len = self.plan.len() as u64;

        let rng: &dst_demo_simulator_harness::random::Rng = &RNG;
        let mut rng: dst_demo_simulator_harness::random::Rng = rng.clone();

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
                    let id = CONTEXT
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
                    let id = CONTEXT
                        .get_random_existing_transaction_id(&mut rng)
                        .unwrap_or_else(|| rng.r#gen());

                    self.add_interaction(Interaction::VoidTransaction { id });
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
            | Interaction::GetTransaction { .. } => {}
            Interaction::CreateTransaction { amount } => {
                CONTEXT.add_transaction(Transaction {
                    id: CONTEXT.curr_id(),
                    amount: *amount,
                    created_at: 0,
                });
            }
            Interaction::VoidTransaction { id } => {
                if let Some(existing) = CONTEXT.get_transaction(*id) {
                    CONTEXT.add_transaction(Transaction {
                        id: CONTEXT.curr_id(),
                        amount: existing.amount,
                        created_at: 0,
                    });
                }
            }
        }
        self.plan.push(interaction);
    }
}
