import { z } from "zod";

const DEFAULT_BASE_URL = "http://127.0.0.1:3001";

/**
 * Zod schema for channels.tidepool.* configuration.
 */
export const TidepoolConfigSchema = z.object({
  /** Display name for this account. */
  name: z.string().optional(),

  /** Whether this channel is enabled. */
  enabled: z.boolean().optional(),

  /** SpacetimeDB base URL. */
  baseUrl: z.string().url().optional(),

  /** SpacetimeDB database name. */
  database: z.string().optional(),

  /** Tidepool handle (account name). */
  handle: z.string().optional(),

  /** Path to the Tidepool identity token file. */
  tokenPath: z.string().optional(),

  /** If true, also deliver messages authored by this account (for debugging). */
  emitSelfMessages: z.boolean().optional(),
});

export type TidepoolConfig = z.infer<typeof TidepoolConfigSchema>;

export function resolveBaseUrl(config: TidepoolConfig | undefined): string {
  return config?.baseUrl ?? DEFAULT_BASE_URL;
}

export function resolveDatabase(config: TidepoolConfig | undefined): string {
  return config?.database ?? "tidepool-dev";
}
