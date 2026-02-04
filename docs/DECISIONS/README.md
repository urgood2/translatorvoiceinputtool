# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records that document significant technical decisions made during the development of OpenVoicy.

## What is an ADR?

An Architecture Decision Record captures an important design decision along with its context and consequences. ADRs help:
- Document **why** decisions were made, not just what was decided
- Enable informed future changes by understanding original constraints
- Onboard new contributors by explaining the project's evolution

## Decision Index

| ID | Title | Status | Date |
|----|-------|--------|------|
| [0001](./0001-asr-backend.md) | ASR Backend Selection | Accepted | 2026-02-04 |

## ADR Statuses

- **Proposed**: Under discussion
- **Accepted**: Decision has been made and is active
- **Deprecated**: No longer applies (superseded by newer ADR)
- **Superseded**: Replaced by a newer decision (link to replacement)

## Creating New ADRs

When adding a new decision record:

1. Use the next sequential number: `NNNN-short-title.md`
2. Follow the standard template:
   - Status
   - Context
   - Options Considered
   - Decision
   - Consequences
   - Related Documents
3. Update this README with the new entry
4. Get team review before merging

## Template

```markdown
# ADR NNNN: Title

**Status:** Proposed | Accepted | Deprecated | Superseded by [NNNN]
**Date:** YYYY-MM-DD
**Deciders:** [Names or roles]

## Context

What is the issue that we're seeing that is motivating this decision?

## Considered Options

### Option 1: Name
- Pros
- Cons

### Option 2: Name
- Pros
- Cons

## Decision

What is the change we're proposing and why?

## Consequences

### Positive
- Benefits

### Negative
- Drawbacks and mitigations

## Related Documents

- Links to related docs
```
