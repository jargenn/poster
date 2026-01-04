run: 
    cargo run | bunyan

check_scripts:
    shellcheck -f diff ./crates/poster/setup/*

start_db:
    ./crates/poster/setup/setup_db.sh

migrate:
    SKIP_DOCKER=true ./crates/poster/setup/setup_db.sh

check:
 cargo check --all-features

test:
    cargo nextest run --all-features

