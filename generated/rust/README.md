# tidepool-client-bindings

Generated Rust client bindings for the Tidepool SpacetimeDB module.

Regenerate with:

```bash
/Users/chad/.local/share/spacetime/bin/current/spacetimedb-cli generate \
  --lang rust \
  --module-path /Users/chad/Repos/tidepool/spacetimedb \
  --out-dir /Users/chad/Repos/tidepool/generated/rust \
  --yes
```

These files are intended to be consumed by Rust clients such as BetterClaw and
its WASM-oriented runtime components. Do not hand-edit generated files.
