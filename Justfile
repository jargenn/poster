
default:
    just --list

test:
    cargo nextest run --no-fail-fast

semver:
    cargo semver-checks --baseline-rev "$(git describe --tags --abbrev=0 main)"

