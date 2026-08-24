# Apple Music provider status

Apple Music work is intentionally isolated on the `codex/apple-music` branch.
Spotify and Neon remain Chordrift's operational focus on `main` until an Apple
Developer Program membership has another concrete use, such as Photara beta or
distribution work.

## What can be built without paid access

The branch contains tested foundations for:

- importing an Apple Media Services `.p8` private key into macOS
  Passwords/Keychain;
- signing short-lived ES256 Apple developer tokens without retaining generated
  bearer tokens;
- receiving per-user MusicKit authorization through a loopback-only local page;
- making read-only Apple Music API requests;
- batching catalog lookups for as many as 25 ISRCs per request;
- searching the catalog for metadata fallback candidates; and
- decoding the fields needed for future scored matching, including Apple
  catalog ID, ISRC, duration, public URL, and the extended `audioVariants`
  metadata whose `dolby-atmos` value identifies Spatial Audio availability.

These components are fixture-tested but cannot be integration-tested against
Apple until Chordrift has a Media ID and Media Services key.

## What requires Apple Developer Program enrollment

Apple requires paid-program access to Certificates, Identifiers & Profiles.
That access is needed to register a Media ID, enable MusicKit, and create the
Media Services private key used to sign Apple Music developer tokens.

When work resumes:

1. Enroll in the Apple Developer Program.
2. Register a Media ID named `Chordrift` with an identifier such as
   `media.com.orbyts.chordrift` and enable MusicKit.
3. Create a Media Services key associated with that Media ID.
4. Download the `.p8` key once and import it into Chordrift's Keychain-backed
   credential store.
5. Complete MusicKit user authorization for the `personal` account.
6. Integration-test catalog access and ISRC-first matching.
7. Add persisted match decisions, ambiguity reporting, and user-facing CLI
   commands before merging into a release branch.

Official setup references:

- <https://developer.apple.com/help/account/capabilities/create-a-media-identifier-and-private-key>
- <https://developer.apple.com/documentation/applemusicapi/generating-developer-tokens>
- <https://developer.apple.com/documentation/applemusicapi/user-authentication-for-musickit>
- <https://developer.apple.com/documentation/applemusicapi/get-multiple-catalog-songs-by-isrc>

## Listening history

Apple Music API history endpoints expose recent plays, heavy rotation, and the
latest eligible Replay summary. They are not a complete event-level listening
ledger comparable to Spotify's extended streaming-history export.

Apple's privacy portal can provide a personal-data archive independently of a
developer membership. Chordrift should not finalize a parser based on assumed
filenames or third-party examples. Once an actual export is available, inspect
its schema and add it as a provider-specific, immutable, Git-ignored archive
source with the same principles as Spotify history:

- Neon remains authoritative;
- later cumulative exports supplement rather than duplicate known events;
- raw archives remain local recovery inputs; and
- unrelated account PII is excluded.

Privacy requests are available at <https://privacy.apple.com/>.

## Provider-data boundary

Apple catalog responses remain Apple provider inventory and matching evidence.
Canonical, user-owned library state remains the orchestration source. Before
shipping cross-provider synchronization, recheck the then-current MusicKit
agreement and ensure Chordrift's use of Apple metadata and playlist operations
stays within Apple's documented permissions.
