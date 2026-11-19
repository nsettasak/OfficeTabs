# Security

Only the latest preview is considered for fixes; there is no security support SLA.
This add-in executes in Office processes, so memory-safety errors can affect Office.

Use the repository's **Security > Report a vulnerability** option when private
vulnerability reporting has been enabled. If it is unavailable, open a minimal
issue asking maintainers to enable a private channel; do not post exploit details,
documents, credentials or unsanitized logs publicly. No private mailbox is currently
designated. Maintainers should enable private reporting before accepting reports.

Include affected version, Office/Windows bitness, reproduction with disposable
files, expected impact and any proposed fix. Redact usernames, machine paths and
document contents. Local debug logs are not automatically uploaded.

Builds are currently unsigned. Verify release origin; never disable Windows
security protections on the basis of instructions in an issue or pull request.
