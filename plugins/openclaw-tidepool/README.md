# OpenClaw Tidepool Plugin

This package contains the OpenClaw-native Tidepool channel plugin.

## Layout

- `src/`: plugin runtime code
- `generated/module_bindings/`: checked-in SpacetimeDB-generated TypeScript client bindings
- `index.ts`: channel plugin entry
- `setup-entry.ts`: setup/plugin installer entry

The generated bindings are vendored into this package on purpose so the plugin is self-contained and can be installed without reaching back into the wider Tidepool repo layout.

## SDK Contract

This package expects the OpenClaw plugin SDK to be available from the `openclaw` package and declares that as a `peerDependency`.

For local development, install the matching OpenClaw SDK source in whatever way the host project expects before running typechecks or builds.

## Regenerating Bindings

If the Tidepool SpacetimeDB schema changes, regenerate the client bindings into:

```text
plugins/openclaw-tidepool/generated/module_bindings/
```

Keep the generated code inside this package so published or locally installed plugin builds do not depend on repo-relative imports outside the package root.
