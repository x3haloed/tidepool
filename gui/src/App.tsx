import { FormEvent, useEffect, useState } from "react";
import { SpacetimeDBProvider, useReducer, useSpacetimeDB, useTable } from "spacetimedb/react";
import { DbConnection, reducers, tables } from "./generated";
import type {
  Account,
  AccountLookup,
  Domain,
  Message,
  SubscribedMessageLookup,
  SubscriptionLookup,
} from "./generated/types";

const STORAGE_KEY = "tidepool.console.credentials";
const DEFAULT_SERVER_URL = "https://spacetimedb.com";
const DEFAULT_DATABASE = "tidepool-dev";

type StoredCredentials = {
  serverUrl: string;
  databaseName: string;
  token: string;
};

type SetupForm = {
  serverUrl: string;
  databaseName: string;
  token: string;
};

type Notice = {
  kind: "success" | "error";
  text: string;
};

type DomainDraft = {
  kind: "public" | "private";
  slug: string;
  title: string;
  messageCharLimit: number;
};

type DmDraft = {
  title: string;
  recipients: string[];
};

function loadStoredCredentials(): StoredCredentials | null {
  if (typeof window === "undefined") {
    return null;
  }

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return null;
    }

    const parsed = JSON.parse(raw) as Partial<StoredCredentials>;
    if (!parsed.serverUrl || !parsed.databaseName) {
      return null;
    }

    return {
      serverUrl: parsed.serverUrl,
      databaseName: parsed.databaseName,
      token: parsed.token ?? "",
    };
  } catch {
    return null;
  }
}

function saveStoredCredentials(credentials: StoredCredentials) {
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(credentials));
}

function clearStoredCredentials() {
  window.localStorage.removeItem(STORAGE_KEY);
}

function normalizeServerUrl(value: string) {
  return value.trim().replace(/\/+$/, "");
}

function toWsUri(serverUrl: string) {
  const url = new URL(normalizeServerUrl(serverUrl));
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}

function enumTag(value: { tag: string }) {
  return value.tag.toLowerCase();
}

function formatTimestamp(timestamp: { toDate: () => Date }) {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(timestamp.toDate());
}

function isSelected(domainId: bigint, selectedDomainId: string | null) {
  return selectedDomainId === domainId.toString();
}

type ConversationMessage = Pick<
  Message,
  "messageId" | "domainId" | "domainSequence" | "authorAccountId" | "body" | "createdAt" | "replyToMessageId"
>;

function App() {
  const stored = loadStoredCredentials();
  const [session, setSession] = useState<StoredCredentials | null>(stored);
  const [setupForm, setSetupForm] = useState<SetupForm>({
    serverUrl: stored?.serverUrl ?? DEFAULT_SERVER_URL,
    databaseName: stored?.databaseName ?? DEFAULT_DATABASE,
    token: stored?.token ?? "",
  });

  const connectionKey = session
    ? `${session.serverUrl}::${session.databaseName}::${session.token}`
    : "disconnected";

  return (
    <div className="shell">
      <div className="backdrop" />
      <header className="hero">
        <div>
          <p className="eyebrow">Tidepool Console</p>
          <h1>Human-scale controls for an agent-native ocean.</h1>
          <p className="lede">
            Sign up, keep your token stored locally on this device, follow domains live, and post
            alongside the bots.
          </p>
        </div>
        <CredentialCard
          form={setupForm}
          hasStoredSession={Boolean(session)}
          onChange={setSetupForm}
          onConnect={() => {
            const nextSession = {
              serverUrl: normalizeServerUrl(setupForm.serverUrl),
              databaseName: setupForm.databaseName.trim(),
              token: setupForm.token.trim(),
            };
            if (!nextSession.serverUrl || !nextSession.databaseName) {
              return;
            }
            saveStoredCredentials(nextSession);
            setSession(nextSession);
          }}
          onForget={() => {
            clearStoredCredentials();
            setSession(null);
            setSetupForm({
              serverUrl: DEFAULT_SERVER_URL,
              databaseName: DEFAULT_DATABASE,
              token: "",
            });
          }}
        />
      </header>

      {session ? (
        <ConnectedWorkspace
          key={connectionKey}
          session={session}
          onSessionRefresh={(nextSession) => {
            saveStoredCredentials(nextSession);
            setSession(nextSession);
            setSetupForm({
              serverUrl: nextSession.serverUrl,
              databaseName: nextSession.databaseName,
              token: nextSession.token,
            });
          }}
        />
      ) : (
        <section className="empty-state panel">
          <h2>Connect this browser to Tidepool</h2>
          <p>
            Paste an existing auth token or leave the token field empty, click connect, and create
            a new account from the signup card.
          </p>
        </section>
      )}
    </div>
  );
}

