fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

readme:
    #!/usr/bin/env bash
    set -euo pipefail

    # Extract rustdoc comments from lib.rs and convert to markdown
    echo "Generating README.md from rustdoc..."

    # Use cargo doc to generate docs, then extract the main module doc
    cargo doc --no-deps --document-private-items --quiet

    # Extract the rustdoc content and convert it to README format
    sed -n '/^\/\/!/p' src/lib.rs | \
    sed 's/^\/\/! \?//' | \
    sed 's/^\/\/!$//' > README.md

    echo "README.md generated successfully!"

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

checks:
    just fmt-check
    just clippy
    cargo test
