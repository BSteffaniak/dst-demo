#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "tokio")]
pub mod tokio;

#[cfg(feature = "simulator")]
pub mod simulator;

pub mod runtime;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Join")]
    Join,
}

#[allow(unused)]
macro_rules! impl_async {
    ($module:ident $(,)?) => {
        pub use $module::task;

        #[cfg(feature = "io")]
        pub use $module::io;
        #[cfg(feature = "sync")]
        pub use $module::sync;

        impl crate::runtime::Builder {
            /// # Errors
            ///
            /// * If the underlying `Runtime` fails to build
            pub fn build(&self) -> Result<$module::runtime::Runtime, Error> {
                $module::runtime::build_runtime(self)
            }
        }
    };
}

#[cfg(feature = "simulator")]
impl_async!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "tokio"))]
impl_async!(tokio);