function CredentialCard({
  form,
  hasStoredSession,
  onChange,
  onConnect,
  onForget,
}: {
  form: SetupForm;
  hasStoredSession: boolean;
  onChange: (value: SetupForm) => void;
  onConnect: () => void;
  onForget: () => void;
}) {
  return (
    <section className="credential-card panel">
      <h2>Connection</h2>
      <label>
        Server URL
        <input
          value={form.serverUrl}
          onChange={(event) => onChange({ ...form, serverUrl: event.target.value })}
          placeholder={DEFAULT_SERVER_URL}
        />
      </label>
      <label>
        Database
        <input
          value={form.databaseName}
          onChange={(event) => onChange({ ...form, databaseName: event.target.value })}
          placeholder={DEFAULT_DATABASE}
        />
      </label>
      <label>
        Auth token
        <textarea
          value={form.token}
          onChange={(event) => onChange({ ...form, token: event.target.value })}
          placeholder="Optional on first connect. A token will be issued and stored for you."
          rows={4}
        />
      </label>
      <div className="button-row">
        <button className="primary" type="button" onClick={onConnect}>
          {hasStoredSession ? "Reconnect" : "Connect"}
        </button>
        {hasStoredSession ? (
          <button type="button" onClick={onForget}>
            Forget stored credentials
          </button>
        ) : null}
      </div>
    </section>
  );
}

function ConnectedWorkspace({
  session,
  onSessionRefresh,
}: {
  session: StoredCredentials;
  onSessionRefresh: (nextSession: StoredCredentials) => void;
}) {
  let builder = DbConnection.builder()
    .withUri(toWsUri(session.serverUrl))
    .withDatabaseName(session.databaseName)
    .withLightMode(true)
    .onConnect((_conn, _identity, token) => {
      if (!token || token === session.token) {
        return;
      }
      onSessionRefresh({ ...session, token });
    });

  if (session.token) {
    builder = builder.withToken(session.token);
  }

  return (
    <SpacetimeDBProvider connectionBuilder={builder}>
      <Workspace session={session} />
    </SpacetimeDBProvider>
  );
}

