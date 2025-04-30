use std::{cell::RefCell, str::FromStr, sync::atomic::AtomicU32};

use dst_demo_server::{
    ServerAction,
    bank::{Transaction, TransactionId},
};
use dst_demo_simulator_harness::{
    CancellableSim, plan::InteractionPlan as _, time::simulator::step_multiplier,
    turmoil::net::TcpStream, utils::thread_id,
};
use plan::{BankerInteractionPlan, Interaction};
use rust_decimal::Decimal;
use tokio::io::AsyncWriteExt as _;

mod plan;

use crate::{
    host::server::{HOST, PORT},
    read_message,
};

thread_local! {
    static ID: RefCell<AtomicU32> = const { RefCell::new(AtomicU32::new(1)) };
}

pub fn reset_id() {
    ID.with_borrow(|x| x.store(1, std::sync::atomic::Ordering::SeqCst));
}

pub fn start(sim: &mut impl CancellableSim) {
    let server_addr = format!("{HOST}_{}:{PORT}", thread_id());

    let name = format!(
        "Banker{}",
        ID.with_borrow(|x| x.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    );

    log::debug!("[{name}] Generating initial test plan");

    let mut plan = BankerInteractionPlan::new().with_gen_interactions(1000);

    sim.client_until_cancelled(&name.clone(), async move {
        loop {
            while let Some(interaction) = plan.step().cloned() {
                static TIMEOUT: u64 = 10;

                #[allow(clippy::cast_possible_truncation)]
                let interaction_timeout = TIMEOUT * 1000
                    + if let Interaction::Sleep(duration) = &interaction {
                        duration.as_millis() as u64
                    } else {
                        0
                    } + step_multiplier() * 1000;

                tokio::select! {
                    resp = perform_interaction(&name, &server_addr, interaction, &plan) => {
                        resp?;
                        tokio::time::sleep(std::time::Duration::from_secs(step_multiplier() * 60)).await;
                    }
                    () = tokio::time::sleep(std::time::Duration::from_millis(interaction_timeout)) => {
                        return Err(Box::new(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            format!("[{name}] Failed to get interaction response within {interaction_timeout}ms")
                        )) as Box<dyn std::error::Error>);
                    }
                }
            }

            plan.gen_interactions(1000);
        }
    });
}

async fn send_action(
    name: &str,
    server_addr: &str,
    addr: &str,
    stream: &mut TcpStream,
    action: ServerAction,
) -> bool {
    log::debug!("[{name} {addr}->{server_addr}] send_action: action={action}");
    let success = send_message(name, server_addr, addr, stream, action.to_string()).await;
    log::debug!(
        "[{name} {addr}->{server_addr}] send_action: sent action={action} success={success}"
    );
    success
}

async fn send_message(
    name: &str,
    server_addr: &str,
    addr: &str,
    stream: &mut TcpStream,
    message: impl Into<String>,
) -> bool {
    let message = message.into();
    log::debug!("[{name} {addr}->{server_addr}] send_message: message={message}");
    let mut bytes = message.clone().into_bytes();
    bytes.push(0_u8);
    match stream.write_all(&bytes).await {
        Ok(resp) => resp,
        Err(e) => {
            log::error!("[{name} {addr}->{server_addr}] failed to make tcp_request: {e:?}");
            return false;
        }
    }
    log::debug!("[{name} {addr}->{server_addr}] send_message: sent message={message} success=true");

    true
}

