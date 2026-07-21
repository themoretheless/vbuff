# Maintainer handoff playbook

Reviewed on 2026-07-21. This playbook transfers operational knowledge without storing a secret, personal account, recovery code, or clipboard payload in the repository.

## Access inventory

| Capability | Where it is configured | Required handoff evidence |
|---|---|---|
| GitHub administration and branch protection | Repository organization/settings | Two current maintainers with owner access; protected `main`; required CI checks documented in the repository settings. |
| GitHub Actions provenance | OIDC through `actions/attest-build-provenance`; no long-lived signing key | A test tag produces a verifiable attestation bound to its source revision. |
| Apple signing and notarization | Actions secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | Named primary and backup custodians outside git; certificate expiry and recovery exercise recorded privately. |
| Windows signing | Not configured | This is a release blocker for a trusted Windows installer; choose the provider and document renewal/revocation before claiming support. |
| Linux repositories and package signing | Not configured | Record package namespace ownership, offline recovery, and revocation before publishing packages. |
| Vulnerability disclosure | `SECURITY.md` | At least two maintainers can receive, embargo, and coordinate a report. |

No maintainer should be the sole holder of a release credential. Store credentials in the platform secret store or organization vault, rotate on role changes, and verify recovery at least twice a year.

## Normal release

1. Confirm [limitations.md](limitations.md) and the current [scope review](scope-review.md) match the executable; unresolved critical gates stay explicit.
2. Require green Quality, Supply Chain, Performance Budgets, packaging smoke, and relevant native signing checks on the release commit.
3. Run the release-provenance workflow. It must produce byte-comparison evidence, a three-OS test matrix, residue-canary logs with an honest scope note, dependency policy/audit/vet logs, performance summaries, CycloneDX SBOMs, checksums, and provenance.
4. Verify the evidence manifest and artifact checksums from a clean machine before creating or promoting a tag.
5. Publish release notes that link the public limitation ledger and identify every `Unknown`, degraded backend, schema migration, and rollback path.
6. Keep the previous verified artifact and configuration rollback instructions available until the new release completes its observation window.

## Emergency patch

1. Triage privately when exploitation or clipboard disclosure is plausible. Record affected versions, data classes, platforms, and whether capture, storage, paste, sync, or update trust is involved.
2. Branch from the affected release tag with the smallest reviewable fix. Do not mix feature work into an emergency patch.
3. Add a regression test that contains no real user content. Run the normal supply-chain, matrix, canary, performance, reproducibility, and signing gates; an emergency does not waive evidence.
4. Revoke or rotate affected credentials before publishing when key compromise is possible. Never overwrite an existing release artifact or tag.
5. Publish a new patch version, checksums, provenance, remediation steps, and a clear statement about whether users must wipe history, rotate copied credentials, or reinstall.
6. Backport only to supported lines. Record why any supported line cannot be repaired and move it through the sunset process.

## Dependency cadence

| Cadence | Work |
|---|---|
| Continuous | Dependabot proposals and CI policy checks; RustSec advisories that affect reachable code interrupt normal work. |
| Weekly | Review new advisories and failed scheduled fuzz/supply-chain runs; classify every exception with owner and removal condition. |
| Monthly | Update direct dependencies in small groups, refresh `cargo-vet` evidence, build fuzz targets, and run package smoke tests. |
| Quarterly | Review pinned Actions/tool versions, MSRV/toolchain, duplicate crates, abandoned dependencies, signing expiries, and this handoff inventory. |
| Before release | Re-run audit/deny/vet from the locked graph and include their raw logs in the evidence bundle. |

## Sunset policy

- Announce end of support at least 90 days before the final planned release when security conditions permit.
- Keep source, release artifacts, checksums, SBOMs, provenance, migration/export instructions, and this limitation ledger available for at least 12 months after the final release.
- Ship a final export path that does not require a network service or active entitlement.
- Do not transfer the project name, signing identity, update channel, or package namespace without a public maintainer and key-transition notice.
- If no trusted maintainer remains, disable update publication, revoke unattended credentials, archive write access, and state plainly that no security fixes will follow.

## Handoff drill

Twice a year, a backup maintainer performs a dry run from a clean checkout: validates branch protection, runs the full local acceptance commands, dispatches (but does not publish) release evidence, locates credential recovery instructions outside git, and writes down every missing permission or undocumented step. A failed drill blocks the next release.
