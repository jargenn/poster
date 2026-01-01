run: 
    cargo run | bunyan

check_scripts:
    shellcheck -f diff setup/*

start_db:
    ./setup/setup_db.sh

migrate:
    SKIP_DOCKER=true ./setup/setup_db.sh

check:
 cargo check --all-features

test:
    cargo nextest run --all-features

