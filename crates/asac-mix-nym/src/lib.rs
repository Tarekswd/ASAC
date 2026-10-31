//! Live Nym (Sphinx mixnet) backend for [`asac_mix::MixTransport`] (EXTERNAL-PLAN.md item 3; FLAGSHIP-DESIGN.md
//! §10). This is the keystone that joins the three layers: the `asac-mix` scheduler produces fixed-size framed
//! packets (a real query and a cover message are identical on the wire), and this crate pushes each one through
//! the Nym mixnet as a Sphinx packet — so a network observer sees only a constant-rate stream of uniform
//! packets, carried on a deployed mixnet's crowd. We never roll crypto: Sphinx and mixing are Nym's.
//!
//! The `MixTransport` seam is synchronous but `nym-sdk` is async, so [`NymTransport`] owns a tokio runtime and
//! blocks on each send.
//!
//! shortcut: `send` is one-way with no reply SURBs (`IncludedSurbs::none()`) — the search service answering
//! back over the mixnet is later work; add SURB/reply handling then. shortcut: `send` calls
//! `Runtime::block_on`, so it must NOT be called from inside a tokio runtime thread; the intended live loop
//! sleeps the scheduler delay on a plain thread, then calls `send`. Upgrade to a channel+task bridge if a
//! caller needs to send from async context.

use asac_mix::{MixError, MixTransport};
use nym_sdk::mixnet::{IncludedSurbs, MixnetClient, MixnetMessageSender, Recipient};
use tokio::runtime::Runtime;

/// A live mixnet transport backed by Nym. Owns its own tokio runtime and a connected ephemeral mixnet client;
/// each [`send`](MixTransport::send) submits one framed packet to `recipient` as a Sphinx packet.
pub struct NymTransport {
    rt: Runtime,
    client: MixnetClient,
    recipient: Recipient,
}

impl NymTransport {
    /// Connect an ephemeral Nym client aimed at `recipient` (a Nym address string, e.g. the search service's
    /// address). The address is parsed *before* any network use, so a malformed address fails fast and offline.
    /// Blocks until the client has connected to an entry gateway.
    pub fn connect(recipient: &str) -> Result<Self, MixError> {
        let recipient: Recipient = recipient
            .parse()
            .map_err(|e| MixError(format!("bad Nym recipient: {e}")))?;
        let rt = Runtime::new().map_err(|e| MixError(format!("tokio runtime: {e}")))?;
        let client = rt
            .block_on(MixnetClient::connect_new())
            .map_err(|e| MixError(format!("Nym connect: {e}")))?;
        Ok(Self { rt, client, recipient })
    }

    /// This client's own Nym address (where a reply would be sent).
    pub fn nym_address(&self) -> Recipient {
        *self.client.nym_address()
    }
}

impl MixTransport for NymTransport {
    fn send(&self, packet: &[u8]) -> Result<(), MixError> {
        self.rt
            .block_on(self.client.send_message(self.recipient, packet, IncludedSurbs::none()))
            .map_err(|e| MixError(format!("Nym send: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Offline self-check: a malformed recipient is rejected at parse time, before touching the network.
    #[test]
    fn bad_recipient_is_rejected_without_network() {
        // Not unwrap_err(): NymTransport holds a tokio Runtime and isn't Debug, so match on the result.
        match NymTransport::connect("not-a-nym-address") {
            Err(e) => assert!(e.0.contains("bad Nym recipient"), "unexpected error: {}", e.0),
            Ok(_) => panic!("expected a parse error for a malformed recipient"),
        }
    }

    // Live smoke: needs a reachable Nym gateway + network. Set NYM_TEST_RECIPIENT to a valid Nym address and run
    // `cargo test -p asac-mix-nym -- --ignored`. Sends one framed packet and expects the send to be queued.
    #[test]
    #[ignore = "requires a live Nym network + gateway and NYM_TEST_RECIPIENT"]
    fn live_send() {
        let to = std::env::var("NYM_TEST_RECIPIENT").expect("set NYM_TEST_RECIPIENT");
        let t = NymTransport::connect(&to).expect("connect");
        let pkt = asac_mix::frame(b"private search", 1024).expect("frame");
        t.send(&pkt).expect("send");
    }
}
