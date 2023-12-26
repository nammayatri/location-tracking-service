default:
    @just --list

# Auto-format project tree
fmt:
    treefmt

# Run location-tracking-service
run-lts:
    cargo run --bin location-tracking-service

# Watch for changes, recompile and run location-tracking-service
watch-lts:
    cargo watch -x run --bin location-tracking-service

# Run notification-service
run-ns:
    cargo run --bin notification-service

# Watch for changes, recompile and run notification-service
watch-ns:
    cargo watch -x run --bin notification-service

# Run the project service dependencies
services:
    run-services

# Fix and lint the project
fix-warnings:
    cargo fix --allow-dirty --allow-staged
    cargo clippy --all-targets --all-features -- -D warnings