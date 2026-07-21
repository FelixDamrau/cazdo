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
ci:
    cargo fmt -- --check
    cargo clippy --locked --all-targets --all-features -- -D warnings
    cargo build --locked
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

# Regenerate the README demo assets (requires vhs).
demo:
    scripts/render-demo.sh

# Remove stale temporary VHS demo repositories.
cleanup-demos:
    scripts/cleanup-vhs-demo-repos.sh
