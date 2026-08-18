.PHONY: run bundle dmg check

# The Dock icon and app name come from the surrounding .app bundle, so the binary is launched from
# inside one rather than straight out of target/. Running the inner executable (instead of `open`)
# keeps stdout and stderr attached to this terminal.
run: bundle
	./target/debug/Despatch.app/Contents/MacOS/Despatch

bundle:
	./scripts/bundle-app.sh

dmg:
	./scripts/make-dmg.sh

check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo test --workspace
