run: 
    cargo run | bunyan

check_scripts:
    shellcheck -f diff ./crates/poster/setup/*

start_db:
    cd ./crates/poster/ && ./setup/setup_db.sh

migrate:
    cd ./crates/poster/ && SKIP_DOCKER=true ./setup/setup_db.sh

check:
    cargo check --all-features

test:
    cargo nextest run --all-features

