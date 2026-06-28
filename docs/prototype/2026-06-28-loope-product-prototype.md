# Loope Product Prototype

## Product Narrative

Loope introduces a small but strict development loop:

```text
Human requirement
  -> Design contract
  -> Claude implements
  -> Codex reviews
  -> Claude revises
  -> Loope verifies artifacts and gates
```

The core bet is that the next useful agentic development pattern is not "one stronger agent", but **role separation plus repeatable gates**.

## MVP Experience

User runs:

```bash
loope plan --design "Build a settings page for API keys"
```

Loope outputs:

```markdown
# Loope Plan

## Requirement
Build a settings page for API keys

## Steps
1. designer via generic: produce design contract
2. implementer via claude: implement against design contract
3. reviewer via codex: review code and design consistency
4. implementer via claude: revise based on review
5. verifier via generic: run tests and confirm gates
```

## Future Frontend Design Integration

The design phase should later support:

- Figma import/export.
- Screenshot review.
- Component inventory.
- Design-token contract.
- UI state matrix.
- Accessibility checks.

The important rule: implementation and review both read the same design contract, so frontend and backend decisions do not drift.

## Open Source Hook

README headline:

> Pair your coding agents: Claude writes, Codex reviews, OpenCode can run the loop.

The first demo should be a GIF:

1. User enters one requirement.
2. Loope prints a loop.
3. Claude receives an implementation prompt.
4. Codex receives a review prompt.
5. The final markdown report shows what changed, what was reviewed, and what still needs verification.
