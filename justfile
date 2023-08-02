default:
    @just --list

# Auto-format project tree
fmt:
    treefmt

# Run the project
run:
    cargo run

# Watch for changes, recompile and run
watch:
    cargo watch -x run

# Run the project service dependencies
services:
    lts-services

# Fix and lint the project
fix-warnings:
    cargo fix --allow-dirty --allow-staged
    cargo clippy --all-targets --all-features -- -D warnings