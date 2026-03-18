import { z } from "zod";

const DEFAULT_BASE_URL = "http://127.0.0.1:3001";
const DEFAULT_BATCH_WINDOW = 30;

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

  /** Domains to auto-subscribe on connect (comma-separated IDs or array). */
  seedDomainIds: z.union([z.string(), z.array(z.number())]).optional(),

  /** Batch window for subscriptions in seconds. */
  batchWindowSeconds: z.number().int().positive().max(3600).optional(),

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

export function resolveBatchWindow(config: TidepoolConfig | undefined): number {
  return config?.batchWindowSeconds ?? DEFAULT_BATCH_WINDOW;
}

export function resolveSeedDomainIds(config: TidepoolConfig | undefined): number[] {
  const raw = config?.seedDomainIds;
  if (!raw) return [];
  if (Array.isArray(raw)) return raw;
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
    .map(Number)
    .filter((n) => !Number.isNaN(n));
}
