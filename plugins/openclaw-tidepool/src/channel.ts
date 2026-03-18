import {
  buildChannelConfigSchema,
  collectStatusIssuesFromLastError,
  createDefaultChannelRuntimeState,
  DEFAULT_ACCOUNT_ID,
  type ChannelPlugin,
} from "openclaw/plugin-sdk/nostr";
import fs from "node:fs";
import path from "node:path";
import { TidepoolConfigSchema } from "./config-schema.js";
import {
  resolveTidepoolAccount,
  listTidepoolAccountIds,
  resolveDefaultTidepoolAccountId,
  type ResolvedTidepoolAccount,
} from "./types.js";
import { getTidepoolRuntime } from "./runtime.js";

// Re-export for the entry point
export { setTidepoolRuntime } from "./runtime.js";

// ── SpacetimeDB generated bindings ──────────────────────────────────
import type {
  DbConnection,
  SubscriptionHandle,
  SubscriptionEventContext,
  EventContext,
} from "../generated/module_bindings/index.js";

// ── Active connections per account ──────────────────────────────────
interface TidepoolConnection {
  accountId: string;
  conn: DbConnection;
  subscription: SubscriptionHandle;
  cursorPath: string;
  cursors: Map<string, number>; // domain_id (string) -> domain_sequence
}

let activeConnection: TidepoolConnection | null = null;

// ── Helpers ─────────────────────────────────────────────────────────

function threadKey(domainId: number): string {
  return `tidepool:domain:${domainId}`;
}

function domainIdFromThread(threadId: string): number | undefined {
  const match = threadId.match(/^tidepool:domain:(\d+)$/);
  return match ? Number(match[1]) : undefined;
}

function loadPersistedCursors(cursorPath: string): Map<string, number> {
  try {
    if (!fs.existsSync(cursorPath)) {
      return new Map();
    }
    const parsed = JSON.parse(fs.readFileSync(cursorPath, "utf-8")) as Record<
      string,
      number
    >;
    return new Map(
      Object.entries(parsed).filter(
        ([, value]) => typeof value === "number" && Number.isFinite(value),
      ),
    );
  } catch {
    return new Map();
  }
}

function persistCursors(cursorPath: string, cursors: Map<string, number>): void {
  try {
    fs.mkdirSync(path.dirname(cursorPath), { recursive: true });
  } catch {}
  try {
    fs.writeFileSync(
      cursorPath,
      `${JSON.stringify(Object.fromEntries(cursors), null, 2)}\n`,
      "utf-8",
    );
  } catch {
    // best-effort only
  }
}

function getActiveConnection(): TidepoolConnection | null {
  return activeConnection;
}

// ── Plugin ──────────────────────────────────────────────────────────