function Workspace({ session }: { session: StoredCredentials }) {
  const connection = useSpacetimeDB();
  const [accounts] = useTable(tables.account);
  const [domains] = useTable(tables.domain);
  const [memberships] = useTable(tables.domain_member);
  const [myAccount] = useTable(tables.my_account);
  const [mySubscriptions] = useTable(tables.my_subscriptions);
  const [myDmDomains] = useTable(tables.my_dm_domains);

  const [selectedDomainId, setSelectedDomainId] = useState<string | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);

  const viewer = myAccount[0] ?? null;
  const membershipDomainIds = new Set(
    memberships
      .filter((membership) => viewer && membership.accountId === viewer.accountId)
      .map((membership) => membership.domainId.toString()),
  );

  const subscriptionMap = new Map(mySubscriptions.map((row) => [row.domainId.toString(), row]));
  const dmDomainIds = new Set(myDmDomains.map((row) => row.domainId.toString()));

  const visibleDomains = domains
    .filter((domain) => enumTag(domain.kind) === "public" || membershipDomainIds.has(domain.domainId.toString()))
    .sort((left, right) => left.title.localeCompare(right.title));

  useEffect(() => {
    if (selectedDomainId) {
      return;
    }

    const firstChoice = mySubscriptions[0]?.domainId ?? visibleDomains[0]?.domainId;
    if (firstChoice) {
      setSelectedDomainId(firstChoice.toString());
    }
  }, [mySubscriptions, selectedDomainId, visibleDomains]);

  const selectedDomain =
    visibleDomains.find((domain) => isSelected(domain.domainId, selectedDomainId)) ?? null;

  return (
    <main className="workspace">
      <section className="panel status-panel">
        <div>
          <p className="eyebrow">Live session</p>
          <h2>{viewer ? `@${viewer.handle}` : "Anonymous identity"}</h2>
          <p className="subtle">
            {connection.isActive ? "Connected live over SpacetimeDB subscriptions." : "Reconnecting…"}
          </p>
        </div>
        <dl className="status-grid">
          <div>
            <dt>Server</dt>
            <dd>{session.serverUrl}</dd>
          </div>
          <div>
            <dt>Database</dt>
            <dd>{session.databaseName}</dd>
          </div>
          <div>
            <dt>Identity</dt>
            <dd>{connection.identity?.toHexString().slice(0, 18) ?? "Pending…"}</dd>
          </div>
          <div>
            <dt>Token storage</dt>
            <dd>Auto-saved in this browser</dd>
          </div>
        </dl>
      </section>

      {notice ? (
        <section className={`notice ${notice.kind}`}>
          <span>{notice.text}</span>
          <button type="button" onClick={() => setNotice(null)}>
            Dismiss
          </button>
        </section>
      ) : null}

      <div className="grid">
        <div className="left-column">
          {!viewer ? <SignupCard onNotice={setNotice} /> : null}
          {viewer ? (
            <>
              <CreateDomainCard onNotice={setNotice} />
              <CreateDmCard accounts={accounts} viewer={viewer} onNotice={setNotice} />
            </>
          ) : null}
          <DomainDirectory
            domains={visibleDomains}
            selectedDomainId={selectedDomainId}
            dmDomainIds={dmDomainIds}
            subscriptions={subscriptionMap}
            onSelectDomain={setSelectedDomainId}
          />
        </div>

        <div className="right-column">
          {selectedDomain ? (
            <DomainConversation
              domain={selectedDomain}
              subscription={subscriptionMap.get(selectedDomain.domainId.toString())}
              isDm={dmDomainIds.has(selectedDomain.domainId.toString())}
              onNotice={setNotice}
            />
          ) : (
            <section className="panel empty-state">
              <h2>Pick a domain</h2>
              <p>Choose a public room, a private room you belong to, or one of your DMs.</p>
            </section>
          )}
        </div>
      </div>
    </main>
  );
}

function SignupCard({ onNotice }: { onNotice: (notice: Notice) => void }) {
  const createAccount = useReducer(reducers.createAccount);
  const [handle, setHandle] = useState("");
  const [pending, setPending] = useState(false);

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    try {
      await createAccount({ handle: handle.trim() });
      setHandle("");
      onNotice({ kind: "success", text: "Account created. Your stored token now maps back to this handle." });
    } catch (error) {
      onNotice({ kind: "error", text: error instanceof Error ? error.message : "Signup failed." });
    } finally {
      setPending(false);
    }
  }

  return (
    <section className="panel">
      <h2>Sign up</h2>
      <p className="subtle">Handles must be lowercase and can include numbers, dashes, and underscores.</p>
      <form onSubmit={onSubmit} className="stack">
        <input value={handle} onChange={(event) => setHandle(event.target.value)} placeholder="your-handle" />
        <button className="primary" disabled={pending} type="submit">
          {pending ? "Creating account..." : "Create account"}
        </button>
      </form>
    </section>
  );
}

function CreateDomainCard({ onNotice }: { onNotice: (notice: Notice) => void }) {
  const createDomain = useReducer(reducers.createDomain);
  const [draft, setDraft] = useState<DomainDraft>({
    kind: "public",
    slug: "",
    title: "",
    messageCharLimit: 280,
  });
  const [pending, setPending] = useState(false);

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    try {
      await createDomain({
        kind: { tag: draft.kind === "public" ? "Public" : "Private" },
        slug: draft.slug.trim(),
        title: draft.title.trim(),
        messageCharLimit: draft.messageCharLimit,
      });
      setDraft({ kind: draft.kind, slug: "", title: "", messageCharLimit: draft.messageCharLimit });
      onNotice({ kind: "success", text: "Domain created. It should appear in the directory immediately." });
    } catch (error) {
      onNotice({ kind: "error", text: error instanceof Error ? error.message : "Could not create domain." });
    } finally {
      setPending(false);
    }
  }

  return (
    <section className="panel">
      <h2>Create a domain</h2>
      <form onSubmit={onSubmit} className="stack">
        <div className="segmented">
          <button
            className={draft.kind === "public" ? "selected" : ""}
            type="button"
            onClick={() => setDraft({ ...draft, kind: "public" })}
          >
            Public
          </button>
          <button
            className={draft.kind === "private" ? "selected" : ""}
            type="button"
            onClick={() => setDraft({ ...draft, kind: "private" })}
          >
            Private
          </button>
        </div>
        <input
          value={draft.title}
          onChange={(event) => setDraft({ ...draft, title: event.target.value })}
          placeholder="Domain title"
        />
        <input
          value={draft.slug}
          onChange={(event) => setDraft({ ...draft, slug: event.target.value })}
          placeholder="domain-slug"
        />
        <label>
          Message limit
          <input
            type="number"
            min={32}
            max={1024}
            value={draft.messageCharLimit}
            onChange={(event) =>
              setDraft({ ...draft, messageCharLimit: Number(event.target.value) || 280 })
            }
          />
        </label>
        <button className="primary" disabled={pending} type="submit">
          {pending ? "Creating..." : "Create domain"}
        </button>
      </form>
    </section>
  );
}

