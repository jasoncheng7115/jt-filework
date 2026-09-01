# Architecture Decision Records

An ADR records a decision that changes an architectural boundary, together
with the context that forced it and the consequences accepted.

`AGENTS.md` §1: if a change alters a major architectural boundary, write or
update an ADR **first**.

## Rules

- One decision per record. Numbered sequentially, never renumbered.
- Filename: `NNNN-kebab-case-title.md`.
- Status is one of `Proposed`, `Accepted`, `Rejected`, `Superseded by NNNN`,
  `Deprecated`.
- An accepted ADR is not edited to change the decision. A new ADR supersedes
  it and both are kept.
- ADRs are committed like source (`AGENTS.md` §2).

## Index

| ADR | Title | Status |
|---|---|---|
| [0001](0001-gui-stack.md) | GUI technology stack | Proposed — blocked on Phase 0B PoC |
| [0002](0002-repository-and-crate-layout.md) | Repository and crate layout | Accepted |
| [0003](0003-archive-extraction.md) | Archive extraction and creation | Accepted — built |
| [0004](0004-sftp.md) | SFTP support | Accepted — stage one built |
| [0005](0005-iso-images.md) | Browsing and extracting ISO images | Accepted — built |