export const tidepoolPlugin: ChannelPlugin<ResolvedTidepoolAccount> = {
  id: "tidepool",
  meta: {
    id: "tidepool",
    label: "Tidepool",
    selectionLabel: "Tidepool (agent coordination)",
    docsPath: "/channels/tidepool",
    docsLabel: "tidepool",
    blurb: "SpacetimeDB-based agent coordination substrate.",
    order: 90,
  },
  capabilities: {
    chatTypes: ["group"], // domains are group-like
    media: false,
  },
  reload: { configPrefixes: ["channels.tidepool"] },
  configSchema: buildChannelConfigSchema(TidepoolConfigSchema),

  config: {
    listAccountIds: (cfg) => listTidepoolAccountIds(cfg),
    resolveAccount: (cfg, accountId) =>
      resolveTidepoolAccount({ cfg, accountId }),
    defaultAccountId: (cfg) => resolveDefaultTidepoolAccountId(cfg),
    isConfigured: (account) => account.configured,
    describeAccount: (account) => ({
      accountId: account.accountId,
      name: account.name ?? account.handle,
      enabled: account.enabled,
      configured: account.configured,
    }),
  },

  outbound: {
    deliveryMode: "direct",
    textChunkLimit: 1024, // Tidepool message_char_limit is typically 280-1024
    sendText: async ({ to, text, replyToMessageId }) => {
      const conn = getActiveConnection();
      if (!conn) {
        throw new Error("Tidepool connection not active");
      }

      const domainId = domainIdFromThread(to);
      if (domainId === undefined) {
        throw new Error(`Invalid Tidepool target: ${to}`);
      }

      conn.conn.reducers.postMessage(domainId, text, replyToMessageId ?? null);
      return {
        channel: "tidepool" as const,
        to,
        messageId: `tidepool-${domainId}-${Date.now()}`,
      };
    },
  },

  status: {
    defaultRuntime: createDefaultChannelRuntimeState(DEFAULT_ACCOUNT_ID),
    collectStatusIssues: (accounts) =>
      collectStatusIssuesFromLastError("tidepool", accounts),
    buildChannelSummary: ({ snapshot }) => ({
      configured: snapshot.configured ?? false,
      handle: snapshot.handle ?? null,
      running: snapshot.running ?? false,
      lastStartAt: snapshot.lastStartAt ?? null,
      lastStopAt: snapshot.lastStopAt ?? null,
      lastError: snapshot.lastError ?? null,
    }),
    buildAccountSnapshot: ({ account, runtime }) => ({
      accountId: account.accountId,
      name: account.name ?? account.handle,
      enabled: account.enabled,
      configured: account.configured,
      handle: account.handle,
      running: runtime?.running ?? false,
      lastStartAt: runtime?.lastStartAt ?? null,
      lastStopAt: runtime?.lastStopAt ?? null,
      lastError: runtime?.lastError ?? null,
      lastInboundAt: runtime?.lastInboundAt ?? null,
      lastOutboundAt: runtime?.lastOutboundAt ?? null,
    }),
  },

  gateway: {
    startAccount: async (ctx) => {
      const account = ctx.account;
      ctx.setStatus({
        accountId: account.accountId,
        handle: account.handle,
      });

      if (!account.configured) {
        throw new Error(
          `Tidepool not configured: need handle and token file`,
        );
      }

      const runtime = getTidepoolRuntime();
      const { DbConnection } = await import(
        "../generated/module_bindings/index.js"
      );

      ctx.log?.info(
        `[${account.accountId}] connecting to Tidepool at ${account.baseUrl}/${account.database} as ${account.handle}`,
      );

      if (activeConnection && activeConnection.accountId !== account.accountId) {
        throw new Error(
          `Tidepool already has an active account (${activeConnection.accountId}); only one configured account is supported`,
        );
      }

      const cursors = loadPersistedCursors(account.cursorPath);

      // Connect to SpacetimeDB
      const conn = await new Promise<DbConnection>((resolve, reject) => {
        const timeout = setTimeout(
          () => reject(new Error("Tidepool connection timeout (15s)")),
          15_000,
        );

        const connection = DbConnection.builder()
          .withUri(account.baseUrl)
          .withModuleName(account.database)
          .withToken(account.token)
          .onConnect((_conn, _identity, _token) => {
            clearTimeout(timeout);
            resolve(connection);
          })
          .onDisconnect((_ctx, reason) => {
            ctx.log?.warn(
              `[${account.accountId}] Tidepool disconnected: ${reason?.message ?? "unknown"}`,
            );
            if (activeConnection?.accountId === account.accountId) {
              activeConnection = null;
            }
          })
          .onConnectError((_ctx, error) => {
            clearTimeout(timeout);
            reject(error);
          })
          .build();
      });

      // Subscribe to the same tables BetterClaw uses
      const subscription = conn
        .subscriptionBuilder()
        .onApplied((subCtx: SubscriptionEventContext) => {
          ctx.log?.debug?.(
            `[${account.accountId}] Tidepool subscription applied`,
          );

          // Seed cursors from current message state so we don't replay history
          for (const msg of subCtx.db.mySubscribedMessages().iter()) {
            const key = String(msg.domainId);
            const existing = cursors.get(key) ?? 0;
            if (msg.domainSequence > existing) {
              cursors.set(key, msg.domainSequence);
            }
          }
          persistCursors(account.cursorPath, cursors);
          ctx.log?.debug?.(
            `[${account.accountId}] seeded ${cursors.size} domain cursors`,
          );
        })
        .subscribe([
          "SELECT * FROM my_account",
          "SELECT * FROM my_subscriptions",
          "SELECT * FROM my_subscribed_messages",
          "SELECT * FROM my_dm_domains",
          "SELECT * FROM domain_member",
        ]);

      // Handle new messages
      conn.db.mySubscribedMessages().onInsert((_ctx: EventContext, row) => {
        const domainKey = String(row.domainId);
        const currentCursor = cursors.get(domainKey) ?? 0;

        // Cursor-based dedup (same as BetterClaw)
        if (row.domainSequence <= currentCursor) {
          return;
        }
        cursors.set(domainKey, row.domainSequence);
        persistCursors(account.cursorPath, cursors);

        // Skip own messages unless configured otherwise
        const myAccount = Array.from(conn.db.myAccount().iter())[0];
        if (
          !account.emitSelfMessages &&
          myAccount &&
          row.authorAccountId === myAccount.accountId
        ) {
          return;
        }

        // Build metadata
        const subscription = Array.from(conn.db.mySubscriptions().iter()).find(
          (s) => s.domainId === row.domainId,
        );

        const domainTitle = subscription?.title ?? `Domain ${row.domainId}`;
        const domainSlug = subscription?.slug ?? "";
        const threadId = threadKey(row.domainId);
        const createdAtMicros = Number(row.createdAt.microsSinceUnixEpoch);

        ctx.log?.info(
          `[${account.accountId}] message from domain ${row.domainId} seq ${row.domainSequence}`,
        );

        const inbound = {
          channel: "tidepool",
          accountId: account.accountId,
          senderId: String(row.authorAccountId),
          chatType: "group",
          chatId: threadId,
          text: row.body,
          replyToMessageId: row.replyToMessageId
            ? String(row.replyToMessageId)
            : undefined,
          meta: {
            domain_id: row.domainId,
            domain_title: domainTitle,
            domain_slug: domainSlug,
            message_id: row.messageId,
            domain_sequence: row.domainSequence,
            author_account_id: row.authorAccountId,
            reply_to_message_id: row.replyToMessageId ?? null,
            created_at_micros: createdAtMicros,
            tidepool_target: threadId,
          },
          reply: async (responseText: string) => {
            conn.reducers.postMessage(
              row.domainId,
              responseText,
              row.messageId,
            );
          },
        };
        const handler = (
          runtime.channel.reply as {
            handleInboundMessage?: (params: unknown) => Promise<void>;
          }
        ).handleInboundMessage;
        void handler?.(inbound).catch(async (error) => {
          const errorText =
            error instanceof Error ? error.message : String(error);
          ctx.log?.error?.(
            `[${account.accountId}] Tidepool inbound handling failed for message ${row.messageId}: ${errorText}`,
          );
          try {
            await handler?.({
              channel: "tidepool",
              accountId: account.accountId,
              senderId: "system:tidepool",
              chatType: "group",
              chatId: threadId,
              text: `System note: handling Tidepool message ${row.messageId} in domain ${row.domainId} failed before the model completed the turn. Error: ${errorText}. Please account for that failure in your ongoing coordination state.`,
              meta: {
                domain_id: row.domainId,
                domain_title: domainTitle,
                domain_slug: domainSlug,
                message_id: row.messageId,
                domain_sequence: row.domainSequence,
                author_account_id: row.authorAccountId,
                reply_to_message_id: row.replyToMessageId ?? null,
                created_at_micros: createdAtMicros,
                tidepool_target: threadId,
                tidepool_runtime_error: true,
                tidepool_runtime_error_message: errorText,
                tidepool_runtime_error_source_message_id: row.messageId,
              },
            });
          } catch (notifyError) {
            const notifyText =
              notifyError instanceof Error
                ? notifyError.message
                : String(notifyError);
            ctx.log?.error?.(
              `[${account.accountId}] Tidepool runtime error notification also failed for message ${row.messageId}: ${notifyText}`,
            );
          }
        });
      });

      // Store connection
      activeConnection = {
        accountId: account.accountId,
        conn,
        subscription,
        cursorPath: account.cursorPath,
        cursors,
      };

      ctx.log?.info(
        `[${account.accountId}] Tidepool connected and subscribed`,
      );

      // Return cleanup
      return {
        stop: () => {
          const active = activeConnection;
          if (active && active.accountId === account.accountId) {
            active.subscription.unsubscribe();
            active.conn.disconnect();
            activeConnection = null;
          }
          ctx.log?.info(`[${account.accountId}] Tidepool disconnected`);
        },
      };
    },
  },
};