#[allow(clippy::too_many_lines)]
async fn perform_interaction(
    name: &str,
    server_addr: &str,
    interaction: Interaction,
    plan: &BankerInteractionPlan,
) -> Result<(), Box<dyn std::error::Error>> {
    log::debug!("[{name}] perform_interaction: interaction={interaction:?}");

    if let Interaction::Sleep(duration) = interaction {
        log::debug!("[{name}] perform_interaction: sleeping for duration={duration:?}");
        tokio::time::sleep(duration).await;
        return Ok(());
    }

    loop {
        log::trace!("[{name}] Connecting to server...");
        let mut stream = match TcpStream::connect(server_addr).await {
            Ok(stream) => stream,
            Err(e) => {
                log::debug!("[{name}] Failed to connect to server: {e:?}");
                tokio::time::sleep(std::time::Duration::from_millis(step_multiplier())).await;
                continue;
            }
        };
        let addr = &stream.local_addr().unwrap().to_string();
        log::trace!("[{name} {addr}->{server_addr}] Connected!");

        match interaction {
            Interaction::Sleep(..) => {
                unreachable!();
            }
            Interaction::ListTransactions => {
                if !list_transactions(name, server_addr, addr, plan, &mut stream).await {
                    log::debug!(
                        "[{name} {addr}->{server_addr}] perform_interaction: list_transactions failed"
                    );
                    continue;
                }
            }
            Interaction::GetTransaction { id } => {
                if !get_transaction(id, name, server_addr, addr, &mut stream).await {
                    log::debug!(
                        "[{name} {addr}->{server_addr}] perform_interaction: get_transaction failed"
                    );
                    continue;
                }
            }
            Interaction::CreateTransaction { amount } => {
                if !create_transaction(amount, name, server_addr, addr, &mut stream).await {
                    log::debug!(
                        "[{name} {addr}->{server_addr}] perform_interaction: create_transaction failed"
                    );
                    continue;
                }
            }
            Interaction::VoidTransaction { id } => {
                if !void_transaction(id, name, server_addr, addr, &mut stream).await {
                    log::debug!(
                        "[{name} {addr}->{server_addr}] perform_interaction: void_transaction failed"
                    );
                    continue;
                }
            }
            Interaction::GetBalance => {
                if !get_balance(name, server_addr, addr, &mut stream).await {
                    log::debug!(
                        "[{name} {addr}->{server_addr}] perform_interaction: get_balance failed"
                    );
                    continue;
                }
            }
        }

        break;
    }

    log::debug!("[{name}] perform_interaction: finished interaction={interaction:?}");

    Ok(())
}

async fn get_transaction(
    id: TransactionId,
    name: &str,
    server_addr: &str,
    addr: &str,
    stream: &mut TcpStream,
) -> bool {
    if !send_action(
        name,
        server_addr,
        addr,
        stream,
        ServerAction::GetTransaction,
    )
    .await
    {
        log::debug!("[{name} {addr}->{server_addr}] get_transaction: failed to send");
        return false;
    }

    let message = match read_message(&mut String::new(), Box::pin(&mut *stream)).await {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[{name} {addr}->{server_addr}] get_transaction: failed to read: {e:?}");
            return false;
        }
    };
    let Some(message) = message else {
        log::debug!("[{name} {addr}->{server_addr}] get_transaction: failed to get response");
        return false;
    };

    assert!(
        message == "Enter the transaction ID:",
        "[{name} {addr}->{server_addr}] expected prompt for transaction ID, instead got:\n'{message}'"
    );
    if !send_message(name, server_addr, addr, stream, id.to_string()).await {
        log::debug!("[{name} {addr}->{server_addr}] get_transaction: id failed to send");
        return false;
    }

    let message = match read_message(&mut String::new(), Box::pin(stream)).await {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[{name} {addr}->{server_addr}] get_transaction: failed to read: {e:?}");
            return false;
        }
    };
    let Some(message) = message else {
        log::debug!("[{name} {addr}->{server_addr}] get_transaction: failed to get response");
        return false;
    };

    assert!(
        message == "Transaction not found"
            || Transaction::from_str(&message).is_ok_and(|x| x.id == id),
        "[{name} {addr}->{server_addr}] expected transaction response, instead got:\n'{message}'"
    );

    true
}
async fn list_transactions(
    name: &str,
    server_addr: &str,
    addr: &str,
    plan: &BankerInteractionPlan,
    stream: &mut TcpStream,
) -> bool {
    if !send_action(
        name,
        server_addr,
        addr,
        stream,
        ServerAction::ListTransactions,
    )
    .await
    {
        log::debug!("[{name} {addr}->{server_addr}] list_transactions: failed to send");
        return false;
    }
    let message = match read_message(&mut String::new(), Box::pin(stream)).await {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[{name} {addr}->{server_addr}] list_transactions: failed to read: {e:?}");
            return false;
        }
    };
    let Some(message) = message else {
        log::debug!("[{name} {addr}->{server_addr}] list_transactions: failed to get response");
        return false;
    };

    if message.is_empty() {
        log::debug!(
            "[{name} {addr}->{server_addr}] list_transactions: got 'not transactions' response"
        );
        return true;
    }

    let transactions = message.split('\n');
    let transactions = transactions
        .map(Transaction::from_str)
        .collect::<Result<Vec<Transaction>, _>>()
        .unwrap_or_else(|e| {
            panic!(
                "[{name} {addr}->{server_addr}] Invalid formatted transactions ({e:?}):\n{message}"
            )
        });

    let amounts = plan
        .plan
        .iter()
        .take(usize::try_from(plan.step).unwrap())
        .filter_map(|x| match x {
            Interaction::CreateTransaction { amount } => Some(amount),
            _ => None,
        })
        .collect::<Vec<_>>();

    log::debug!(
        "[{name} {addr}->{server_addr}] amounts.len={} transactions.len={}",
        amounts.len(),
        transactions.len(),
    );

    assert!(
        transactions.len() >= amounts.len(),
        "\
        [{name} {addr}->{server_addr}] expected at least {} transactions, but only saw {}\n\
        Actual transactions:\n\
        {message}\
        ",
        amounts.len(),
        transactions.len(),
    );

    for amount in amounts {
        assert!(
            transactions
                .iter()
                .any(|x| format!("{:.2}", x.amount) == format!("{amount:.2}")),
            "\
            [{name} {addr}->{server_addr}] missing transaction with amount={amount}\n\
            Actual transactions:\n\
            {message}\
            "
        );
    }

    true
}

