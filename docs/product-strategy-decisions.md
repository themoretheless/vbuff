# Product strategy decisions for items 128-140

Reviewed on 2026-07-18. These backlog entries are mutually exclusive pricing, licensing, and governance hypotheses. They are resolved here as explicit policy decisions; vbuff does not contain placeholder billing code, artificial feature locks, or legal promises that no incorporated operator or escrow agent can currently honor.

## Non-negotiable boundary

- The repository remains `MIT OR Apache-2.0`; no time-delayed source license is introduced.
- Local history, local search, export, security fixes, signed-update verification, and self-hosting interfaces must not depend on a subscription.
- A future hosted relay may charge for operating a service, but the protocol and self-hosted path must remain usable without that service.
- Pricing must not require inspecting plaintext, clip types, clip counts, or a user's local history.
- Signing and notarization are security controls, not a reason to ship an intentionally unverifiable free binary.

## Decision record

| Item | Decision | Reason and implementation consequence |
|---:|---|---|
| 128 | Rejected | BSL conflicts with the existing permissive open-core promise. Keep all current crates under `MIT OR Apache-2.0`. |
| 129 | Rejected for now | A dead-man's-switch is a legal/escrow obligation, not a repository feature. Do not claim it until a legal entity, escrow agent, trigger, and key-custody plan exist. |
| 130 | Adapted | Local releases must continue working without a recurring entitlement check. Exact prices and major-version upgrade terms remain outside the software until there is a seller. |
| 131 | Accepted as a boundary | If commercial plans exist, recurring fees may fund only an operated hosted service. No local feature is wired to billing in this batch. |
| 132 | Accepted as a boundary | A future self-hosted relay must not require a hosted-relay subscription. The relay itself is not yet implemented. |
| 133 | Rejected | Ciphertext-byte metering still exposes activity and volume metadata and adds a surveillance-shaped incentive. Prefer fixed service tiers with coarse operational limits. |
| 134 | Rejected for now | Founding scarcity and permanent price promises would be marketing claims without an established product or billing entity. Maintain a public changelog if pricing later exists. |
| 135 | Rejected for now | Per-library billing requires server-visible collaboration metadata before a collaboration product exists. Team policy must follow, not drive, the encrypted protocol. |
| 136 | Rejected | Official signed/notarized artifacts are part of the security chain. Verification, checksums, and source builds remain available without a paid trust gate. |
| 137 | Adapted | A public security bounty is desirable after funding and disclosure operations exist; no percentage-of-revenue promise is made today. |
| 138 | Adapted | Public feature sponsorship can be considered with maintainer acceptance, security review, and no roadmap purchase guarantee. It does not create closed features. |
| 139 | Deferred | Managed compliance policy needs signed policy bundles, admin/user threat modeling, and enforceable encrypted storage first. Shareable redacted config is the current small foundation. |
| 140 | Accepted as a boundary | Local history remains uncapped by commercial entitlement. Device limits for a future hosted relay are deliberately undecided until transport cost and privacy evidence exist. |

## Review trigger

Revisit this record only when a hosted relay, legal operator, or billing system is proposed in a concrete pull request. That review must include the threat model, metadata visible to the service, self-hosting parity, entitlement failure behavior, and migration away from the service.