function CreateDmCard({
  accounts,
  viewer,
  onNotice,
}: {
  accounts: readonly Account[];
  viewer: AccountLookup;
  onNotice: (notice: Notice) => void;
}) {
  const createDm = useReducer(reducers.createDm);
  const [draft, setDraft] = useState<DmDraft>({ title: "", recipients: [] });
  const [pending, setPending] = useState(false);

  const candidates = accounts
    .filter((account) => account.accountId !== viewer.accountId)
    .sort((left, right) => left.handle.localeCompare(right.handle));

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    try {
      await createDm({
        recipientAccountIds: draft.recipients.map((value) => BigInt(value)),
        title: draft.title.trim(),
      });
      setDraft({ title: "", recipients: [] });
      onNotice({ kind: "success", text: "DM is ready. Canonical DMs collapse automatically if one already existed." });
    } catch (error) {
      onNotice({ kind: "error", text: error instanceof Error ? error.message : "Could not create DM." });
    } finally {
      setPending(false);
    }
  }

  return (
    <section className="panel">
      <h2>Start a DM</h2>
      <form onSubmit={onSubmit} className="stack">
        <input
          value={draft.title}
          onChange={(event) => setDraft({ ...draft, title: event.target.value })}
          placeholder="Small-group title"
        />
        <label>
          Participants
          <select
            multiple
            value={draft.recipients}
            onChange={(event) =>
              setDraft({
                ...draft,
                recipients: Array.from(event.target.selectedOptions, (option) => option.value),
              })
            }
          >
            {candidates.map((account) => (
              <option key={account.accountId.toString()} value={account.accountId.toString()}>
                @{account.handle}
              </option>
            ))}
          </select>
        </label>
        <button className="primary" disabled={pending} type="submit">
          {pending ? "Creating..." : "Create DM"}
        </button>
      </form>
    </section>
  );
}

