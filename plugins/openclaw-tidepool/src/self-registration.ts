import fs from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import path from "node:path";
import { getTidepoolRuntime } from "./runtime.js";
import { resolveTidepoolAccount } from "./types.js";

function sendJson(
  res: ServerResponse,
  status: number,
  body: Record<string, unknown>,
): void {
  res.statusCode = status;
  res.setHeader("content-type", "application/json; charset=utf-8");
  res.end(`${JSON.stringify(body, null, 2)}\n`);
}

export function createTidepoolSelfRegistrationHandler(log?: {
  info?: (message: string) => void;
  warn?: (message: string) => void;
  error?: (message: string) => void;
}) {
  return async (req: IncomingMessage, res: ServerResponse): Promise<boolean> => {
    if (req.method !== "POST") {
      res.setHeader("allow", "POST");
      sendJson(res, 405, {
        error: "method_not_allowed",
        message: "Use POST to complete Tidepool self-registration.",
      });
      return true;
    }

    const runtime = getTidepoolRuntime();
    const cfg = runtime.config.loadConfig();
    const account = resolveTidepoolAccount({ cfg });

    if (!account.handle) {
      sendJson(res, 400, {
        error: "not_configured",
        message: "channels.tidepool.handle is required before self-registration.",
      });
      return true;
    }

    if (account.token) {
      sendJson(res, 409, {
        error: "already_registered",
        message: `A Tidepool token is already saved at ${account.tokenPath}.`,
        handle: account.handle,
        token_path: account.tokenPath,
      });
      return true;
    }

    try {
      fs.mkdirSync(path.dirname(account.tokenPath), { recursive: true });
    } catch (error) {
      sendJson(res, 500, {
        error: "token_dir_create_failed",
        message: `Failed to create Tidepool token directory for ${account.tokenPath}.`,
        details: error instanceof Error ? error.message : String(error),
      });
      return true;
    }

    const url = `${account.baseUrl.replace(/\/+$/, "")}/v1/database/${account.database}/call/create_account`;
    log?.info?.(`[tidepool] self-registration request for ${account.handle} -> ${url}`);

    let response: Response;
    try {
      response = await fetch(url, {
        method: "POST",
        headers: {
          "content-type": "application/json",
        },
        body: JSON.stringify([account.handle]),
      });
    } catch (error) {
      sendJson(res, 502, {
        error: "registration_request_failed",
        message: "Sending the Tidepool registration request failed.",
        details: error instanceof Error ? error.message : String(error),
      });
      return true;
    }

    const token = response.headers
      .get("spacetime-identity-token")
      ?.trim();
    const bodyText = await response.text();

    if (!response.ok) {
      sendJson(res, 502, {
        error: "registration_failed",
        message: `Tidepool registration failed with ${response.status}.`,
        status: response.status,
        body: bodyText,
      });
      return true;
    }

    if (!token) {
      sendJson(res, 502, {
        error: "missing_identity_token",
        message:
          "Tidepool registration succeeded but no spacetime-identity-token header was returned.",
        body: bodyText,
      });
      return true;
    }

    try {
      fs.writeFileSync(account.tokenPath, `${token}\n`, "utf-8");
    } catch (error) {
      sendJson(res, 500, {
        error: "token_write_failed",
        message: `Failed to write Tidepool token to ${account.tokenPath}.`,
        details: error instanceof Error ? error.message : String(error),
      });
      return true;
    }

    sendJson(res, 200, {
      status: "registered",
      handle: account.handle,
      token_path: account.tokenPath,
      base_url: account.baseUrl,
      database: account.database,
      channel_restart_required: true,
    });
    return true;
  };
}
