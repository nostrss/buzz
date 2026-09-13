---
status: accepted
date: 2026-09-13
---

# The account service holds every person's identity key

Upstream Buzz makes each person generate and back up their own Nostr key
("your keys, your identity", VISION_SOVEREIGN.md). Non-developers stalled at
that screen, so the hosted fork moves key custody to the account service: a
person signs in with an email and a 6-digit code, and the account service
issues, encrypts, and stores the identity key on their behalf. The desktop app
never shows or exports the key.

## Considered options

- **Custodial (chosen)**: account service owns the key. Only option with a
  working "forgot password"; no relay changes.
- **Semi-custodial**: key encrypted with a user password, server cannot read
  it. Losing the password loses the identity, which is the failure mode we are
  removing.
- **Remote signing (NIP-46)**: app never holds the key. No desktop client
  exists yet; largest build.
- **Keep keys visible, polish UX**: does not remove the stall.

## Consequences

- Contradicts upstream's sovereignty vision on purpose. Anyone who wants to
  hold their own key is not the audience of the hosted fork.
- The account service's master key and database are now identity-critical.
  Back them up like `BUZZ_RELAY_PRIVATE_KEY`.
- Moving to semi-custodial or NIP-46 later is possible without relay changes
  as long as the account service stays the sole owner of the key store.
