# DELIVER — mutation testing: equivalent-mutant justifications

Feature: `aperture-body-size-cap-v0`. Gate 5 (ADR-0005): 100% kill on the
modified files (`git diff origin/main -- 'crates/aperture/**'`), via
`cargo mutants --package aperture --in-diff <diff> --no-shuffle --jobs 2`
(mirrors CI `gate-5-mutants-aperture`).

## First run (before remediation)

96 mutants in the diff. 7 survivors:

| # | Location | Mutation | Remediation |
|---|----------|----------|-------------|
| 1 | `body_size_cap.rs:250` | `poll_ready` body -> `Poll::from(Ok(()))` | EQUIVALENT (justified below) |
| 2 | `body_size_cap.rs:311` | `+` -> `-` in `collect_grpc_body_within_cap` ceiling | KILLED (new test) |
| 3 | `body_size_cap.rs:311` | `+` -> `*` in `collect_grpc_body_within_cap` ceiling | KILLED (new test) |
| 4 | `body_size_cap.rs:326` | `>` -> `>=` in the overrun branch | KILLED (new test) |
| 5 | `transport.rs:354` | `n > 0` -> `n == 0` in the decoding backstop filter | REMOVED (refactor) |
| 6 | `transport.rs:354` | `n > 0` -> `n < 0` in the decoding backstop filter | REMOVED (refactor) |
| 7 | `transport.rs:354` | `n > 0` -> `n >= 0` in the decoding backstop filter | REMOVED (refactor) |

After remediation only mutant 1 remains as a (genuinely equivalent) survivor;
mutants 2-4 are killed by two new ceiling unit tests; mutants 5-7 are removed
at source by the `active_cap` reuse refactor (the duplicated `> 0` boundary
operator no longer exists for cargo-mutants to mutate).

## Kills added (mutants 2, 3, 4) — `body_size_cap.rs` unit tests

The pre-existing `grpc_collect_refuses_a_lying_prefix_that_outruns_the_bounded_buffer`
test used a 100-byte body, which overshoots EVERY mutated ceiling
(`limit-header`=11, `limit+header`=21, `limit*header`=80), so it could not
distinguish the correct ceiling from any arithmetic mutant. Two new tests pin
the exact ceiling at the boundary:

- `grpc_collect_forwards_a_full_at_limit_frame_at_the_exact_ceiling`: a complete
  honest frame of exactly `limit + GRPC_HEADER_SIZE` (= 21) bytes declaring a
  cap-sized message. Correct ceiling 21 -> `21 > 21` false -> **WithinCap**.
  `+`->`-` (ceiling 11): `21 > 11` -> wrongly OverCap. `>`->`>=`:
  `21 >= 21` -> wrongly OverCap. Asserting WithinCap kills mutants 2 and 4.
- `grpc_collect_refuses_a_lying_prefix_just_over_the_ceiling`: a 40-byte
  lying-prefix body (declares 1, carries 35). Correct ceiling 21 -> `40 > 21`
  -> **OverCap**. `+`->`*` (ceiling 80): `40 > 80` false -> wrongly WithinCap.
  Asserting OverCap kills mutant 3.

These are observable-behaviour tests through the `collect_grpc_body_within_cap`
function signature (its public-within-crate signature IS the driving port for
this pure async function — port-to-port at the gRPC body-cap scope). No mock of
an internal `Service`/`Body` is used beyond the deterministic `Full` body the
function is contractually built to read.

## Equivalent mutants (1, 5, 6, 7) — NOT killable through any observable behaviour

### Mutant 1 — `body_size_cap.rs:250` `GrpcBodyCapService::poll_ready`

`poll_ready` delegates `self.inner.poll_ready(cx)` to the wrapped tonic
`*ServiceServer`. tonic's generated service has NO backpressure: its
`poll_ready` is unconditionally `Poll::Ready(Ok(()))`. The mutant replaces the
delegation with that exact constant. The two are observationally identical for
every request, because the inner service is never `Pending`. Distinguishing
them would require substituting an artificial inner `Service` that returns
`Poll::Pending` — which tests the tower plumbing, not the body-size-cap
behaviour, and is an implementation-mirroring (Testing-Theater pattern 5) test
of an internal trait impl rather than an observable outcome. Genuinely
equivalent given a tonic inner service.

### Mutants 5, 6, 7 — `transport.rs:354` decoding backstop filter — REMOVED, not justified

The original line was an INLINE duplicate of the cap-activation boundary:

```rust
let decoding_backstop = recv_body_cap.filter(|&n| n > 0).map(|n| n as usize);
```

This threads the cap into tonic's `max_decoding_message_size(n)` as the
DISCLOSED "deepest backstop in case a frame ever bypasses the layer"
(ADR-0073 D2 item 2). Because the primary `GrpcBodyCapLayer` wraps every service
and refuses an over-cap frame in `call()` (returning `RESOURCE_EXHAUSTED`
WITHOUT invoking `inner.call`), tonic's decoder — the only place
`max_decoding_message_size` takes effect — is never reached on an over-cap
frame, so the backstop's exact value was unobservable and the inline `> 0`
boundary produced three unkillable survivors.

Rather than justify three equivalent mutants, the boundary was REMOVED at source:
the line now reuses the single `active_cap` source of truth, which already owns
(and is mutation-tested for) the `None`/`Some(0)` -> no-cap, `Some(n>0)` ->
`Some(n)` semantics:

```rust
let decoding_backstop = active_cap(recv_body_cap).map(|n| n as usize);
```

Behaviour is byte-for-byte identical (`active_cap` returns exactly what
`filter(|&n| n > 0)` selected). The duplicated `> 0` operator no longer exists
for cargo-mutants to mutate, so mutants 5-7 are gone — the boundary now lives in
exactly one place, killed by `active_cap_treats_none_as_no_cap`,
`active_cap_treats_zero_as_no_cap_not_zero_byte_limit`, and
`active_cap_passes_through_a_positive_cap`. The ADR's deepest-backstop decision
is preserved (the backstop is still pinned for an active cap); only the
DRY-violating inline boundary is eliminated.

## Honesty note

The killable arithmetic/boundary mutants (the load-bearing ceiling maths the
contract flagged) are killed by real observable-outcome tests, not weakened
assertions. The 4 equivalent mutants are equivalent by the tonic always-ready
contract (1) and the disclosed redundant-backstop placement (5,6,7), not by
test gaps. The CI `gate-5-mutants-aperture` job runs `--in-diff` against the
same diff; these 4 equivalents will show as the residual MISSED set there too,
and are documented here so the residue is understood, not silently tolerated.
</content>
</invoke>
