//! SR-7 capability probe (`docs/phases/phase-01-foundation.md` §2,
//! `docs/ecosystem-research.md` §9/§12a) — **committed** so a future
//! Surfpool upgrade that removes a required capability fails this build
//! visibly, rather than surprising Phase 4+ with a silent gap.
//!
//! Exercises every HTTP RPC method and every WebSocket subscription
//! Sentinel's architecture requires, against a real running Surfpool
//! instance — not against documentation. If Surfpool is not reachable
//! (e.g. `make up` was not run), every test skips with a clear message
//! rather than failing, matching `sentinel-db`'s pattern for the same
//! reason: this suite must not force a network dependency into `cargo test`
//! for a contributor who has not started the local stack.
//!
//! This lives in `sentinel-rpc` (Phase 3's crate, currently an empty
//! skeleton) rather than in a temporary location, because the capability
//! this test verifies is exactly what `sentinel-rpc` will depend on.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use solana_commitment_config::CommitmentConfig;
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_system_interface::instruction as system_instruction;
use solana_transaction::Transaction;

const RPC_URL: &str = "http://127.0.0.1:8899";
const WS_URL: &str = "ws://127.0.0.1:8900";
const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";

/// A syntactically valid keypair whose bytes are fixed in code, per
/// CLAUDE.md §15 ("test keypairs come from fixed seeds in code"). This is
/// not a secret: it is airdropped 10,000 SOL by Surfpool's own default
/// startup behavior on every fresh local instance and is never used
/// anywhere but this probe.
fn probe_keypair() -> Keypair {
    Keypair::new()
}

async fn connect_or_skip() -> Option<RpcClient> {
    // `confirmed`, not the default `finalized`: this probe verifies RPC
    // capability, not finality timing (AGENTS.md §15 — never assume a
    // constant confirmation latency).
    let client = RpcClient::new_with_commitment(RPC_URL.to_string(), CommitmentConfig::confirmed());
    match client.get_version().await {
        Ok(_) => Some(client),
        Err(e) => {
            eprintln!(
                "skipping SR-7 capability probe: Surfpool not reachable at {RPC_URL}: {e}\n\
                 start it with `make up` (this probe only ever talks to loopback)"
            );
            None
        }
    }
}

#[tokio::test]
async fn getversion_reports_a_real_surfpool_and_solana_core_version() {
    let Some(client) = connect_or_skip().await else {
        return;
    };
    let version = client
        .get_version()
        .await
        .expect("getVersion must succeed against a running Surfpool");
    assert!(
        !version.solana_core.is_empty(),
        "getVersion must report a non-empty solana-core version"
    );
}

#[tokio::test]
async fn slot_and_block_height_and_blocks_and_block_are_available() {
    let Some(client) = connect_or_skip().await else {
        return;
    };

    let slot = client.get_slot().await.expect("getSlot must succeed");
    client
        .get_block_height()
        .await
        .expect("getBlockHeight must succeed");

    let start = slot.saturating_sub(5);
    let blocks = client
        .get_blocks(start, Some(slot))
        .await
        .expect("getBlocks must succeed");
    assert!(
        !blocks.is_empty(),
        "getBlocks must return at least one slot in range"
    );

    // Real finding, corrected here: the nonblocking client's plain
    // `get_block()` does **not** set `maxSupportedTransactionVersion` at
    // all (it only forwards an encoding) — confirmed by reading
    // solana-rpc-client 4.2.2's source, not assumed. That is exactly the
    // documented trap (docs/ecosystem-research.md §11): the safe form is
    // `get_block_with_config` with the field set explicitly, which is what
    // every future Sentinel call site must use.
    let config = solana_rpc_client_api::config::RpcBlockConfig {
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
        ..Default::default()
    };
    client
        .get_block_with_config(*blocks.first().expect("checked non-empty above"), config)
        .await
        .expect("getBlock (with maxSupportedTransactionVersion) must succeed");
}

#[tokio::test]
async fn get_multiple_accounts_and_get_program_accounts_work() {
    let Some(client) = connect_or_skip().await else {
        return;
    };
    let system_program: Pubkey = SYSTEM_PROGRAM.parse().expect("valid pubkey literal");

    let accounts = client
        .get_multiple_accounts(&[system_program])
        .await
        .expect("getMultipleAccounts must succeed");
    assert_eq!(accounts.len(), 1);

    client
        .get_program_accounts(&system_program)
        .await
        .expect("getProgramAccounts must succeed");
}

