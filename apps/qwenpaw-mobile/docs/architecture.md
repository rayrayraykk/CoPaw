# QwenPaw Mobile Architecture

## Scope

The first release connects one Android or iOS device to one QwenPaw
deployment. It supports QR pairing, a direct server login fallback, agent
selection, chat history, streaming chat, cancellation, and attachments.

## Pairing protocol

1. An authenticated Console requests `POST /api/auth/pairing` with the
   Console origin as `base_url`.
2. QwenPaw creates a random, one-time ticket that expires after two minutes.
   Only a SHA-256 digest of the ticket is retained in process memory.
3. The Console renders the returned `qwenpaw://pair` URI as a QR code.
4. Mobile scans the URI and sends the ticket to
   `POST /api/auth/pairing/redeem` at the embedded HTTPS origin.
5. QwenPaw atomically consumes the ticket and returns a 30-day access token.
6. Mobile stores the token in iOS Keychain or Android Keystore through
   Expo SecureStore.

Tickets are single-use, process-local, short lived, and never written to
disk. A failed redemption does not reveal whether a ticket existed.

## Client boundaries

- `src/api`: transport and wire-format parsing.
- `src/store`: session state and application actions.
- `src/app`: Expo Router screens.
- `src/components`: reusable native presentation components.
- `src/theme`: shared visual tokens.

The mobile client speaks the current QwenPaw API directly. It does not add a
legacy protocol adapter.