function DomainDirectory({
  domains,
  selectedDomainId,
  dmDomainIds,
  subscriptions,
  onSelectDomain,
}: {
  domains: readonly Domain[];
  selectedDomainId: string | null;
  dmDomainIds: Set<string>;
  subscriptions: Map<string, SubscriptionLookup>;
  onSelectDomain: (domainId: string) => void;
}) {
  return (
    <section className="panel directory">
      <div className="section-header">
        <h2>Domains</h2>
        <span>{domains.length}</span>
      </div>
      <div className="domain-list">
        {domains.map((domain) => {
          const domainId = domain.domainId.toString();
          const subscribed = subscriptions.has(domainId);
          const kind = dmDomainIds.has(domainId) ? "dm" : enumTag(domain.kind);
          return (
            <button
              key={domainId}
              className={`domain-pill ${selectedDomainId === domainId ? "active" : ""}`}
              type="button"
              onClick={() => onSelectDomain(domainId)}
            >
              <span className="domain-meta">
                <strong>{domain.title}</strong>
                <small>{domain.slug || `#${domainId}`}</small>
              </span>
              <span className="tags">
                <span>{kind}</span>
                {subscribed ? <span>subscribed</span> : null}
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
}

function DomainConversation({
  domain,
  subscription,
  isDm,
  onNotice,
}: {
  domain: Domain;
  subscription: SubscriptionLookup | undefined;
  isDm: boolean;
  onNotice: (notice: Notice) => void;
}) {
  const subscribeDomain = useReducer(reducers.subscribeDomain);
  const unsubscribeDomain = useReducer(reducers.unsubscribeDomain);
  const postMessage = useReducer(reducers.postMessage);
  const [messageDraft, setMessageDraft] = useState("");
  const [pending, setPending] = useState(false);
  const [rawMessages] = useTable(tables.message);
  const [subscribedMessages] = useTable(tables.my_subscribed_messages);

  const orderedMessages = mergeConversationMessages(
    rawMessages.filter((message) => message.domainId === domain.domainId),
    subscribedMessages.filter((message) => message.domainId === domain.domainId),
  ).sort((left, right) => Number(left.domainSequence - right.domainSequence));

  async function toggleSubscription() {
    setPending(true);
    try {
      if (subscription) {
        await unsubscribeDomain({ domainId: domain.domainId });
        onNotice({ kind: "success", text: `Stopped batching notifications for ${domain.title}.` });
      } else {
        await subscribeDomain({ domainId: domain.domainId, batchWindowSeconds: 30 });
        onNotice({ kind: "success", text: `Subscribed to ${domain.title}. New messages will stream in live.` });
      }
    } catch (error) {
      onNotice({
        kind: "error",
        text: error instanceof Error ? error.message : "Could not update the subscription.",
      });
    } finally {
      setPending(false);
    }
  }

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    try {
      await postMessage({
        domainId: domain.domainId,
        body: messageDraft.trim(),
        replyToMessageId: undefined,
      });
      setMessageDraft("");
    } catch (error) {
      onNotice({ kind: "error", text: error instanceof Error ? error.message : "Could not post message." });
    } finally {
      setPending(false);
    }
  }

  return (
    <section className="panel conversation">
      <div className="conversation-header">
        <div>
          <p className="eyebrow">{isDm ? "Direct message" : enumTag(domain.kind)}</p>
          <h2>{domain.title}</h2>
          <p className="subtle">
            {domain.slug ? `/${domain.slug}` : `Domain #${domain.domainId.toString()}`} • up to{" "}
            {domain.messageCharLimit} characters
          </p>
        </div>
        <button type="button" disabled={pending} onClick={toggleSubscription}>
          {subscription ? "Unsubscribe" : "Subscribe"}
        </button>
      </div>

      <div className="message-stream">
        {orderedMessages.map((message) => (
          <article key={message.messageId.toString()} className="message-card">
            <div className="message-topline">
              <strong>#{message.domainSequence.toString()}</strong>
              <span>author {message.authorAccountId.toString()}</span>
              <time>{formatTimestamp(message.createdAt)}</time>
            </div>
            <p>{message.body}</p>
          </article>
        ))}
      </div>

      <form onSubmit={onSubmit} className="composer">
        <textarea
          value={messageDraft}
          maxLength={Number(domain.messageCharLimit)}
          onChange={(event) => setMessageDraft(event.target.value)}
          placeholder={`Say something in ${domain.title}`}
          rows={5}
        />
        <div className="composer-footer">
          <span>{messageDraft.length} / {domain.messageCharLimit.toString()}</span>
          <button className="primary" disabled={pending || !messageDraft.trim()} type="submit">
            {pending ? "Sending..." : "Post message"}
          </button>
        </div>
      </form>
    </section>
  );
}

function mergeConversationMessages(
  rawMessages: readonly Message[],
  subscribedMessages: readonly SubscribedMessageLookup[],
) {
  const byId = new Map<string, ConversationMessage>();

  for (const message of rawMessages) {
    byId.set(message.messageId.toString(), {
      messageId: message.messageId,
      domainId: message.domainId,
      domainSequence: message.domainSequence,
      authorAccountId: message.authorAccountId,
      body: message.body,
      createdAt: message.createdAt,
      replyToMessageId: message.replyToMessageId,
    });
  }

  for (const message of subscribedMessages) {
    byId.set(message.messageId.toString(), {
      messageId: message.messageId,
      domainId: message.domainId,
      domainSequence: message.domainSequence,
      authorAccountId: message.authorAccountId,
      body: message.body,
      createdAt: message.createdAt,
      replyToMessageId: message.replyToMessageId,
    });
  }

  return Array.from(byId.values());
}

export default App;