async fn create_transaction(
    amount: Decimal,
    name: &str,
    server_addr: &str,
    addr: &str,
    stream: &mut TcpStream,
) -> bool {
    if !send_action(
        name,
        server_addr,
        addr,
        stream,
        ServerAction::CreateTransaction,
    )
    .await
    {
        log::debug!("[{name} {addr}->{server_addr}] create_transaction: failed to send");
        return false;
    }
    if !send_message(name, server_addr, addr, stream, amount.to_string()).await {
        log::debug!("[{name} {addr}->{server_addr}] create_transaction: amount failed to send");
        return false;
    }

    let message = match read_message(&mut String::new(), Box::pin(&mut *stream)).await {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[{name} {addr}->{server_addr}] create_transaction: failed to read: {e:?}");
            return false;
        }
    };
    let Some(message) = message else {
        log::debug!("[{name} {addr}->{server_addr}] create_transaction: failed to get prompt response");
        return false;
    };

    assert!(
        message == "Enter the transaction amount:",
        "[{name} {addr}->{server_addr}] expected prompt for transaction amount, instead got:\n'{message}'"
    );

    let message = match read_message(&mut String::new(), Box::pin(stream)).await {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[{name} {addr}->{server_addr}] create_transaction: failed to read: {e:?}");
            return false;
        }
    };
    let Some(message) = message else {
        log::debug!("[{name} {addr}->{server_addr}] create_transaction: failed to get transaction response");
        return false;
    };

    assert!(
        Transaction::from_str(&message).is_ok(),
        "[{name} {addr}->{server_addr}] expected to be able to parse create_transaction response as a transaction:\n'{message}'",
    );

    true
}

async fn void_transaction(
    id: TransactionId,
    name: &str,
    server_addr: &str,
    addr: &str,
    stream: &mut TcpStream,
) -> bool {
    if !send_action(
        name,
        server_addr,
        addr,
        stream,
        ServerAction::VoidTransaction,
    )
    .await
    {
        log::debug!("[{name} {addr}->{server_addr}] void_transaction: failed to send");
        return false;
    }
    if !send_message(name, server_addr, addr, stream, id.to_string()).await {
        log::debug!("[{name} {addr}->{server_addr}] void_transaction: id failed to send");
        return false;
    }

    let message = match read_message(&mut String::new(), Box::pin(stream)).await {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[{name} {addr}->{server_addr}] void_transaction: failed to read: {e:?}");
            return false;
        }
    };
    let Some(message) = message else {
        log::debug!("[{name} {addr}->{server_addr}] void_transaction: failed to get response");
        return false;
    };

    assert!(
        message == "Enter the transaction ID:",
        "[{name} {addr}->{server_addr}] expected prompt for transaction ID, instead got:\n'{message}'"
    );

    true
}

async fn get_balance(name: &str, server_addr: &str, addr: &str, stream: &mut TcpStream) -> bool {
    if !send_action(name, server_addr, addr, stream, ServerAction::GetBalance).await {
        log::debug!("[{name} {addr}->{server_addr}] get_balance: failed to send");
        return false;
    }

    let message = match read_message(&mut String::new(), Box::pin(stream)).await {
        Ok(x) => x,
        Err(e) => {
            log::debug!("[{name} {addr}->{server_addr}] get_balance: failed to read: {e:?}");
            return false;
        }
    };
    let Some(message) = message else {
        log::debug!("[{name} {addr}->{server_addr}] get_balance: failed to get response");
        return false;
    };

    assert!(message.starts_with('$'), "[{name} {addr}->{server_addr}] expected a monetary response");

    let message = message.strip_prefix('$').unwrap();

    assert!(
        Decimal::from_str(message).is_ok(),
        "[{name} {addr}->{server_addr}] [{name} {addr}->{server_addr}] expected a decimal balance"
    );

    true
}
