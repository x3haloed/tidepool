# AGENTS

Tidepool currently has multiple parallel implementations of similar behavior:

- the Tidepool database/module and generated client bindings
- the OpenClaw Tidepool plugin in `plugins/openclaw-tidepool/`
- BetterClaw's built-in Tidepool channel and Tidepool tools in the betterclaw repo

When one path is changed and the others are not reviewed, the integrations drift in ways that are easy to miss during local debugging. A Tidepool fix discovered in one client often reflects a shared semantic requirement rather than a one-off bug.

## Core Rule

Treat these as linked surfaces, not isolated codepaths.

If you change one of the following:

- Tidepool account, domain, subscription, DM, or message semantics
- generated client bindings layout or usage assumptions
- thread-keying, reply mapping, cursoring, or self-echo behavior
- the OpenClaw Tidepool plugin in `plugins/openclaw-tidepool/`
- BetterClaw Tidepool behavior in the betterclaw repo: `src/channels/tidepool.rs`
- BetterClaw Tidepool tool behavior in the betterclaw repo: `src/tool/tool_tidepool.rs`

you must explicitly review the sibling implementations for the same issue.

## Required Drift Check

For any Tidepool semantic or integration change, check all of these:

1. Tidepool schema/client-binding assumptions in this repo
2. OpenClaw Tidepool plugin behavior in `plugins/openclaw-tidepool/`
3. BetterClaw Tidepool channel behavior in the betterclaw repo: `src/channels/tidepool.rs`
4. BetterClaw Tidepool tool behavior in the betterclaw repo: `src/tool/tool_tidepool.rs`

Do not assume a Tidepool bug or policy change is unique to the path where it was discovered.

## Symmetry Expectations

When behavior should be equivalent, keep it equivalent.

Examples:

- If Tidepool thread IDs, domain mapping, or reply semantics change here, verify whether BetterClaw and the OpenClaw plugin need the same change.
- If Tidepool message filtering, self-echo handling, or cursor behavior changes in one client, verify the other client behavior too.
- If a schema or generated binding change affects how account/domain/message data is read, verify all consumer codepaths that depend on those shapes.
- If the plugin package layout changes, keep it self-contained and verify that sibling integrations are not depending on stale repo-relative paths.

## Tests And Verification

Prefer paired verification over one-off fixes.

When fixing a Tidepool integration bug:

- add or update the test that reproduces the bug where practical
- verify the sibling client implementation if the same invariant should hold there
- document any intentionally unmirrored behavior in the commit or handoff note

If a sibling implementation cannot be updated immediately, leave a concrete note explaining the mismatch and the follow-up needed.

## Review Checklist

Before finishing a change in this area, answer these:

1. Does the same issue exist in both the OpenClaw plugin and BetterClaw's Tidepool support?
2. Did I check both BetterClaw Tidepool layers: channel and tools?
3. Did I check whether generated binding or schema assumptions changed underneath a client?
4. Did I add tests or at least a concrete verification step for the affected path?
5. If I did not mirror the change elsewhere, have I documented the reason?

## Bias

Bias toward explicit symmetry and explicit verification.

Do not rely on "the other Tidepool client probably works the same way."
