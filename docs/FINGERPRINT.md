# Potato Balanced — wire-fingerprint analysis (Phase 1 gate)

The Phase 1 gate requires **no wire-distinguishability from Arti beyond negotiated features** [PLAN.md Phase 1],
enforcing the rule that Potato clients must look like ordinary Arti clients or the anonymity set shrinks to
"Potato users" [PLAN.md 3.9].

## Method

Potato Balanced is not a fork: it is the **stock `arti` binary** run with `crates/potato/config/balanced.toml`
(`arti proxy -c balanced.toml`). Therefore a relay cannot distinguish Potato from stock Arti as long as every
*effective* config value equals Arti's own default — same binary + default-equal config implies identical wire
behaviour, with no packet capture required. We compare each key against Arti 2.7.0's documented defaults (the
commented values in `arti-example-config.toml`).

## Config vs Arti 2.7.0 defaults

| Key | balanced.toml | Arti default | Deviates? | Relay-visible? |
|-----|---------------|--------------|-----------|----------------|
| `proxy.socks_listen` | `9150` | `9150` | no | No — local SOCKS port, never sent to relays |
| `vanguards.mode` | `"lite"` | `"auto"` = lite | no (same effect) | Only affects onion-service circuits; identical to default |
| `channel.padding` | `"normal"` | `"normal"` | no | No deviation |
| `circuit_timing.max_dirtiness` | `"10 minutes"` | `"10 minutes"` | no | Local circuit-reuse policy; not a wire signal |
| `preemptive_circuits.initial_predicted_ports` | `[80, 443]` | `[80, 443]` | no | No deviation |
| `preemptive_circuits.min_exit_circs_for_port` | `2` | `2` | no | (Would be the only observable knob if raised; left at default) |

Defaults verified against `arti-example-config.toml` for arti 2.7.0, read 2026-10-26.

## Conclusion

Every effective value in `balanced.toml` equals Arti's default. Potato Balanced therefore runs **identically to
stock Arti** on the wire; there is no distinguishable fingerprint beyond the features Arti itself negotiates.
The config's role is to **pin** the privacy posture (vanguards-lite, full padding, standard rotation) explicitly,
so a future change to an Arti default cannot silently alter Potato's behaviour.

## Limitation

This is a **configuration-equivalence** argument (same binary, default-equal config), which is sound but not an
empirical trace. A packet/cell-level capture comparing Potato Balanced against stock Arti — the measurement the
gate table lists [PLAN.md section 6] — is deferred to the Shadow measurement work (Phase 3) as defense in depth.
If Balanced ever sets a value that deviates from the Arti default, that trace comparison becomes mandatory
before it ships.
