# Native service reliability

Scope: service reliability within the native Muzon plan. Workspace and UI migration
are described in NATIVE_MIGRATION.md. No credentials or live-account operations
were used for service verification.

## S3 content and conflict policy

Previously the planner considered matching lengths identical and uploaded every
size mismatch over the remote object. Equal-size tag edits were invisible, and
an independently changed remote file could be replaced by stale local content.

The planner now separates existing keys from missing keys. For every existing
key, sync streams SHA-256 over the local file and a remote GET. Only matching
hashes count as up to date. This works with multipart and encrypted objects;
ETags are opaque identifiers, not assumed MD5 digests. Read/comparison errors
are reported in `SyncOutcome.failures`, never treated as equality.

A differing hash is a content conflict in `failures`, regardless of size.
`SyncOutcome.conflicts` carries every conflicting key; its length is the count.
The first failure line reports the total conflict count, followed by per-key
conflicts. The current native UI displays the first ten failures as an error,
so conflicts cannot silently appear as an entirely successful sync. Both
copies are preserved. There is no automatic winner or replacement option in this
change. Local tag edits are detected and reported but do not silently overwrite
the remote original. Resolving a conflict currently requires choosing a copy
outside sync (keep a backup and use a distinct key/path if both are needed).

Missing remote tracks are uploaded with `If-None-Match: *`. A competing creation
between listing and PUT must fail rather than overwrite. Providers must enforce
conditional writes; unsupported requests should fail, with no unconditional
fallback. This guarantee needs verification on each compatible provider.

Remote-only files are downloaded only to absent destinations. The existing
same-directory temporary file, fsync, and atomic hard-link commit refuse
replacement, including a destination created during the download. Failed or
cancelled downloads remove the temporary file. No object/file deletion is issued.
Deleting locally is not propagated: a subsequent sync can restore the remote copy.
Deleting remotely is not propagated: the local copy can be uploaded again.

The hash strategy deliberately avoids a persistent manifest: a manifest can be
stale after another client changes an object. Cost per sync: **the full remote
contents of every common object are downloaded again**, in addition to actual new-file downloads. For a 100 GiB library already
present on both sides, an unchanged sync transfers approximately 100 GiB inbound
plus listing/metadata overhead, and reads approximately 100 GiB locally. Provider
egress charges can apply. `bytes_down` counts newly downloaded files only; it does
not include comparison traffic. Memory stays bounded while hashing.

There is no bandwidth-saving hash cache or checksum-header optimization yet.
A future optimization must distinguish a full-object SHA-256 from a multipart
composite checksum and handle absent/unsupported headers; an ETag is not enough.
Progress reuses phase `scan` with done/total and the current full key; the current
UI labels it “Сравнение файлов”. A new `compare` string would incorrectly render
as “Завершение” in that UI, so it is deliberately not emitted.

The native S3 help text describes SHA-256 comparisons, bandwidth use and preservation
of both conflicting copies. The completion notice reports the conflict count.

`library.json` still uses the existing union/max metadata merge and PUT, separately
from track conflict handling. It has no conditional merge/retry protocol: two
simultaneous writers can lose metadata updates. It is not a content manifest.

## Cancellation, persistence, and shutdown

The Hub service thread aborts and awaits its network flush and now-playing tasks
before exiting. It then hands off **all** SQLite outbox batches to the durable
scrobble queues (formerly only one batch of 100) and joins the service thread.
A persistence failure stops the drain, reports an error, and leaves the
unacknowledged SQLite suffix for the next run. Repeated shutdown is safe; worker
panics are now reported through `Hub::error()` rather than silently discarded.

Shutdown cancels active Hub S3 sync/check futures on their next poll and rejects
new sync/check calls. Sync credential loading also observes cancellation. A scan
waiter already started through `spawn_blocking` checks cancellation before
requesting a rescan and between controller barriers. Dropping the sync future
cancels its scan waiter through a drop guard, even without cancelling the caller's
token. New downloads are published only after completion.

