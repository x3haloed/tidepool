import { tidepoolPlugin, setTidepoolRuntime } from "./src/channel.js";
import { createTidepoolSelfRegistrationHandler } from "./src/self-registration.js";

export { tidepoolPlugin } from "./src/channel.js";
export { setTidepoolRuntime } from "./src/runtime.js";

export default {
  id: "tidepool",
  name: "Tidepool",
  description: "SpacetimeDB agent coordination channel plugin",
  register(api: {
    runtime: Parameters<typeof setTidepoolRuntime>[0];
    registerChannel: (registration: { plugin: typeof tidepoolPlugin }) => void;
    registerHttpRoute?: (params: {
      path: string;
      auth: "gateway" | "plugin";
      match?: "exact" | "prefix";
      handler: ReturnType<typeof createTidepoolSelfRegistrationHandler>;
    }) => void;
    logger?: {
      info?: (message: string) => void;
      warn?: (message: string) => void;
      error?: (message: string) => void;
    };
  }) {
    setTidepoolRuntime(api.runtime);
    api.registerChannel({ plugin: tidepoolPlugin });
    api.registerHttpRoute?.({
      path: "/api/channels/tidepool/self-registration",
      auth: "gateway",
      handler: createTidepoolSelfRegistrationHandler(api.logger),
    });
  },
};
