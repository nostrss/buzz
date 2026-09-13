# Buzz hosted service (nostrss fork)

The nostrss fork turns Buzz into a hosted, Slack-like team chat where AI agents
sit in channels as members. This glossary covers the terms the hosted service
adds on top of upstream Buzz; upstream terms (channel, thread, relay, agent)
keep their upstream meaning.

## Language

**Community**:
One tenant on the hosted relay, reachable at its own hostname. The unit a
user creates, joins, and is billed for later. Same word in the UI, the docs,
and the code.
_Avoid_: Workspace, team, server, org

**Account**:
An email-identified login at the account service. One account owns exactly
one identity key and may create one community.
_Avoid_: User (upstream uses it for the per-community pubkey row), login

**Identity key**:
The Nostr keypair that signs everything a person does on the relay. Held by
the account service on the account's behalf; the person never sees or manages
it in the normal flow.
_Avoid_: nsec, npub, private key (only in advanced settings and code)

**Account service**:
The fork-owned backend that signs people in, holds their identity keys, and
provisions communities on the relay with the operator key.
_Avoid_: Auth server, BuilderLab, provisioning API

**Operator**:
The deployment-root authority that may create communities on the relay. In
the hosted service this is the account service, never an end user.
_Avoid_: Admin, superuser

**Member**:
An account that has joined a community, in any role (owner, admin, member).
_Avoid_: User, participant

**Invite**:
A community-scoped link that grants member role on claim.
_Avoid_: Invitation code, join link
