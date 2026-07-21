current_tag := "v" + `python -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])'`

# List available recipes.
_default:
    @just --list

# Build the project for local development.
build:
    cargo build

# Run the test suite for local development.
test:
    cargo test

# Run cazdo, passing any additional arguments to the binary.
run *ARGS:
    cargo run -- {{ ARGS }}

# Format the codebase in place.
fmt:
    cargo fmt

# Run Clippy across all targets and features.
clippy:
    cargo clippy --all-targets --all-features

# Run the same formatting, lint, build, and test gates as CI.
ci: fmt-check clippy-ci build-ci test-ci

# Check formatting.
fmt-check:
    cargo fmt -- --check

# Run strict Clippy checks.
clippy-ci:
    cargo clippy --locked --all-targets --all-features -- -D warnings

# Build with locked dependencies.
build-ci:
    cargo build --locked

# Test with locked dependencies.
test-ci:
    cargo test --locked

# Preview the changelog for unreleased changes (requires git-cliff).
changelog:
    git-cliff --config cliff.toml --unreleased

# Prepare a release for VERSION without committing it (requires git-cliff).
prepare-release VERSION:
    scripts/prepare-release.sh {{ VERSION }}

# Verify the generated release workflow (requires cargo-dist).
verify-release-yml:
    dist generate
    git diff --exit-code .github/workflows/release.yml

# Verify that unreleased changelog generation produces output (requires git-cliff).
verify-changelog:
    git-cliff --config cliff.toml --unreleased --tag v999.999.999 > /tmp/generated-changelog.md
    test -s /tmp/generated-changelog.md

# Extract release notes for TAG, defaulting to the current package version.
extract-notes TAG=current_tag:
    scripts/extract-release-notes.sh {{ quote(TAG) }} > /tmp/release-notes.md
    test -s /tmp/release-notes.md

# Verify release metadata when the package version changes.
check-release-pr BASE HEAD:
    scripts/check-release-pr.sh {{ quote(BASE) }} {{ quote(HEAD) }}

# Regenerate the README demo assets (requires vhs).
demo:
    scripts/render-demo.sh

# Remove stale temporary VHS demo repositories.
cleanup-demos:
    scripts/cleanup-vhs-demo-repos.sh
