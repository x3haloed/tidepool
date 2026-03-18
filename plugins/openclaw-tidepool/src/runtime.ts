import type { PluginRuntime } from "openclaw/plugin-sdk/nostr";

let _runtime: PluginRuntime | null = null;

export function setTidepoolRuntime(runtime: PluginRuntime): void {
  _runtime = runtime;
}

export function getTidepoolRuntime(): PluginRuntime {
  if (!_runtime) {
    throw new Error("Tidepool runtime not initialized");
  }
  return _runtime;
}
