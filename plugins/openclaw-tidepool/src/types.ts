import {
  DEFAULT_ACCOUNT_ID,
  normalizeAccountId,
} from "openclaw/plugin-sdk/account-id";
import type { OpenClawConfig } from "openclaw/plugin-sdk/nostr";
import fs from "node:fs";
import path from "node:path";
import {
  type TidepoolConfig,
  resolveBaseUrl,
  resolveDatabase,
} from "./config-schema.js";

export interface ResolvedTidepoolAccount {
  accountId: string;
  name?: string;
  enabled: boolean;
  configured: boolean;
  baseUrl: string;
  database: string;
  handle: string;
  tokenPath: string;
  token: string;
  cursorPath: string;
  emitSelfMessages: boolean;
  config: TidepoolConfig;
}

function readToken(tokenPath: string): string {
  try {
    if (!fs.existsSync(tokenPath)) return "";
    return fs.readFileSync(tokenPath, "utf-8").trim();
  } catch {
    return "";
  }
}

function expandUserPath(input: string): string {
  if (!input) {
    return input;
  }
  if (input === "~") {
    return process.env.HOME ?? "/root";
  }
  if (input.startsWith("~/")) {
    return path.join(process.env.HOME ?? "/root", input.slice(2));
  }
  return input;
}

function resolveConfig(
  cfg: OpenClawConfig,
): TidepoolConfig | undefined {
  return (cfg.channels as Record<string, unknown> | undefined)?.tidepool as
    | TidepoolConfig
    | undefined;
}

export function listTidepoolAccountIds(cfg: OpenClawConfig): string[] {
  const account = resolveTidepoolAccount({ cfg, accountId: DEFAULT_ACCOUNT_ID });
  return account.handle ? [DEFAULT_ACCOUNT_ID] : [];
}

export function resolveDefaultTidepoolAccountId(_cfg: OpenClawConfig): string {
  return DEFAULT_ACCOUNT_ID;
}

export function resolveTidepoolAccount(opts: {
  cfg: OpenClawConfig;
  accountId?: string | null;
}): ResolvedTidepoolAccount {
  const accountId = normalizeAccountId(
    opts.accountId ?? resolveDefaultTidepoolAccountId(opts.cfg),
  );
  const tc = resolveConfig(opts.cfg);

  const handle = tc?.handle ?? "";
  const rawTokenPath = tc?.tokenPath ?? "";
  const tokenPath = rawTokenPath
    ? path.resolve(expandUserPath(rawTokenPath))
    : path.join(
        process.env.HOME ?? "/root",
        ".betterclaw",
        "tidepool",
        `${handle}.token`,
      );
  const cursorPath = path.join(
    path.dirname(tokenPath),
    `${handle || "default"}.${resolveDatabase(tc)}.cursors.json`,
  );
  const token = readToken(tokenPath);
  const configured = Boolean(handle && token);

  return {
    accountId,
    name: tc?.name?.trim() || undefined,
    enabled: tc?.enabled !== false,
    configured,
    baseUrl: resolveBaseUrl(tc),
    database: resolveDatabase(tc),
    handle,
    tokenPath,
    token,
    cursorPath,
    emitSelfMessages: tc?.emitSelfMessages ?? false,
    config: tc ?? {},
  };
}
