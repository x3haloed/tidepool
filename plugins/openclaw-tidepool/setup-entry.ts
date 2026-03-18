import { defineSetupPluginEntry } from "openclaw/plugin-sdk/core";
import { tidepoolPlugin } from "./src/channel.js";

export default defineSetupPluginEntry(tidepoolPlugin);
