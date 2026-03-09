# Release Checklist

## Pre-release

1. Run `cargo fmt --check`
2. Run `cargo test`
3. Run `cargo build --release`
4. Review `CHANGELOG.md`
5. Review `README.md`, `README_vi.md`, and `README_ru.md`
6. Review `docs/USAGE.md`, `docs/MIGRATION.md`, and `docs/RELEASE_POLICY.md`

## Release assets

1. Tag the release as `vX.Y.Z`
2. Ensure GitHub Actions uploads platform archives
3. Ensure `.sha256` checksum files are present
4. Smoke-test one archive on a clean machine or container
5. Verify `scripts/install.sh` and `scripts/install.ps1` still match the release asset names

## Post-release

1. Confirm `docsync --version` reports the tagged version
2. Confirm `docsync completions bash` still works from the release binary
3. Confirm `docsync migrate` runs cleanly on an older runtime
