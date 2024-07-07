#

## TODO
- [ ] Error handling in the API (properly log it too)
- [ ] Maybe add traces? (SPAN) in the API
- [ ] Validators in the API (not empty string for example)
- [ ] Error hanndling in the repo
- [ ] First Svelte component

## Rust

1. `sudo apt install build-essential`
2. `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

## Firestore emulator

1. `sudo apt install default-jre`
2. [Install gcloud](https://cloud.google.com/sdk/docs/install)
3. `gcloud components update`
4. `gcloud auth application-default login`
5. `make start-firestore`

## Start the API

1. `make start-api`
2. Opens `http://localhost:9000/` to access metrics
3. Server is listening on `http://localhost:3000/`
