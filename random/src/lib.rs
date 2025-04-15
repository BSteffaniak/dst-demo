#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "rand")]
pub mod rand;

#[cfg(feature = "simulator")]
pub mod simulator;

pub trait GenericRng: Send + Sync {
    fn next_u64(&self) -> u64;
}

pub struct Rng<R: GenericRng>(R);

impl<R: GenericRng> GenericRng for Rng<R> {
    fn next_u64(&self) -> u64 {
        self.0.next_u64()
    }
}

impl<R: GenericRng> Rng<R> {
    #[must_use]
    #[cfg(feature = "simulator")]
    pub fn new() -> Rng<simulator::SimulatorRng> {
        Self::from_seed(None)
    }

    #[must_use]
    #[cfg(all(not(feature = "simulator"), feature = "rand"))]
    pub fn new() -> Rng<rand::RandRng> {
        Self::from_seed(None)
    }

    #[cfg(feature = "simulator")]
    pub fn from_seed<S: Into<Option<u64>>>(seed: S) -> Rng<simulator::SimulatorRng> {
        Rng(simulator::SimulatorRng::new(seed))
    }

    #[cfg(all(not(feature = "simulator"), feature = "rand"))]
    pub fn from_seed<S: Into<Option<u64>>>(seed: S) -> Rng<rand::RandRng> {
        Rng(rand::RandRng::new(seed))
    }
}