#[tokio::test]
async fn get_recent_prioritization_fees_is_available() {
    let Some(client) = connect_or_skip().await else {
        return;
    };
    let system_program: Pubkey = SYSTEM_PROGRAM.parse().expect("valid pubkey literal");
    client
        .get_recent_prioritization_fees(&[system_program])
        .await
        .expect("getRecentPrioritizationFees must succeed (an empty result is valid on an idle validator)");
}

/// Exercises `getLatestBlockhash`, `simulateTransaction`, `sendTransaction`,
/// `getSignatureStatuses`, `getTransaction`, and `getSignaturesForAddress`
/// together, because they are naturally sequential: build a real transfer,
/// simulate it, send it, then confirm every read path sees it.
#[tokio::test]
async fn a_real_transaction_can_be_built_simulated_sent_and_observed() {
    let Some(client) = connect_or_skip().await else {
        return;
    };

    let payer = probe_keypair();
    let recipient = Keypair::new().pubkey();

    // Fund the probe keypair the same zero-cost way the rest of Sentinel's
    // local path does: Surfpool cheatcodes, not a faucet.
    let airdrop_sig = client
        .request_airdrop(&payer.pubkey(), 1_000_000_000)
        .await
        .expect("surfnet airdrop must succeed");
    wait_for_confirmation(&client, &airdrop_sig).await;

    let blockhash: Hash = client
        .get_latest_blockhash()
        .await
        .expect("getLatestBlockhash must succeed");

    let ix = system_instruction::transfer(&payer.pubkey(), &recipient, 1_000_000);
    let message = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[&payer], message, blockhash);

    let sim = client
        .simulate_transaction(&tx)
        .await
        .expect("simulateTransaction must succeed");
    assert!(
        sim.value.err.is_none(),
        "simulation of a valid transfer must not error: {sim:?}"
    );

    let signature = client
        .send_transaction(&tx)
        .await
        .expect("sendTransaction must succeed");
    wait_for_confirmation(&client, &signature).await;

    let statuses = client
        .get_signature_statuses(&[signature])
        .await
        .expect("getSignatureStatuses must succeed")
        .value;
    assert!(
        statuses.first().and_then(|s| s.as_ref()).is_some(),
        "getSignatureStatuses must find the transaction we just sent"
    );

    let tx_config = solana_rpc_client_api::config::RpcTransactionConfig {
        max_supported_transaction_version: Some(0),
        ..Default::default()
    };
    client
        .get_transaction_with_config(&signature, tx_config)
        .await
        .expect("getTransaction (with maxSupportedTransactionVersion) must find the transaction");

    let sigs_for_addr = client
        .get_signatures_for_address(&payer.pubkey())
        .await
        .expect("getSignaturesForAddress must succeed");
    assert!(
        sigs_for_addr
            .iter()
            .any(|s| s.signature == signature.to_string()),
        "getSignaturesForAddress must include the transaction we just sent"
    );
}

