# Security policy

vbuff handles clipboard data, so reports about unintended capture, plaintext
residue, wrong-target paste, authentication or signature bypass, unsafe export,
or dependency compromise are treated as security-sensitive.

## Supported versions

There is no supported production or LTS release line yet. Security fixes target
the current development line and any release explicitly named as supported in
its GitHub release notes. A tag, branch, or downloaded CI artifact is not a
support promise by itself.

## Report a vulnerability

Use GitHub's private vulnerability reporting for this repository:

<https://github.com/themoretheless/vbuff/security/advisories/new>

Do not open a public issue with exploit details, clipboard samples, database
files, tokens, private keys, screenshots, or source/window identifiers. If the
private form is unavailable, open a content-free public issue requesting a
private security contact and include no technical details beyond the affected
release and platform.

Include only the minimum useful evidence:

- affected commit or release and operating system;
- whether capture, storage, search, paste, export, sync, update, or plugin
  boundaries are involved;
- reproducible steps using synthetic data;
- expected and observed security boundary;
- exploitability, persistence, and user action required;
- a proposed disclosure date, if one is necessary.

## Maintainer response

1. A maintainer acknowledges the private report within three business days.
2. Initial severity and affected-version assessment is targeted within seven
   calendar days. Unknown clipboard disclosure is treated as high severity
   until bounded by evidence.
3. Confirmed issues receive a private advisory, a minimal regression test using
   synthetic content, a patch owner, supported-line decision, and notification
   plan.
4. A CVE is requested for a confirmed vulnerability that affects a released
   version and meets the CNA/GitHub advisory criteria.
5. Critical active exploitation targets a protective action or patch within 72
   hours. Other high-severity fixes target 14 days; lower severities receive a
   documented target in the advisory.
6. Embargo ends after fixed artifacts, checksums/provenance, upgrade or
   mitigation instructions, and affected-version details are ready. The
   reporter receives credit unless anonymity is requested.

These are response targets, not a promise to expose private reporter data or to
publish unsafe proof-of-concept material. Release and emergency-patch evidence
requirements remain in [docs/maintainer-handoff.md](docs/maintainer-handoff.md).
