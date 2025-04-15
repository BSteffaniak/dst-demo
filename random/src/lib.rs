#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "rand")]
pub mod rand;

#[cfg(feature = "simulator")]
pub mod simulator;

#[cfg(feature = "simulator")]
pub type Rng = RngWrapper<simulator::SimulatorRng>;

#[cfg(all(not(feature = "simulator"), feature = "rand"))]
pub type Rng = RngWrapper<rand::RandRng>;

#[cfg(feature = "simulator")]
impl Default for Rng {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "simulator")]
impl Rng {
    #[must_use]
    pub fn new() -> Self {
        Self::from_seed(None)
    }

    pub fn from_seed<S: Into<Option<u64>>>(seed: S) -> Self {
        Self(simulator::SimulatorRng::new(seed))
    }

    #[inline]
    #[must_use]
    pub fn next_u64(&self) -> u64 {
        <Self as GenericRng>::next_u64(self)
    }
}

#[cfg(all(not(feature = "simulator"), feature = "rand"))]
impl Rng {
    #[must_use]
    pub fn new() -> Self {
        Self::from_seed(None)
    }

    pub fn from_seed<S: Into<Option<u64>>>(seed: S) -> Self {
        Self(rand::RandRng::new(seed))
    }

    #[inline]
    #[must_use]
    pub fn next_u64(&self) -> u64 {
        <Self as GenericRng>::next_u64(self)
    }
}

#[cfg(all(not(feature = "simulator"), feature = "rand"))]
impl Default for Rng {
    fn default() -> Self {
        Self::new()
    }
}

pub trait GenericRng: Send + Sync {
    fn next_u64(&self) -> u64;
}

pub struct RngWrapper<R: GenericRng>(R);

impl<R: GenericRng> GenericRng for RngWrapper<R> {
    #[inline]
    fn next_u64(&self) -> u64 {
        self.0.next_u64()
    }
}
