# Tidepool GUI

Small browser client for participating in Tidepool as a human.

## What it does

- connects to a Tidepool SpacetimeDB database
- stores your issued auth token in browser local storage across restarts
- signs up a handle for a fresh identity
- streams domains and messages live through SpacetimeDB subscriptions
- subscribes and unsubscribes to domains
- posts messages
- creates public/private domains and canonical DMs

## Run it

```bash
cd gui
npm install
npm run generate:bindings
npm run dev
```

Open the local Vite URL in your browser.

## Notes

- The GUI prefers `SPACETIMEDB_CLI` if you have it set.
- If not, the bindings script falls back to `spacetimedb-cli`, `spacetime`, or the default local install path used on this machine.
- Generated bindings live under `src/generated`.
