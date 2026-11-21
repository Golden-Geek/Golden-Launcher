#!/usr/bin/env bash
set -euo pipefail

target="$1"
messages="$(mktemp)"
trap 'rm -f "$messages"' EXIT

cargo test --locked --release --no-run --manifest-path src-tauri/Cargo.toml \
  --features tauri/custom-protocol --target "$target" --bin golden-launcher \
  --message-format json > "$messages"

executable="$(node -e '
  const fs = require("node:fs");
  const messages = fs.readFileSync(process.argv[1], "utf8").trim().split("\n").map(JSON.parse);
  const test = messages.find(entry =>
    entry.reason === "compiler-artifact" && entry.target.name === "golden-launcher" &&
    entry.profile.test && entry.executable
  );
  if (!test) throw new Error("The Golden Launcher test executable was not built.");
  process.stdout.write(test.executable);
' "$messages")"

cp "$executable" "src-tauri/target/$target/release/publisher-smoke"
