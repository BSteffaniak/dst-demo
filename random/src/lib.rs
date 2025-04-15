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

pub struct Rng(Box<dyn GenericRng>);

impl GenericRng for Rng {
    fn next_u64(&self) -> u64 {
        self.0.next_u64()
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng {
    #[must_use]
    pub fn new() -> Self {
        Self::from_seed(None)
    }

    /// # Panics
    ///
    /// * If no random backend features are enabled
    pub fn from_seed<S: Into<Option<u64>>>(seed: S) -> Self {
        #[cfg(feature = "simulator")]
        if dst_demo_simulator_utils::simulator_enabled() {
            return Self(Box::new(simulator::SimulatorRng::new(seed)));
        }

        if cfg!(feature = "rand") {
            #[cfg(feature = "rand")]
            {
                Self::from_seed_rand(seed)
            }
            #[cfg(not(feature = "rand"))]
            unreachable!()
        } else {
            panic!("No HTTP backend feature enabled (seed={:?})", seed.into());
        }
    }

    #[cfg(feature = "rand")]
    #[allow(unreachable_code)]
    pub fn from_seed_rand<S: Into<Option<u64>>>(seed: S) -> Self {
        #[cfg(feature = "simulator")]
        if dst_demo_simulator_utils::simulator_enabled() {
            return Self(Box::new(simulator::SimulatorRng::new(seed)));
        }

        Self(Box::new(rand::RandRng::new(seed)))
    }
}
