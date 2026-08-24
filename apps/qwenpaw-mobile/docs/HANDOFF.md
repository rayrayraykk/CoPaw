# QwenPaw Mobile Development Handoff

## Git state

- Branch: `feature/qwenpaw-mobile`
- Branch base: `df67a01cd49ff43c1f791370bd0724137d919cd8`
- Delivery commit title: `feat: add QwenPaw mobile client and QR pairing`
- To obtain the immutable delivery commit SHA after checkout, run:

  ```bash
  git log -1 --oneline
  ```

The branch contains only the QwenPaw mobile client, its Console entry point,
the backend pairing protocol, and related tests. Pre-existing untracked review
and research files in the source workspace were deliberately excluded.

## Delivered scope

The client is a single Expo and React Native application for Android and iOS.
It is not a WebView wrapper.

- One-time QR pairing from the existing QwenPaw Console.
- Direct QwenPaw URL and username/password login.
- AgentScope Platform login and hosted deployment discovery.
- Agent selection and persisted active connection.
- Chat list, chat history, streaming replies, cancellation, and deletion.
- Image, video, audio, and file attachments.
- Credentials stored through Expo SecureStore in iOS Keychain or Android
  Keystore.
- Responsive native UI using Lucide icons.

## Important paths

- `apps/qwenpaw-mobile/src/app`: Expo Router screens.
- `apps/qwenpaw-mobile/src/api`: QwenPaw transport, Platform discovery, QR
  parsing, and SSE parsing.
- `apps/qwenpaw-mobile/src/store/app.ts`: application state and chat actions.
- `apps/qwenpaw-mobile/src/storage/connection.ts`: secure persistence.
- `console/src/components/MobilePairingModal.tsx`: dynamic QR dialog.
- `src/qwenpaw/app/pairing.py`: one-time ticket store.
- `src/qwenpaw/app/routers/auth.py`: pairing create and redeem endpoints.
- `tests/unit/app/test_pairing.py`: ticket and endpoint coverage.

See `architecture.md` in this directory for the pairing sequence.

## Pairing contract

1. An authenticated Console calls `POST /api/auth/pairing` with its reachable
   origin.
2. The server returns a `qwenpaw://pair` URI and a base64 PNG QR code.
3. The QR contains the QwenPaw base URL and a random ticket, never a password
   or access token.
4. The ticket expires after 120 seconds and is accepted once by
   `POST /api/auth/pairing/redeem`.
5. With authentication enabled, redemption returns a 30-day QwenPaw token.
6. Mobile verifies the connection and stores it in SecureStore.

Tickets are stored only in process memory and only as a ticket identifier plus
a SHA-256 secret digest. A multi-worker QwenPaw deployment needs a shared
ticket store before this protocol can work reliably across workers.

The URL embedded by Console is `window.location.origin`. The phone must be able
to reach that address. `localhost` will not work from a physical phone; use a
LAN address or a public HTTPS deployment.

## AgentScope Platform integration

The current mobile flow uses the Platform web application's existing private
API surface:

- `POST /api/v1/auth/login`
- `GET /api/v1/app/list`
- `GET /api/v1/app/get?appId=...`
- `POST /api/v1/app/start`

The client currently selects the first returned deployment and waits up to
about 30 seconds when it must be started. These endpoints are not documented as
a stable public mobile API, so response and authentication changes on Platform
may require an update to `src/api/client.ts`.

## New-machine setup

Use Node.js 22.13 or newer from the Node 22 LTS line. Node 23 is outside the
supported engine range of React Native 0.86.

```bash
git fetch origin
git switch feature/qwenpaw-mobile
cd apps/qwenpaw-mobile
npm install --registry=https://registry.npmjs.org
npm run typecheck
npm run lint
npm test
```

Start a development client with one of:

```bash
npm run android
npm run ios
npm start
```

`npm run ios` starts Metro and opens the client in an iOS Simulator. It does
not start the QwenPaw backend. Run the existing QwenPaw server separately.

Expo SDK 57 requires Xcode 26.4 or newer for local native iOS compilation. The
original development machine has an M4 Pro but runs macOS 15.4.1, which cannot
install that Xcode version. Use a Mac with a compatible macOS/Xcode pair or an
Expo EAS cloud build.

## Verification completed

- Mobile TypeScript typecheck: passed.
- Mobile Expo ESLint: passed.
- Mobile unit tests: 6 passed.
- Expo dependency compatibility check: passed.
- Android Metro production export: passed.
- Backend pairing tests: 5 passed.
- Python pre-commit hooks for touched files: passed.
- Existing Console production build: passed.
- Console pairing component Prettier and ESLint checks: passed.
- `git diff --check`: passed.

Expo Doctor passed 17 of 18 checks. Its React Native Directory metadata check
received an unexpected response from the external service; no dependency
mismatch was reported. `npm install` reported 14 transitive audit findings
(12 moderate and 2 high). No automatic audit fix was applied because a forced
upgrade could change Expo-compatible dependency versions.

## Recommended next validation

1. Start a real QwenPaw deployment and open Console.
2. Confirm the `Pair mobile` action displays a QR code.
3. Scan from an Android and an iPhone on a network that can reach Console.
4. Verify agent selection, streaming chat, cancellation, history, and each
   attachment category against the live deployment.
5. Add a deployment picker if a Platform account can own multiple apps.
6. Replace the generated placeholder application icons before store release.
7. Configure EAS project ownership, signing, bundle identifiers, and store
   metadata before distributing binaries.

## Commit and push workflow

The delivery is intentionally one reviewable commit. After checking it out:

```bash
git show --stat --oneline HEAD
git status --short
```

The commit is local until explicitly pushed. To publish the branch to the
configured fork:

```bash
git push -u origin feature/qwenpaw-mobile
```
