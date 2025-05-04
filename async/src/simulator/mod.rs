pub mod futures;
pub mod runtime;
pub mod task;

#[cfg(feature = "io")]
pub mod io;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(feature = "time")]
pub mod time;
#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "macros")]
pub use ::futures::select;

#[cfg(feature = "macros")]
#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::runtime::Builder;

    use super::runtime::build_runtime;

    #[cfg(feature = "time")]
    #[test]
    fn can_select_2_futures() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async move {
            super::select! {
                () = super::time::sleep(Duration::from_millis(100)) => {},
                () = super::time::sleep(Duration::from_millis(200)) => {
                    panic!("Should have selected other future");
                },
            }
        });

        runtime.block_on(async move {
            super::select! {
                () = super::time::sleep(Duration::from_millis(200)) => {
                    panic!("Should have selected other future");
                },
                () = super::time::sleep(Duration::from_millis(100)) => {},
            }
        });

        runtime.wait().unwrap();
    }

    #[cfg(feature = "time")]
    #[test]
    fn can_select_3_futures() {
        let runtime = build_runtime(&Builder::new()).unwrap();

        runtime.block_on(async move {
            super::select! {
                () = super::time::sleep(Duration::from_millis(10)) => {},
                () = super::time::sleep(Duration::from_millis(100)) => {
                    panic!("Should have selected other future");
                },
                () = super::time::sleep(Duration::from_millis(200)) => {
                    panic!("Should have selected other future");
                },
            }
        });

        runtime.block_on(async move {
            super::select! {
                () = super::time::sleep(Duration::from_millis(10)) => {},
                () = super::time::sleep(Duration::from_millis(200)) => {
                    panic!("Should have selected other future");
                },
                () = super::time::sleep(Duration::from_millis(100)) => {
                    panic!("Should have selected other future");
                },
            }
        });

        runtime.block_on(async move {
            super::select! {
                () = super::time::sleep(Duration::from_millis(200)) => {
                    panic!("Should have selected other future");
                },
                () = super::time::sleep(Duration::from_millis(10)) => {},
                () = super::time::sleep(Duration::from_millis(100)) => {
                    panic!("Should have selected other future");
                },
            }
        });

        runtime.block_on(async move {
            super::select! {
                () = super::time::sleep(Duration::from_millis(200)) => {
                    panic!("Should have selected other future");
                },
                () = super::time::sleep(Duration::from_millis(100)) => {
                    panic!("Should have selected other future");
                },
                () = super::time::sleep(Duration::from_millis(10)) => {},
            }
        });

        runtime.wait().unwrap();
    }
}