Shutdown persists pending listens; it does not wait for successful HTTP delivery
or start a final network flush. Cancelling an in-flight submission leaves its
durable queue entries pending. A server might have accepted the request before
cancellation or before a failed local acknowledgement write: retry may submit
that listen again. Exactly-once scrobbling remains unsupported.

The core owns playback/session persistence and scan execution. The native UI
requests integration shutdown, then stops and joins the core before the final Hub
outbox drain. This ensures final core-produced listens enter the durable queues
before the service worker exits. Tray and integration joins follow the audio stop. Service shutdown does not cancel a scan already owned by the
core, nor can Rust forcibly interrupt a running keyring subprocess, blocking
filesystem syscall, or controller barrier. The Hub joins its service thread;
caller-owned S3 futures must still be polled/joined by their runtime. A request
already accepted by S3 can complete remotely after local cancellation.

## Automated verification

Run from the native workspace after its parent-owned lock resolution:

```sh
cargo fmt --check --package muzon-services
cargo test --offline --locked -p muzon-services
```

Verified from the root workspace: formatting passed; **51 tests passed, one
external-network probe ignored**, zero failures. No locks were regenerated by
this service work.

Tests use disposable profiles, fake vaults, loopback HTTP servers and the Python
S3 stub, with synthetic credentials only. Coverage includes equal-size tag edits
through a complete sync, unchanged hashes, opaque ETags, remote changes, missing
objects, conditional upload races, preserving existing downloads, cancelling a
partial download, cancelling an in-flight scrobble and reopening its queue,
shutdown during a pending S3 request, shutdown idempotence/rejection of new sync,
a 251-entry durable final drain, and
failed outbox handoff recovery. The ignored Yandex probe remains opt-in and is
not a real-account verification.

## Real-account and device verification checklist

These checks are **not performed** by the automated suite. Use a separate profile,
a disposable S3 prefix/bucket, copied audio and explicit test accounts. Enter keys
through the normal credential UI; never put them in this document, shell logs,
fixtures, screenshots or source files.

- S3: check connection on the intended provider; upload a copy, sync unchanged,
  then edit a tag while keeping exactly the same byte length. Expect one conflict,
  zero replacements, and unchanged independently downloaded remote bytes.
- Change only the remote copy, with both equal and different sizes. Expect a
  conflict and an unchanged local file. Test multipart/encrypted objects without
  relying on their ETags. Test denied GET/PUT, offline/reconnect and disk removal.
- Race a second client creating a new key during upload. Verify the provider
  rejects `If-None-Match: *` with a precondition failure and preserves its bytes.
- Cancel during comparison and download, then quit during an active sync. Verify
  no partial destination appears, no `.part` remains after ordinary cancellation,
  and a later sync recovers. Forced kill/power loss can leave `.part` files;
  automatic stale-part cleanup is not implemented.
- Remove a test file on each side independently and confirm the surviving copy
  is restored, not deleted. A rename is a new key; the old key remains.
- Last.fm: authorize a test account, verify now-playing and one counted listen,
  go offline, quit, restart, reconnect and retry. Inspect queue/error status and
  account history. Repeat for ListenBrainz with its test token. Check disconnect
  clears that account's pending queue and does not replay old history to a new one.
- Quit during a scrobble request and simulate a transient server failure. Confirm
  pending entries survive restart; record any duplicate caused by ambiguous HTTP
  acknowledgement as the known delivery limitation.
- Separately verify Wayland/GPU, HiDPI/scale changes, suspend/resume, removed disks,
  tray hide/restore and final session persistence in the native UI. Unit tests do
  not establish these results.

## Remaining limits

There is no global filesystem snapshot while hashing/uploading: concurrently
editing a file can produce a transient comparison or upload result; avoid tag
writes during sync and rerun after edits settle. Remote listings and GETs are
not one atomic snapshot either. New-file downloads do not verify against a
trusted remote checksum/version. Library folders with identical basenames can
map to the same remote key. Path checks reduce traversal/symlink escape risks but
are not an OS-level directory-handle sandbox against concurrent path changes.
Queue retention caps remain unchanged. Crash/power-loss durability and live
provider/account behavior require the checks above.
