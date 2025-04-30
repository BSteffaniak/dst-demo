pub use tokio::runtime::Runtime;

use crate::{Error, runtime::Builder};

#[allow(unused)]
pub(crate) fn build_runtime(#[allow(unused)] builder: &Builder) -> Result<Runtime, Error> {
    #[cfg(feature = "rt-multi-thread")]
    {
        Ok(if let Some(threads) = builder.max_blocking_threads {
            tokio::runtime::Builder::new_multi_thread()
                .max_blocking_threads(threads as usize)
                .enable_io()
                .build()?
        } else {
            tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .build()?
        })
    }
    #[cfg(not(feature = "rt-multi-thread"))]
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()?)
}