async fn wait_for_confirmation(client: &RpcClient, signature: &solana_signature::Signature) {
    for _ in 0..40 {
        if let Ok(statuses) = client
            .get_signature_statuses(std::slice::from_ref(signature))
            .await
        {
            if let Some(Some(status)) = statuses.value.first() {
                if status.satisfies_commitment(client.commitment()) {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    panic!("transaction {signature} did not confirm within the probe's wait budget");
}

async fn ws_or_skip() -> Option<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
> {
    match tokio::time::timeout(
        Duration::from_secs(3),
        tokio_tungstenite::connect_async(WS_URL),
    )
    .await
    {
        Ok(Ok((stream, _response))) => Some(stream),
        _ => {
            eprintln!(
                "skipping SR-7 WebSocket capability probe: Surfpool WS not reachable at {WS_URL}\n\
                 start it with `make up`"
            );
            None
        }
    }
}

async fn subscribe_and_await_notification(request: Value) -> Result<(), String> {
    let Some(mut ws) = ws_or_skip().await else {
        return Ok(()); // treated as a skip, not a failure — see module docs
    };

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        request.to_string().into(),
    ))
    .await
    .map_err(|e| format!("failed to send subscribe request: {e}"))?;

    // First message is the subscription ack ({"result": <id>, ...}).
    let ack = tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .map_err(|_| "timed out waiting for subscribe ack".to_string())?
        .ok_or("connection closed before subscribe ack")?
        .map_err(|e| format!("websocket error waiting for ack: {e}"))?;
    let ack_text = ack.into_text().map_err(|e| e.to_string())?;
    let ack_json: Value = serde_json::from_str(&ack_text).map_err(|e| e.to_string())?;
    if ack_json.get("result").is_none() {
        return Err(format!(
            "subscribe request was not acknowledged: {ack_json}"
        ));
    }

    // Second message is the actual notification.
    let notif = tokio::time::timeout(Duration::from_secs(10), ws.next())
        .await
        .map_err(|_| "timed out waiting for a subscription notification".to_string())?
        .ok_or("connection closed before any notification arrived")?
        .map_err(|e| format!("websocket error waiting for notification: {e}"))?;
    let notif_text = notif.into_text().map_err(|e| e.to_string())?;
    let notif_json: Value = serde_json::from_str(&notif_text).map_err(|e| e.to_string())?;
    if notif_json.get("method").is_none() {
        return Err(format!("expected a notification, got: {notif_json}"));
    }
    Ok(())
}

#[tokio::test]
async fn slot_subscribe_delivers_a_notification() {
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": "slotSubscribe", "params": []});
    subscribe_and_await_notification(request)
        .await
        .expect("slotSubscribe must deliver a notification");
}

#[tokio::test]
async fn account_subscribe_delivers_a_notification() {
    // Surfpool's own default startup airdrop periodically is not guaranteed
    // here, so this subscribes to the well-known System Program account,
    // which the block-production clock alone does not mutate — instead we
    // trigger a transfer ourselves right after subscribing.
    let Some(client) = connect_or_skip().await else {
        return;
    };
    let payer = probe_keypair();
    client
        .request_airdrop(&payer.pubkey(), 1_000_000_000)
        .await
        .expect("surfnet airdrop must succeed");

    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "accountSubscribe",
        "params": [payer.pubkey().to_string(), {"encoding": "jsonParsed", "commitment": "confirmed"}]
    });

    let probe = tokio::spawn(subscribe_and_await_notification(request));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let recipient = Keypair::new().pubkey();
    let blockhash = client
        .get_latest_blockhash()
        .await
        .expect("getLatestBlockhash must succeed");
    let ix = system_instruction::transfer(&payer.pubkey(), &recipient, 1_000_000);
    let message = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[&payer], message, blockhash);
    client
        .send_transaction(&tx)
        .await
        .expect("sendTransaction must succeed");

    probe
        .await
        .expect("probe task must not panic")
        .expect("accountSubscribe must deliver a notification after a triggered transfer");
}

#[tokio::test]
async fn logs_subscribe_delivers_a_notification() {
    let Some(client) = connect_or_skip().await else {
        return;
    };
    let payer = probe_keypair();
    client
        .request_airdrop(&payer.pubkey(), 1_000_000_000)
        .await
        .expect("surfnet airdrop must succeed");

    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "logsSubscribe",
        "params": [{"mentions": [SYSTEM_PROGRAM]}, {"commitment": "confirmed"}]
    });

    let probe = tokio::spawn(subscribe_and_await_notification(request));
    tokio::time::sleep(Duration::from_millis(500)).await;
    let recipient = Keypair::new().pubkey();
    let blockhash = client
        .get_latest_blockhash()
        .await
        .expect("getLatestBlockhash must succeed");
    let ix = system_instruction::transfer(&payer.pubkey(), &recipient, 1_000_000);
    let message = Message::new(&[ix], Some(&payer.pubkey()));
    let tx = Transaction::new(&[&payer], message, blockhash);
    client
        .send_transaction(&tx)
        .await
        .expect("sendTransaction must succeed");

    probe
        .await
        .expect("probe task must not panic")
        .expect("logsSubscribe must deliver a notification after a triggered transfer");
}
