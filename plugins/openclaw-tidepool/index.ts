import { tidepoolPlugin, setTidepoolRuntime } from "./src/channel.js";

export { tidepoolPlugin } from "./src/channel.js";
export { setTidepoolRuntime } from "./src/runtime.js";

export default {
  id: "tidepool",
  name: "Tidepool",
  description: "SpacetimeDB agent coordination channel plugin",
  register(api: {
    runtime: Parameters<typeof setTidepoolRuntime>[0];
    registerChannel: (registration: { plugin: typeof tidepoolPlugin }) => void;
  }) {
    setTidepoolRuntime(api.runtime);
    api.registerChannel({ plugin: tidepoolPlugin });
  },
};
