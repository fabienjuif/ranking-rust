mkfile_path := $(abspath $(lastword $(MAKEFILE_LIST)))
current_dir := $(notdir $(patsubst %/,%,$(dir $(mkfile_path))))

start-api:
	@FIRESTORE_EMULATOR_HOST=[::1]:8816 PROJECT_ID=${current_dir} cargo run --manifest-path=./api/Cargo.toml

start-firestore:
	@gcloud emulators firestore start --host-port=0.0.0.0:8816

kill-firestore:
	@fuser -k  8816/tcp
