# Pinning and expiry

Use the pin icon or **Pin / Unpin** in a clip's actions. Pinning protects a clip from normal capacity cleanup; it does not cancel an explicit expiry deadline.

For ordinary persistent clips, open **Clip actions → Set expiry** and choose 15 minutes, 1 hour, 1 day, 7 days, 30 days, or **No explicit expiry**. The countdown starts when the command is applied. Removing expiry does not pin the clip: normal retention still applies.

Expiry is stored atomically in both the query column and clip metadata, survives restart and re-copy, and works with SQLite and DuckDB. Re-copying can tighten a deadline but does not silently remove or extend it. The user can explicitly replace the deadline through the menu while the clip is still active.

Expired clips disappear from active views and searches; maintenance removes their database records and unreferenced payloads. Pinning and session protection do not override hard expiry. This is logical retention, not a guarantee of physical erasure from backups or filesystem snapshots.

Sensitive and memory-only clips retain their privacy-policy deadlines; the ordinary TTL editor cannot disable those deadlines. The store API accepts durations from one second to one year; the UI currently provides presets.
