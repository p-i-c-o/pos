# pos-mail: crate-specific instructions

Read and follow the workspace root AGENTS.md. This is a library crate.

## Responsibility

Email client operations over existing IMAP and SMTP accounts.

## Boundaries

Do not implement a mail server now. Guard against duplicate outbound sends and credential leakage.

## Verification

Future tests should cover message parsing, retries and provider failures.
The current crate is a compiling scaffold only; do not imply that its future features exist.
