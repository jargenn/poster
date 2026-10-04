
default:
    just --list

test:
    cargo nextest run --no-fail-fast
