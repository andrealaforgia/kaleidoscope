# Slice 01 — RuleStateStore seam + InMemory adapter

`@infrastructure`

## Summary

Introduce a `RuleStateStore` trait and an `InMemoryRuleStateStore`
adapter. Rewire `run_rule` in `crates/beacon-server/src/main.rs` so the
per-rule loop reads/writes state through the store instead of the
`let mut state = RuleState::Inactive` local (line 146). Behaviour-
preserving: state still lives in memory and is still lost on restart.

## Stories

- US-01

## Carpaccio taste tests

| Test | Result |
|------|--------|
| Does it deliver an operator-visible behaviour change on its own? | No — it is a pure refactor. Honestly `@infrastructure`. |
| Is it thin (one outcome, end to end at the code level)? | Yes — one seam, no new behaviour. |
| Does it unblock the value slice? | Yes — slice 02 plugs a durable adapter into this trait without touching `transition`. |
| Could it ship alone as the feature's value? | No — and it must not. Slice 02 carries the value. |

## Why it does not block the feature

Slice 01 has no Elevator Pitch because it produces no user decision. It
is permitted as a labelled `@infrastructure` slice ONLY because slice 02
delivers the operator value end to end. Per the reviewer's slice-level
check, the feature as a whole contains user-visible stories (US-02,
US-03 in slice 02), so the feature is not all-infrastructure.

## Constraints carried

- ADR-0037: `transition` untouched.
- Trait shape mirrors `crates/lumen/src/store.rs` (`LogStore` +
  `InMemoryLogStore`).

## Dependencies

None. Must land before slice 02.
