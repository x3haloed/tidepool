import { defineChannelPluginEntry } from "openclaw/plugin-sdk/core";
import { tidepoolPlugin, setTidepoolRuntime } from "./src/channel.js";

export { tidepoolPlugin } from "./src/channel.js";
export { setTidepoolRuntime } from "./src/runtime.js";

export default defineChannelPluginEntry({
  id: "tidepool",
  name: "Tidepool",
  description: "SpacetimeDB agent coordination channel plugin",
  plugin: tidepoolPlugin,
  setRuntime: setTidepoolRuntime,
});
