# WI Preview Length Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Increase the default `cazdo wi` description preview length and add a `--long` flag for a larger bounded preview.

**Architecture:** Keep the existing `compact_text_preview` truncation path and only change how the `wi` command selects the preview length. Parse the new flag in the CLI layer, thread it into command execution, and verify behavior with focused unit tests.

**Tech Stack:** Rust, clap, existing unit tests in `src/cli.rs` and `src/commands.rs`

---

## Chunk 1: CLI flag plumbing

### Task 1: Add `--long` to the `wi` subcommand

**Files:**
- Modify: `src/cli.rs`
- Test: `src/cli.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn parses_wi_with_long_flag() {
    let cli = Cli::parse_from(["cazdo", "wi", "--long"]);

    match cli.command {
        Some(Commands::Wi { id, long }) => {
            assert_eq!(id, None);
            assert!(long);
        }
        _ => panic!("expected wi command with long flag"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test parses_wi_with_long_flag`
Expected: FAIL because the `Wi` variant does not yet contain `long`

- [ ] **Step 3: Write minimal implementation**

Add a `long: bool` field to `Commands::Wi` with clap attribute support and help text.

- [ ] **Step 4: Add an ID + flag parsing test**

```rust
#[test]
fn parses_wi_with_id_and_long_flag() {
    let cli = Cli::parse_from(["cazdo", "wi", "120", "--long"]);

    match cli.command {
        Some(Commands::Wi { id, long }) => {
            assert_eq!(id, Some(120));
            assert!(long);
        }
        _ => panic!("expected wi command with id and long flag"),
    }
}
```

- [ ] **Step 5: Run CLI tests**

Run: `cargo test parses_wi_`
Expected: PASS

## Chunk 2: Preview length selection

### Task 2: Use different bounded preview lengths

**Files:**
- Modify: `src/commands.rs`
- Test: `src/commands.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn compact_text_preview_uses_default_and_long_limits() {
    let html = format!("<p>{}</p>", "abcdefghijklmnopqrstuvwxyz".repeat(40));

    let default_preview = compact_text_preview(&html, 320);
    let long_preview = compact_text_preview(&html, 600);

    assert!(default_preview.len() < long_preview.len());
    assert!(default_preview.ends_with("..."));
    assert!(long_preview.ends_with("..."));
}
```

- [ ] **Step 2: Run test to verify it fails if needed**

Run: `cargo test compact_text_preview_uses_default_and_long_limits`
Expected: PASS or FAIL depending on when added; if it passes immediately, keep it as regression coverage and proceed

- [ ] **Step 3: Write minimal implementation**

Introduce explicit constants for the default and long preview limits and use the `long` flag in the `wi` command handler to choose between them.

- [ ] **Step 4: Run focused command tests**

Run: `cargo test compact_text_preview_`
Expected: PASS

## Chunk 3: Docs and verification

### Task 3: Update command documentation

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update usage examples**

Add an example showing `cazdo wi --long`.

- [ ] **Step 2: Run full test suite**

Run: `cargo test`
Expected: PASS

- [ ] **Step 3: Review diff for scope**

Run: `git diff -- src/cli.rs src/commands.rs README.md docs/superpowers/specs/2026-03-25-wi-preview-length-design.md docs/superpowers/plans/2026-03-25-wi-preview-length.md`
Expected: Only the planned flag, preview-length, docs, and plan/spec changes appear
