use std::{sync::LazyLock, time::Duration};

use dst_demo_async::{Error, runtime::Builder, time, util::CancellationToken};

static TOKEN: LazyLock<CancellationToken> = LazyLock::new(CancellationToken::new);

fn ctrl_c() {
    println!("ctrl+c received. shutting runtime down...");
    TOKEN.cancel();
}

fn main() -> Result<(), Error> {
    ctrlc::set_handler(ctrl_c).unwrap();

    pretty_env_logger::init();

    let runtime = Builder::new().build()?;

    runtime.block_on(TOKEN.run_until_cancelled(async move {
        println!("Blocking Function. Press ctrl+c to exit");
        time::sleep(Duration::MAX).await;
        println!("Blocking Function Polled To Completion");
    }));
    println!("After block_on");

    runtime.wait()?;
    println!("Runtime shut down cleanly");

    Ok(())
}
