# Conduit Inbound Webhook Reference

This document is a field-level reference for every webhook/notification payload that Sonarr, Radarr, Lidarr, Plex, and Ombi can send to Conduit's inbound receivers:

- `POST http://conduit.example.com:4242/api/sonarr/inbound`
- `POST http://conduit.example.com:4242/api/radarr/inbound`
- `POST http://conduit.example.com:4242/api/lidarr/inbound`
- `POST http://conduit.example.com:4242/api/plex/inbound`
- `POST http://conduit.example.com:4242/api/ombi/inbound`

As of 0.8.0, the Sonarr/Radarr/Lidarr endpoints also accept an optional `?zone=<id>` query
param for multi-tenant "zones" setups — see §6.5.

**Sonarr, Radarr, and Lidarr** sections are sourced directly from the vendored `.NET` source in this repo (`3rd/Sonarr`, `3rd/Radarr`, `3rd/Lidarr` under `src/NzbDrone.Core/Notifications/Webhook/`), not from third-party docs, so they reflect exactly what the versions checked into `3rd/` will emit. **Ombi** is likewise sourced from its vendored source (`3rd/Ombi`). **Plex** has no vendored source in this repo, so that section is sourced from Plex's official webhook documentation (`https://support.plex.tv/articles/115002267687-webhooks/`) plus cross-referenced third-party payload bindings where the article itself was unreachable — flagged inline where confidence is lower.

A combined **"Current Conduit Handling"** section (§6) documents what `src/api/arr_routes.rs` and `src/api/webhook_routes.rs` actually do with each event today, and where fields or whole event types still get dropped on the floor. §7 ranks concrete next steps. Sonarr/Radarr's ingestion was substantially rebuilt after the first version of this doc (commit `7954bca`, "complete Sonarr and Radarr webhook ingestion and system event routing") — §6 reflects the **current**, post-rebuild code, not the original gap list.

---

## 1. Sonarr Event Types

Master enum: `3rd/Sonarr/src/NzbDrone.Core/Notifications/Webhook/WebhookEventType.cs`

```csharp
public enum WebhookEventType
{
    Test, Grab, Download, Rename, SeriesAdd, SeriesDelete,
    EpisodeFileDelete, Health, ApplicationUpdate, HealthRestored,
    ManualInteractionRequired
}
```

Serialized as a **string** (`StringEnumConverter`), e.g. `"eventType": "Grab"`. All payloads inherit the base `WebhookPayload` fields:

| Field | Type | Notes |
|---|---|---|
| `eventType` | string | One of the enum values above |
| `instanceName` | string | Name configured in Sonarr's General settings |
| `applicationUrl` | string | Sonarr's configured external URL (often empty) |

Event → payload/builder mapping is in `Webhook.cs` / `WebhookBase.cs`.

### 1.1 `Test`

Sent by the "Test" button in Sonarr's connection settings. Uses `WebhookGrabPayload` shape but with fabricated data (`WebhookBase.BuildTestPayload`):
- `series` — fake series (`id:1`, `title:"Test Title"`, `tvdbId:1234`, `tags:["test-tag"]`)
- `episodes` — one fake episode (`id:123`, `seasonNumber:1`, `episodeNumber:1`, `title:"Test title"`)

No release/download fields are populated. **Conduit must not treat this as a real grab/import.**

### 1.2 `Grab` — `WebhookGrabPayload`

Fired when Sonarr sends a release to a download client.

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | see §1.9 |
| `episodes` | `WebhookEpisode[]` | see §1.10 |
| `release` | `WebhookRelease` | see §1.12 |
| `downloadClient` | string | Download client display name |
| `downloadClientType` | string | Download client implementation (e.g. `Transmission`) |
| `downloadId` | string | Client-side hash/ID (torrent hash for Transmission) |
| `customFormatInfo` | `WebhookCustomFormatInfo` | see §1.15 |

### 1.3 `Download` — `WebhookImportPayload`

Fired once **per file** when Sonarr imports a completed download into the library. If multiple episodes import as a batch, Sonarr may also fire `ImportComplete` (see §1.4) — note `ImportComplete` payload's own `eventType` is (by a quirk in `WebhookBase.BuildOnImportCompletePayload`) still literally `"Download"`, so `eventType` alone cannot distinguish per-file `Download` from the batch-complete variant; only the payload shape (presence of `episodeFile` vs `episodeFiles`) can.

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | |
| `episodes` | `WebhookEpisode[]` | |
| `episodeFile` | `WebhookEpisodeFile` | see §1.11; includes `sourcePath` |
| `isUpgrade` | bool | true if this replaced an existing file |
| `downloadClient` / `downloadClientType` / `downloadId` | string | |
| `deletedFiles` | `WebhookEpisodeFile[]` (nullable) | old file(s) replaced, only present when `isUpgrade` — each entry additionally carries `recycleBinPath` |
| `customFormatInfo` | `WebhookCustomFormatInfo` | |
| `release` | `WebhookGrabbedRelease` | see §1.13 — **note: smaller shape than Grab's `release`**, no quality/customFormats/languages |

### 1.4 `ImportComplete` (internal name; wire `eventType` = `"Download"`)

`WebhookImportCompletePayload`, fired once per **import batch** (e.g. a whole season pack) rather than per-file.

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | |
| `episodes` | `WebhookEpisode[]` | all episodes imported in this batch |
| `episodeFiles` | `WebhookEpisodeFile[]` | **plural** — distinguishes this from the per-file `Download` payload |
| `downloadClient` / `downloadClientType` / `downloadId` | string | |
| `release` | `WebhookGrabbedRelease` | |
| `fileCount` | int | computed, `episodeFiles.Count` |
| `sourcePath` | string | |
| `destinationPath` | string | |

**Implication for Conduit:** `payload.get("episodeFiles")` (array) vs `payload.get("episodeFile")` (object) is the only reliable discriminator between "single file imported" and "batch import complete" — both report `eventType: "Download"`.

### 1.5 `Rename` — `WebhookRenamePayload`

Fired when Sonarr renames files on disk (e.g. after a naming-format change or manual rename trigger), not tied to a grab/import.

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | |
| `renamedEpisodeFiles` | `WebhookRenamedEpisodeFile[]` | `WebhookEpisodeFile` + `previousRelativePath`, `previousPath` |

### 1.6 `SeriesAdd` — `WebhookSeriesAddPayload`

Fired when a new series is added to Sonarr (e.g. via Ombi/Overseerr auto-add, or manual add in the UI) — **before any search/grab happens**.

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | |

### 1.7 `SeriesDelete` — `WebhookSeriesDeletePayload`

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | |
| `deletedFiles` | bool | whether files on disk were also deleted |

### 1.8 `EpisodeFileDelete` — `WebhookEpisodeDeletePayload`

Fired when an episode file is deleted (including as part of an upgrade replacing an old file).

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | |
| `episodes` | `WebhookEpisode[]` | |
| `episodeFile` | `WebhookEpisodeFile` | the file that was deleted |
| `deleteReason` | string enum | `Upgrade`, `NoLinkedEpisodes`, `Manual`, `MissingFromDisk` (see `DeleteMediaFileReason`) |

**Implication:** `deleteReason` lets Conduit tell "this delete is part of a normal upgrade replace" (`Upgrade`) apart from "the file vanished off disk unexpectedly" (`MissingFromDisk`) or a user-initiated manual delete (`Manual`) — currently Conduit's delete handling doesn't branch on this at all (see §3).

### 1.9 `WebhookSeries` (shared shape)

| Field | Type |
|---|---|
| `id` | int |
| `title` | string |
| `titleSlug` | string |
| `path` | string |
| `tvdbId` | int |
| `tvMazeId` | int |
| `tmdbId` | int |
| `imdbId` | string |
| `malIds` | int[] |
| `aniListIds` | int[] |
| `type` | string (`SeriesTypes`: `standard`, `daily`, `anime`) |
| `year` | int |
| `genres` | string[] |
| `images` | `WebhookImage[]` (§1.14) |
| `tags` | string[] (tag **labels**, not IDs) |
| `originalLanguage` | object (`{id, name}`) |
| `originalCountry` | string |

Note: `overview`, `runtime`, and `ratings` are **not** part of `WebhookSeries` in this Sonarr version — Conduit's current code reads `series.overview`, `series.runtime`, `series.ratings.value` (arr_routes.rs:331,334-335), which will silently be `None`/absent for this payload shape. (These may exist in the live Sonarr API response for a series lookup, but not in the webhook payload itself — worth verifying against the deployed Sonarr version's actual JSON, since Servarr apps have changed payload shape across versions.)

### 1.10 `WebhookEpisode`

| Field | Type |
|---|---|
| `id` | int |
| `episodeNumber` | int |
| `seasonNumber` | int |
| `title` | string |
| `overview` | string |
| `airDate` | string (`yyyy-MM-dd`) |
| `airDateUtc` | datetime |
| `seriesId` | int |
| `tvdbId` | int |
| `finaleType` | string (e.g. `series`, `season`, `midseason`) |

### 1.11 `WebhookEpisodeFile`

| Field | Type |
|---|---|
| `id` | int |
| `relativePath` | string |
| `path` | string (absolute) |
| `quality` | string (quality name, e.g. `WEBDL-1080p`) |
| `qualityVersion` | int (revision/proper number) |
| `releaseGroup` | string |
| `sceneName` | string |
| `size` | long (bytes) |
| `dateAdded` | datetime |
| `languages` | object[] (`{id, name}`) |
| `mediaInfo` | `WebhookEpisodeFileMediaInfo` (nullable, §1.11a) |
| `sourcePath` | string (Download-event only) |
| `recycleBinPath` | string (deletedFiles context only) |

#### 1.11a `WebhookEpisodeFileMediaInfo`

| Field | Type |
|---|---|
| `audioChannels` | decimal |
| `audioCodec` | string |
| `audioLanguages` | string[] |
| `height` | int |
| `width` | int |
| `subtitles` | string[] |
| `videoCodec` | string |
| `videoDynamicRange` | string (e.g. `HDR`, empty for SDR) |
| `videoDynamicRangeType` | string (e.g. `HDR10`, `DV`) |

**Currently unused by Conduit entirely** — no ingestion of resolution/HDR/audio codec metadata today.

### 1.12 `WebhookRelease` (Grab-event release shape)

| Field | Type |
|---|---|
| `quality` | string |
| `qualityVersion` | int |
| `releaseGroup` | string |
| `releaseTitle` | string |
| `indexer` | string |
| `size` | long |
| `customFormatScore` | int |
| `customFormats` | string[] |
| `languages` | object[] |
| `indexerFlags` | string[] (e.g. `Internal`, `Freeleech`, `Scene`) |

### 1.13 `WebhookGrabbedRelease` (Download/ManualInteraction-event release shape — smaller)

| Field | Type |
|---|---|
| `releaseTitle` | string |
| `indexer` | string |
| `size` | long |
| `indexerFlags` | string[] |
| `releaseType` | string (`SingleEpisode`, `MultiEpisode`, `SeasonPack`) |

Notably **missing** vs `WebhookRelease`: no `quality`, `customFormats`, `customFormatScore`, `languages` on the `release` object itself for `Download` events — those live instead in the sibling `customFormatInfo` field and the `episodeFile.quality`.

### 1.14 `WebhookImage`

| Field | Type |
|---|---|
| `coverType` | string (`poster`, `banner`, `fanart`, `clearlogo`, `screenshot`) |
| `url` | string (local Sonarr-relative URL) |
| `remoteUrl` | string (original remote image URL — this is what's usable outside Sonarr's network) |

### 1.15 `WebhookCustomFormatInfo`

| Field | Type |
|---|---|
| `customFormats` | `{id, name}[]` |
| `customFormatScore` | int |

### 1.16 `Health` / `HealthRestored` — `WebhookHealthPayload`

Fired when a health check fails / when a previously-failing check recovers. **These payloads do not include `series`, `episodes`, or `release` at all** — they're system-level, not media-level.

| Field | Type | Description |
|---|---|---|
| `level` | string enum (`HealthCheckResult`: `notice`, `warning`, `error`) | severity |
| `message` | string | human-readable health message |
| `type` | string | health check source name (e.g. `IndexerStatusCheck`, `DownloadClientCheck`, `RootFolderCheck`, `ImportListStatusCheck`) |
| `wikiUrl` | string | link to Servarr wiki troubleshooting page for this check |

`applicationUrl` is notably omitted in `BuildHealthRestoredPayload` (only `BuildHealthPayload` sets it) — a small inconsistency in Sonarr's own source.

### 1.17 `ApplicationUpdate` — `WebhookApplicationUpdatePayload`

Fired when Sonarr itself updates.

| Field | Type |
|---|---|
| `message` | string |
| `previousVersion` | string |
| `newVersion` | string |

No `applicationUrl` is set for this event either (omitted in `BuildApplicationUpdatePayload`).

### 1.18 `ManualInteractionRequired` — `WebhookManualInteractionPayload`

Fired when a download is stuck/failed and needs manual user action in Sonarr's queue (e.g. failed import, stuck download that hit a fallback).

| Field | Type | Description |
|---|---|---|
| `series` | `WebhookSeries` | |
| `episodes` | `WebhookEpisode[]` | |
| `downloadInfo` | `WebhookDownloadClientItem` | `{quality, qualityVersion, title, indexer, size}` — snapshot from the download client queue item |
| `downloadClient` / `downloadClientType` / `downloadId` | string | |
| `downloadStatus` | string | tracked-download status enum (e.g. `Warning`, `FailedPending`) |
| `downloadStatusMessages` | `{title, messages[]}[]` | the actual reason(s) — e.g. "Not a preferred word upgrade", "Sample" |
| `customFormatInfo` | `WebhookCustomFormatInfo` | |
| `release` | `WebhookGrabbedRelease` | |

**This is a high-value event Conduit currently drops entirely** — it's the mechanism Sonarr uses to say "I grabbed something but can't finish automatically; a human (or Conduit) needs to look at this." `downloadStatusMessages` carries the actual failure reason text.

---

## 2. Radarr Event Types

Master enum: `3rd/Radarr/src/NzbDrone.Core/Notifications/Webhook/WebhookEventType.cs`

```csharp
public enum WebhookEventType
{
    Test, Grab, Download, Rename, MovieDelete, MovieFileDelete,
    Health, ApplicationUpdate, MovieAdded, HealthRestored,
    ManualInteractionRequired
}
```

Same base `WebhookPayload` fields (`eventType`, `instanceName`, `applicationUrl`). Radarr has **no `ImportComplete`/batch-import event** (movies import one file at a time, unlike season packs) and uses `movie`/`movieFile` terminology instead of `series`/`episode`.

### 2.1 `Test`

`WebhookBase.BuildTestPayload` — fake `movie` (`id:1`, `tmdbId` unset on movie itself, `year:1970`), fake `remoteMovie` (`tmdbId:1234`, `imdbId:"5678"`), fake `release`.

### 2.2 `Grab` — `WebhookGrabPayload`

| Field | Type | Description |
|---|---|---|
| `movie` | `WebhookMovie` | see §2.8 |
| `remoteMovie` | `WebhookRemoteMovie` | see §2.9 — the "release-time" ID snapshot |
| `release` | `WebhookRelease` | see §2.11, same shape as Sonarr's |
| `downloadClient` / `downloadClientType` / `downloadId` | string | |
| `customFormatInfo` | `WebhookCustomFormatInfo` | |

### 2.3 `Download` — `WebhookImportPayload`

| Field | Type | Description |
|---|---|---|
| `movie` | `WebhookMovie` | |
| `remoteMovie` | `WebhookRemoteMovie` | |
| `movieFile` | `WebhookMovieFile` | see §2.10; includes `sourcePath` |
| `isUpgrade` | bool | |
| `downloadClient` / `downloadClientType` / `downloadId` | string | |
| `deletedFiles` | `WebhookMovieFile[]` (nullable) | old file(s) replaced; each carries `recycleBinPath` |
| `customFormatInfo` | `WebhookCustomFormatInfo` | |
| `release` | `WebhookGrabbedRelease` | §2.12 (smaller shape, **no `releaseType` field** — Radarr's `WebhookGrabbedRelease` ctor doesn't take one, unlike Sonarr's) |

### 2.4 `Rename` — `WebhookRenamePayload`

| Field | Type |
|---|---|
| `movie` | `WebhookMovie` |
| `renamedMovieFiles` | `WebhookRenamedMovieFile[]` (`WebhookMovieFile` + `previousRelativePath`, `previousPath`) |

### 2.5 `MovieAdded` — `WebhookAddedPayload`

Fired when a movie is added to Radarr (e.g. via Ombi/Overseerr), before any search.

| Field | Type |
|---|---|
| `movie` | `WebhookMovie` |
| `addMethod` | string enum (`AddMovieMethod`: e.g. `manual`, `list`) — **Radarr-only field, no Sonarr equivalent** |

### 2.6 `MovieDelete` — `WebhookMovieDeletePayload`

| Field | Type |
|---|---|
| `movie` | `WebhookMovie` |
| `deletedFiles` | bool |
| `movieFolderSize` | long — **only populated when `deletedFiles` is true and a file existed**; otherwise `0` |

### 2.7 `MovieFileDelete` — `WebhookMovieFileDeletePayload`

| Field | Type |
|---|---|
| `movie` | `WebhookMovie` |
| `movieFile` | `WebhookMovieFile` |
| `deleteReason` | string enum (`DeleteMediaFileReason`: `Upgrade`, `NoLinkedEpisodes`→N/A for movies but enum is shared, `Manual`, `MissingFromDisk`) |

Same gap as Sonarr's `EpisodeFileDelete`: Conduit doesn't branch on `deleteReason` today.

### 2.8 `WebhookMovie` (shared shape)

| Field | Type |
|---|---|
| `id` | int |
| `title` | string |
| `year` | int |
| `filePath` | string (only set when constructed with a `movieFile` — not always present) |
| `releaseDate` | string (`yyyy-MM-dd`, physical release date) |
| `folderPath` | string |
| `tmdbId` | int |
| `imdbId` | string |
| `overview` | string |
| `genres` | string[] |
| `images` | `WebhookImage[]` |
| `tags` | string[] (labels) |
| `originalLanguage` | object |

Note: **no `ratings` or `runtime` field on `WebhookMovie`** at all (Conduit's `arr_routes.rs:590-591` reads `movie.runtime` / `movie.ratings.value`, which will always be `None` for this payload — same class of gap as the Sonarr series issue in §1.9).

### 2.9 `WebhookRemoteMovie`

| Field | Type |
|---|---|
| `tmdbId` | int |
| `imdbId` | string |
| `title` | string |
| `year` | int |

Essentially a redundant/lighter snapshot of movie identity at grab time — present on `Grab` and `Download` payloads alongside the full `movie` object.

### 2.10 `WebhookMovieFile`

| Field | Type |
|---|---|
| `id` | int |
| `relativePath` | string |
| `path` | string |
| `quality` | string |
| `qualityVersion` | int |
| `releaseGroup` | string |
| `sceneName` | string |
| `indexerFlags` | string — **Radarr serializes this as a single flags string (`.ToString()` on the enum), not a string array like Sonarr's episode file** |
| `size` | long |
| `dateAdded` | datetime |
| `languages` | object[] |
| `mediaInfo` | `WebhookMovieFileMediaInfo` (nullable, §2.10a) |
| `sourcePath` | string |
| `recycleBinPath` | string |

#### 2.10a `WebhookMovieFileMediaInfo`

Same field set as Sonarr's `WebhookEpisodeFileMediaInfo` (§1.11a): `audioChannels`, `audioCodec`, `audioLanguages[]`, `height`, `width`, `subtitles[]`, `videoCodec`, `videoDynamicRange`, `videoDynamicRangeType`.

### 2.11 `WebhookRelease` (Grab-event release shape — identical field set to Sonarr's)

`quality`, `qualityVersion`, `releaseGroup`, `releaseTitle`, `indexer`, `size`, `customFormatScore`, `customFormats[]`, `languages[]`, `indexerFlags[]`.

### 2.12 `WebhookGrabbedRelease` (Download/ManualInteraction-event release shape)

`releaseTitle`, `indexer`, `size`, `indexerFlags[]`. **No `releaseType` field** (Sonarr's equivalent has one for season-pack detection; Radarr has no concept of multi-file releases at the movie level so it's absent).

### 2.13 `WebhookImage`, `WebhookCustomFormatInfo`, `WebhookCustomFormat`, `WebhookDownloadClientItem`, `WebhookDownloadStatusMessage`

Byte-for-byte identical shapes to Sonarr's (§1.14, §1.15). Reuse the same parsing code on the Conduit side.

### 2.14 `Health` / `HealthRestored` — `WebhookHealthPayload`

Identical shape to Sonarr's (§1.16): `level`, `message`, `type`, `wikiUrl`. Same `applicationUrl`-omitted-on-restore quirk.

### 2.15 `ApplicationUpdate` — `WebhookApplicationUpdatePayload`

Identical shape to Sonarr's (§1.17): `message`, `previousVersion`, `newVersion`.

### 2.16 `ManualInteractionRequired` — `WebhookManualInteractionPayload`

| Field | Type |
|---|---|
| `movie` | `WebhookMovie` |
| `downloadInfo` | `WebhookDownloadClientItem` |
| `downloadClient` / `downloadClientType` / `downloadId` | string |
| `downloadStatus` | string |
| `downloadStatusMessages` | `{title, messages[]}[]` |
| `customFormatInfo` | `WebhookCustomFormatInfo` |
| `release` | `WebhookGrabbedRelease` |

Same shape as Sonarr's (§1.18); see §6.1 for current handling — this event is now actively handled by Conduit as of the post-rebuild code.

---

## 3. Lidarr Event Types

Master enum: `3rd/Lidarr/src/NzbDrone.Core/Notifications/Webhook/WebhookEventType.cs`

```csharp
public enum WebhookEventType
{
    Test, Grab, Download, DownloadFailure, ImportFailure, Rename,
    ArtistAdd, ArtistDelete, AlbumDelete, Health, Retag,
    ApplicationUpdate, HealthRestored
}
```

Same base `WebhookPayload` fields (`eventType`, `instanceName`, `applicationUrl`). Lidarr is **artist/album/track**-based rather than series/episode or movie-based, and has two structural differences from Sonarr/Radarr worth calling out up front:

- **No `ManualInteractionRequired` event.** Lidarr's nearest equivalents are `DownloadFailure` and `ImportFailure` — two separate events rather than one unified "needs a human" signal.
- **`Retag` is Lidarr-only** — fired when audio file ID3/metadata tags are rewritten without a file move (no Sonarr/Radarr equivalent).

Event → builder mapping (`Webhook.cs`): `OnGrab`→`Grab`, `OnReleaseImport`→`Download`, `OnDownloadFailure`→`DownloadFailure`, `OnImportFailure`→`ImportFailure`, `OnRename`→`Rename`, `OnTrackRetag`→`Retag`, `OnArtistAdd`→`ArtistAdd`, `OnArtistDelete`→`ArtistDelete`, `OnAlbumDelete`→`AlbumDelete`, `OnHealthIssue`→`Health`, `OnHealthRestored`→`HealthRestored`, `OnApplicationUpdate`→`ApplicationUpdate`.

### 3.1 `Test`

`WebhookGrabPayload` shape with fabricated `artist` (`id:1`, `name:"Test Name"`, `mbId:"aaaaa-aaa-aaaa-aaaaaa"`, `tags:["test-tag"]`) and one fake `albums[0]` (`id:123`, `title:"Test title"`). No `release`.

### 3.2 `Grab` — `WebhookGrabPayload`

| Field | Type | Description |
|---|---|---|
| `artist` | `WebhookArtist` | see §3.11 |
| `albums` | `WebhookAlbum[]` | **plural** — a release can cover multiple albums (e.g. box sets) |
| `release` | `WebhookRelease` | see §3.14 — carries `customFormats`/`customFormatScore` **directly**, no separate `customFormatInfo` wrapper like Sonarr/Radarr |
| `downloadClient` / `downloadClientType` / `downloadId` | string | |

### 3.3 `Download` — `WebhookImportPayload`

| Field | Type | Description |
|---|---|---|
| `artist` | `WebhookArtist` | |
| `album` | `WebhookAlbum` | **singular** (one album per import event) |
| `tracks` | `WebhookTrack[]` | |
| `trackFiles` | `WebhookTrackFile[]` | **plural** |
| `deletedFiles` | `WebhookTrackFile[]` (nullable) | only when `isUpgrade` |
| `isUpgrade` | bool | |
| `downloadClient` / `downloadClientType` / `downloadId` | string | |

**Structural gotcha: `WebhookImportPayload` has no `release` field at all.** Unlike Sonarr/Radarr's `Download` event (which carries a `WebhookGrabbedRelease`), Lidarr's Download/Import payload carries no release/indexer/quality-at-release-level info — that data only ever arrives on the earlier `Grab` event for the same release.

### 3.4 `DownloadFailure` — `WebhookDownloadFailurePayload`

Flat, no nested objects — and notably **no `artist`/`album` at all**, so this event cannot be tied to specific media without matching `releaseTitle`/`downloadId` back to an earlier `Grab` record:

| Field | Type |
|---|---|
| `quality` | string |
| `qualityVersion` | int |
| `releaseTitle` | string |
| `downloadClient` | string |
| `downloadId` | string |

### 3.5 `ImportFailure` — `WebhookImportPayload` (same class as Download, different `eventType`)

| Field | Type |
|---|---|
| `artist` | `WebhookArtist` |
| `tracks` | `WebhookTrack[]` |
| `trackFiles` | `WebhookTrackFile[]` |
| `deletedFiles` | `WebhookTrackFile[]` (nullable) |
| `isUpgrade` | bool |
| `downloadClient` / `downloadClientType` / `downloadId` | string |

Note: `BuildOnImportFailurePayload` never sets `Album` — only `artist`, `tracks`, `trackFiles` are populated, so album context is absent even though the underlying class has an `album` property.

### 3.6 `Rename` — `WebhookRenamePayload`

| Field | Type |
|---|---|
| `artist` | `WebhookArtist` |
| `renamedTrackFiles` | `WebhookRenamedTrackFile[]` (`WebhookTrackFile` + `previousPath` only — **no `previousRelativePath`**, unlike Sonarr's renamed-file shape) |

### 3.7 `Retag` — `WebhookRetagPayload`

| Field | Type |
|---|---|
| `artist` | `WebhookArtist` |
| `trackFile` | `WebhookTrackFile` (singular) |

### 3.8 `ArtistAdd` — `WebhookArtistAddPayload`

| Field | Type |
|---|---|
| `artist` | `WebhookArtist` |

### 3.9 `ArtistDelete` — `WebhookArtistDeletePayload`

| Field | Type |
|---|---|
| `artist` | `WebhookArtist` |
| `deletedFiles` | bool |

### 3.10 `AlbumDelete` — `WebhookAlbumDeletePayload`

| Field | Type |
|---|---|
| `artist` | `WebhookArtist` |
| `album` | `WebhookAlbum` |
| `deletedFiles` | bool |

### 3.11 `WebhookArtist` (shared shape)

| Field | Type | Notes |
|---|---|---|
| `id` | int | |
| `name` | string | |
| `disambiguation` | string | MusicBrainz disambiguation comment (e.g. "US rock band" vs. a same-named artist) — no Sonarr/Radarr equivalent |
| `path` | string | |
| `mbId` | string | **MusicBrainz artist UUID** — Lidarr's cross-reference ID, analogous to Sonarr's `tvdbId`/Radarr's `tmdbId` |
| `type` | string | `Person`, `Group`, `Character`, `Orchestra`, `Choir`, `Other` |
| `overview` | string | |
| `genres` | string[] | |
| `images` | `WebhookImage[]` | |
| `tags` | string[] | labels |

No `year`, `ratings`, or `runtime` fields exist on `WebhookArtist` (artists don't have a single release year).

### 3.12 `WebhookAlbum`

| Field | Type | Notes |
|---|---|---|
| `id` | int | |
| `mbId` | string | MusicBrainz release-group UUID |
| `title` | string | |
| `disambiguation` | string | |
| `overview` | string | |
| `albumType` | string | e.g. `Album`, `EP`, `Single`, `Broadcast` |
| `secondaryAlbumTypes` | string[] | e.g. `Live`, `Compilation`, `Remix`, `Soundtrack` |
| `releaseDate` | datetime (nullable) | note: `releaseDate`, not `airDate` |
| `genres` | string[] | |
| `images` | `WebhookImage[]` | |

### 3.13 `WebhookTrack` / `WebhookTrackFile`

`WebhookTrack`: `id`, `title`, `trackNumber` (**string**, not int — vinyl/multi-disc numbering like `"A1"`), `quality`, `qualityVersion`, `releaseGroup`.

`WebhookTrackFile`: `id`, `path`, `quality`, `qualityVersion`, `releaseGroup`, `sceneName`, `size`, `dateAdded`. **No `mediaInfo` object at all** — Lidarr's webhook does not expose audio codec/bitrate/channel details the way Sonarr/Radarr expose video `mediaInfo`. **No `relativePath`** either (only absolute `path`).

`WebhookRenamedTrackFile` extends `WebhookTrackFile` with `previousPath` only.

### 3.14 `WebhookRelease` (Grab-only)

`quality`, `qualityVersion`, `releaseGroup`, `releaseTitle`, `indexer`, `size`, `customFormatScore`, `customFormats[]`. **No `indexerFlags` field** (Sonarr/Radarr have it; Lidarr's `WebhookRelease` constructor never sets it).

### 3.15 `WebhookImage`

Same shape as Sonarr/Radarr: `coverType`, `url`, `remoteUrl`.

### 3.16 `Health` / `HealthRestored` — `WebhookHealthPayload`

Identical shape to Sonarr/Radarr (§1.16): `level`, `message`, `type`, `wikiUrl`. Same quirk: `applicationUrl` omitted on `HealthRestored`.

### 3.17 `ApplicationUpdate` — `WebhookApplicationUpdatePayload`

Identical to Sonarr/Radarr (§1.17): `message`, `previousVersion`, `newVersion`.

---

## 4. Plex Event Types

Source: [Plex webhooks documentation](https://support.plex.tv/articles/115002267687-webhooks/), cross-referenced against third-party payload bindings (`hekmon/plexwebhooks`) where the article's fine print (e.g. exact "unsupported client" caveats) couldn't be independently re-confirmed — flagged inline below. Unlike Sonarr/Radarr/Lidarr, there is no vendored Plex source in this repo, and **Plex webhooks are a Plex Pass–only feature**, configured per-account under Account Settings (tied to the account that configures them, not necessarily the server owner).

### 4.1 Event types

| Event | Fires when |
|---|---|
| `media.play` | Playback starts |
| `media.pause` | Playback paused |
| `media.resume` | Playback resumes from pause |
| `media.stop` | Playback stops before natural completion |
| `media.scrobble` | Media considered "watched" (~90% played) |
| `media.rate` | User rates an item |
| `library.new` | New item added to a library the user has access to |
| `admin.database.backup` | Scheduled database backup completes |
| `admin.database.corrupted` | Server detects database corruption |
| `device.new` | A new device accesses the owner's server |

### 4.2 Delivery mechanism

Plex POSTs `multipart/form-data` with a `payload` form field containing the JSON string, plus (for media events) a second `thumb` part carrying the actual poster/thumbnail **image bytes** — a real embedded attachment, not a URL. **There is no documented signature or shared-secret verification mechanism** — Plex does not sign or authenticate these requests in any way; anything reachable at the webhook URL can forge a payload.

### 4.3 Payload shape

| Field | Type | Notes |
|---|---|---|
| `event` | string | one of the event names above |
| `user` | bool | whether the event is user-initiated |
| `owner` | bool | whether the triggering account is the server owner |
| `Account.id` / `Account.thumb` / `Account.title` | int / url / string | avatar + username |
| `Server.title` / `Server.uuid` | string | |
| `Player.local` | bool | local vs. remote network |
| `Player.publicAddress` / `Player.title` / `Player.uuid` | ip / string / string | which device/player triggered it |
| `Metadata` | object | full item metadata, see §4.4 |

### 4.4 `Metadata` object (media-relevant subset of 60+ documented fields)

| Field | Notes |
|---|---|
| `librarySectionType`, `librarySectionID`, `librarySectionTitle` | which library the item belongs to |
| `ratingKey`, `key`, `guid` | Plex-internal identity |
| `Guid` (capital G, array) | `[{"id": "imdb://tt..."}, {"id": "tmdb://..."}, {"id": "tvdb://..."}]` — external ID list |
| `type` | `movie`, `episode`, `track`, `season`, `show`, etc. |
| `title`, `originalTitle`, `titleSort` | |
| `grandparentTitle`, `grandparentRatingKey`, `grandparentThumb`, `grandparentArt` | show-level (for episodes) |
| `parentTitle`, `parentIndex`, `parentRatingKey`, `parentThumb`, `parentYear` | season-level |
| `index` | episode/track number |
| `year`, `originallyAvailableAt` | |
| `summary`, `tagline` | |
| `thumb`, `art`, `banner` | image paths — **relative to the local Plex server** (`/library/metadata/…`), unlike Sonarr/Radarr's already-absolute `remoteUrl` — resolving them requires the server's own base URL + auth token |
| `duration`, `viewOffset` | milliseconds |
| `contentRating`, `studio` | |
| `Director[]`, `Writer[]`, `Producer[]`, `Role[]` (cast, with `.thumb`), `Genre[]`, `Country[]`, `Collection[]`, `Mood[]`, `Similar[]` | each an array of `{id, filter, tag, count}`-shaped items |
| `Rating[]` | array of `{image, value, type}` — critic/audience/user scores per source |
| `audienceRating`, `audienceRatingImage`, `userRating`, `viewCount`, `lastViewedAt`, `lastRatedAt` | |
| `chapterSource`, `subtype` | |

---

## 5. Ombi Event Types

Source: `3rd/Ombi/src/Ombi.Notifications/Agents/WebhookNotification.cs` (payload construction), `3rd/Ombi/src/Ombi.Api.External/NotificationServices/Webhook/WebhookApi.cs` (HTTP delivery), `3rd/Ombi/src/Ombi.Notifications/NotificationMessageCurlys.cs` (the field dictionary), `3rd/Ombi/src/Ombi.Helpers/NotificationType.cs` (event enum).

**Ombi's webhook is structurally unlike Sonarr/Radarr/Lidarr's.** It is not a strongly-typed, per-event payload — it's a **flat, single-level JSON object of string→string key/value pairs**, built from a fixed template-substitution dictionary ("Curlys") that is **identical in shape across every notification type**. Irrelevant fields for a given event are simply present as empty strings rather than omitted; there is no nested `movie`/`series`/`release` object at all.

### 5.1 Delivery mechanism

- Body is built in `WebhookNotification.cs` (`Run()`): loads the Curlys dictionary, adds a `notificationType` key (`type.ToString()`).
- Sent in `WebhookApi.cs` (`PushAsync`): keys are camelCased via `CamelCasePropertyNamesContractResolver` before serializing (e.g. `RequestId` → `requestId`), body sent as `application/json` — **not** multipart, unlike Plex.
- **Auth header is `Access-Token`** — not `Authorization` or an `X-*-Secret` style header.

### 5.2 `NotificationType` enum (exact `.ToString()` wire values for `notificationType`)

```
NewRequest, Issue, RequestAvailable, RequestApproved, AdminNote, Test,
RequestDeclined, ItemAddedToFaultQueue, WelcomeEmail, IssueResolved,
IssueComment, Newsletter, PartiallyAvailable, PlexWatchlistTokenExpired,
RequestDeleted, IssueInProgress, IssueDeleted
```

Each value maps 1:1 to a dedicated method in `WebhookNotification.cs` (`NewRequest`→`NewRequest`, `AvailableRequest`→`RequestAvailable`, `RequestDeclined`→`RequestDeclined`, etc.).

### 5.3 Full field list (every webhook payload carries all of these keys, camelCased)

| JSON key | Meaning | Notes |
|---|---|---|
| `notificationType` | Event type name (§5.2) | added outside the Curlys dictionary itself |
| `requestId` | Request DB id | **string**, not int |
| `requestedUser` | Username of requester | |
| `title` | Media title | For TV, pulled from the parent request's title |
| `requestedDate` | Localized long-date string (`.ToString("D")`) | e.g. `"Tuesday, 29 August 2026"` — **not ISO 8601** |
| `type` | Humanized request type | **`"Movie"`, `"TV Show"` (with a space), or `"Album"`** — localized via i18n resource strings, so this can differ under non-English Ombi locales |
| `additionalInformation` | Freeform string set by caller | |
| `longDate` / `shortDate` / `longTime` / `shortTime` | Server "now" timestamps | **not** event timestamps |
| `overview` | Plot overview | |
| `year` | Release year | string, e.g. `"2020"` |
| `episodesList` | Comma-joined episode numbers (TV only) | e.g. `"1,2,3"` |
| `seasonsList` | Comma-joined season numbers (TV only) | |
| `posterImage` | **Full resolved poster URL, prebuilt** | Movie/TV: `https://image.tmdb.org/t/p/w300/{path}`; Album: cover/disk art URL |
| `applicationName` / `applicationUrl` | Ombi instance branding/URL | |
| `issueDescription` / `issueCategory` / `issueStatus` / `issueSubject` / `newIssueComment` / `issueUser` | Issue-ticket fields | populated only for `Issue`/`IssueComment`/`IssueResolved`/`IssueInProgress`/`IssueDeleted` |
| `userName` / `alias` / `requestedByAlias` / `userPreference` | Requester identity variants | |
| `denyReason` | Decline reason text | populated for `RequestDeclined` |
| `availableDate` | Date marked available | |
| `requestStatus` | Precomputed human status string | one of: `"Available"`, `"Denied"`, `"Processing Request"`, `"Pending Approval"` |
| `providerId` | **Single generic external-id field** | Movie: TMDb id; TV: TVDB or TMDb id depending on Ombi's own configured provider; Album: a MusicBrainz GUID string (non-numeric) |
| `partiallyAvailableSeasonNumber` / `partiallyAvailableEpisodeNumbers` / `partiallyAvailableEpisodeCount` / `partiallyAvailableEpisodesList` | Populated only for `PartiallyAvailable` | |

**There is no `imdbId`, `tmdbId`, or `tvDbId` field at all** — only the single overloaded `providerId`, whose meaning depends on media type and Ombi's own provider configuration.

---

## 6. Current Conduit Handling

Route wiring: `src/api/mod.rs:79-90` (Sonarr/Radarr/Lidarr `/inbound` + legacy aliases), `:100-109` (Plex/Ombi `/inbound` + legacy aliases + list endpoints).

### 6.1 Sonarr & Radarr — `src/api/arr_routes.rs`

This pair was substantially rebuilt after the first version of this doc shipped (commit `7954bca`, "complete Sonarr and Radarr webhook ingestion and system event routing") and now handles nearly every event type via dedicated helper functions shared by both apps (and, for `Health`/`ApplicationUpdate`, by Lidarr too):

| Event | Handling |
|---|---|
| `Test` | short-circuits, logs, returns early (`sonarr_inbound` line 303, `radarr_inbound` mirrors it) |
| `Grab` | `status: "fetched"`, notif "Conduit Grabbed" |
| `Download` | `status: "imported"`, notif "Conduit Retrieved & Stored"; batch detection now works — `is_batch`/`batch_file_count`/`batch_total_size` are computed off `episodeFiles.len() > 1` (lines 827-829) instead of only reading a singular `episodeFile` |
| `EpisodeFileDelete` / `MovieFileDelete` | `status: "deleted"`; `deleteReason` is read (default `"Upgrade"` when absent) and **does** vary notification tone: `MissingFromDisk` posts a distinct "⚠️ File Missing From Disk" card and non-`Upgrade` reasons get a "Delete Reason" field, while only `Upgrade`-reason deletes get threaded as a reply under the existing grab card (routine upgrade cleanup) |
| `Rename` | routed to a dedicated `handle_arr_rename` helper (line 656) — no longer falls into the generic "tracked" bucket |
| `SeriesAdd` / `MovieAdded` | routed to `handle_arr_media_added` (line 550) — dedicated "🐕 Conduit Tracking" card, reads `addMethod` (line 564) and `genres[]` (line 583) |
| `SeriesDelete` / `MovieDelete` | routed to `handle_arr_media_deleted` (line 604) — dedicated "🗑️ Conduit Removed" card, reads `deletedFiles` bool and `movieFolderSize` |
| `Health` / `HealthRestored` | routed to shared `handle_arr_health_event` (line 364) — reads `level`, `message`, `type`, `wikiUrl` (line 375); no longer written into `ArrGrabRecord` with placeholder titles |
| `ApplicationUpdate` | routed to shared `handle_arr_app_update` (line 435) |
| `ManualInteractionRequired` | routed to `handle_arr_manual_interaction` (line 468) — reads `downloadStatus`, `downloadStatusMessages[].{title,messages[]}` (line 478-496) and surfaces them as a "🐕 Conduit Needs Help" alert; this was the single highest-value gap flagged in the original version of this doc and is now closed |

**Deserialization strategy is unchanged and still worth revisiting:** both handlers accept the body as raw untyped `serde_json::Value` — there are still no `#[derive(Deserialize)]` structs matching the payload shapes in §1/§2. Every field is pulled ad hoc via `payload.get("...")` chains, so a typo in a field path still fails silently (returns `None`) rather than at compile time or via a deserialize error.

**Remaining unread fields** (present in the payload per §1/§2 but never referenced in `arr_routes.rs`):
- `isUpgrade` — Conduit still infers "upgrade" indirectly via an `existing_grab` DB lookup rather than trusting the payload's own boolean.
- `releaseType` (Sonarr only: `SingleEpisode`/`MultiEpisode`/`SeasonPack`) — would let Conduit label season-pack grabs distinctly without inferring from `episodeFiles.len()`.
- `tags[]` (Sonarr/Radarr tag labels) — could drive per-tag routing/notification rules.
- `titleSlug`, `tvMazeId`, `malIds`/`aniListIds` (Sonarr anime cross-IDs), `originalLanguage`/`originalCountry`.
- `deleteReason` is read but not used to vary notification tone (see table above).

**mediaInfo / customFormatInfo / indexerFlags — now implemented:** `format_media_info_summary` (line 285), `format_indexer_flags` (line 325), and `format_custom_format_summary` (line 342) all parse these fields into human-readable notification-field strings (resolution tier, codec, HDR type, audio codec+channels, custom format names + score, indexer flags). This closes what was the largest single gap in the original version of this doc.

**Structural note carried over from the original analysis:** the vendored Sonarr `WebhookSeries` / Radarr `WebhookMovie` classes (§1.9, §2.8) do not define `overview`, `runtime`, or `ratings` fields at all, yet Conduit's code still reads all three off `series`/`movie`. This is likely harmless (fields simply resolve to `None`) but hasn't been re-verified against a live captured payload — worth doing once, since Servarr apps do drift payload shape across releases (both vendored `WebhookEventType.cs` files carry a `// TODO: In v4 this will likely be changed to the default camel case` comment as a concrete example of a documented future breaking change).

### 6.2 Lidarr — `src/api/arr_routes.rs`

`lidarr_inbound` routes `Test`/`Health`/`HealthRestored`/`ApplicationUpdate` through the same shared helpers as Sonarr/Radarr, and — as of this pass — also routes `ArtistAdd` (`handle_lidarr_artist_added`), `ArtistDelete`/`AlbumDelete` (`handle_lidarr_deleted`), and `DownloadFailure`/`ImportFailure` (`handle_lidarr_download_failure`, Lidarr's nearest equivalent to Sonarr/Radarr's `ManualInteractionRequired`) at the same early-dispatch point, matching the duplicated-check pattern Sonarr/Radarr already use in both their outer route handler and inner `process_*_direct` function. This closes what was previously the largest event-coverage gap in Lidarr handling.

Remaining/fixed items in `process_lidarr_inbound_direct`:
- **Fixed:** the status/notification match arms previously matched on `"Grab" | "Download" | "AlbumDownload" | "TrackFileDelete" | "Rename"` — `"AlbumDownload"` and `"TrackFileDelete"` are not real Lidarr `eventType` wire values (§3) and were dead code, most likely copy-pasted from the Sonarr/Radarr pattern. They've been removed; the match now only checks the real `"Grab" | "Download" | "Rename"`.
- **Fixed — structural mismatch on `Download`:** Lidarr's `WebhookImportPayload` has no `release` object at all (§3.3); the code now reads `trackFiles[]`/`tracks[]` (the plural arrays that actually exist on the payload) for release title, quality, and total size, falling back to `release`/singular `trackFile` only for `Grab`/`Retag`. Previously this data came only from a stale DB-cached `Grab` record.
- `artist_id`/`album_id` are now extracted from the payload and persisted on `ArrGrabRecord` (new nullable `artist_id`/`album_id` columns), which also fixes the re-search endpoint (§7).
- `artist.mbId` / `album.mbId` (MusicBrainz IDs), `artist.disambiguation`, `artist.type`, `album.albumType`/`secondaryAlbumTypes[]` are still never read — lower priority, and `mbId` would need its own schema column (a text field, unlike the integer `artist_id`/`album_id` Lidarr IDs) if it's ever wanted.

### 6.3 Plex — `src/api/webhook_routes.rs`

- **Auth**: `verify_webhook_secret` checked against `config.plex.token` — treats the Plex account token as a shared secret. Since Plex itself never signs webhook requests (§4.2), this only provides real protection if Conduit's own check is the only thing standing between the endpoint and an untrusted caller.
- **Payload extraction** (`extract_plex_json`): handles direct JSON and hand-rolled multipart scanning (finds `name="payload"` then slices between the first `{` and last `}`) — a fragile string-scan rather than a real MIME multipart parser, though it has worked in practice. **The `thumb` image part Plex sends alongside media events is still never extracted** — the handler signature takes `body: String`, so the embedded poster/thumbnail attachment bytes are discarded entirely. This is a real gap but a larger lift (switching the handler to accept and parse actual multipart bodies) than the fixes made in this pass — left open.
- **Fixed — event handling:** previously only `media.scrobble` and `library.new` triggered a notification. `admin.database.corrupted` (Plex's single highest-severity event) now always alerts unconditionally, same as the `*arr` Health-event pattern; `media.rate` now also notifies (gated on `notify_on_scrobble`) and surfaces `userRating` and the item's `summary` (plot blurb) in the card. `media.play`/`pause`/`resume`/`stop`, `admin.database.backup`, and `device.new` are still logged-only by design (too frequent/low-signal for a notification by default).
- **Metadata fields still available but unused**: `thumb`/`art`/`banner` (server-relative paths — need the Plex server's own base URL + token to resolve, unlike Sonarr/Radarr/Lidarr's already-absolute `remoteUrl`), `contentRating`, `Genre[]`/`Director[]`, `Rating[]`/`audienceRating`, `librarySectionTitle` (which library — useful for per-library routing/muting), `Player.title`/`Player.local` (which device, local vs. remote playback).

### 6.4 Ombi — `src/api/webhook_routes.rs`

`ombi_inbound` originally assumed a Sonarr/Radarr-style nested JSON shape, but Ombi's real payload is the flat Curlys dictionary described in §5 — this produced several concrete bugs, not just missing enrichment. Fixed in this pass:

- **Auth header mismatch (bug, fixed):** the shared `verify_webhook_secret` (used by all five integrations) never checked `Access-Token`, the actual header Ombi sends (§5.1) — it now does, alongside the existing `X-*-Secret`/`X-Plex-Token` headers.
- **Status-mapping bug (fixed):** the decline match arm was `"requestdenied" | "denied"`, but Ombi's real enum value is `RequestDeclined` (lowercased `"requestdeclined"`) — every real decline silently fell through to `"pending"`. Now matches `"requestdeclined"` (keeping `"requestdenied"` as a harmless fallback alias), and `"partiallyavailable"` now maps to `"available"` too.
- **Wrong field names for poster/IDs (fixed):** the code checked `posterPath`/`poster`/`banner`, but Ombi's real (and only) key is `posterImage` — now checked first. Likewise `providerId` (Ombi's single overloaded external-id field) is now read as a fallback, assigned to `tmdb_id` for movies or `tvdb_id` for TV based on `media_type`; the legacy `imdbId`/`theMovieDbId`/`tvDbId` aliases are kept for other potential senders but never actually match real Ombi payloads.
- **`media_type` normalization (fixed):** Ombi's real `type` field is a humanized string (`"Movie"`, `"TV Show"`, `"Album"`); it's now mapped to Conduit's normalized `"movie"`/`"tv"`/`"music"` instead of being stored verbatim (previously `"tv show"` with a space).
- **`denyReason` now surfaced** in the notification summary and as a card field on decline events; dedicated `action_label` titles were added for `PartiallyAvailable`, `Issue`/`IssueInProgress`, `IssueComment`, and `IssueResolved` (previously all fell to the generic "🐕 Conduit Kibble Update").
- **Still open:** `requestStatus` (a precomputed status string that could replace Conduit's own inference outright), `availableDate`, the full `issue*` field set (description/category/subject/comment/user — only the event *type* is distinguished now, not the issue's actual content), all `partiallyAvailable*` season/episode detail fields, and `applicationName`/`applicationUrl` (Ombi's own instance identity) are still never read.

### 6.5 Multi-tenant zone routing — `src/api/arr_routes.rs`

Added in 0.8.0. `sonarr_inbound`/`radarr_inbound`/`lidarr_inbound` all accept an optional
`?zone=<id>` query param matched against `AppConfig.zones[].id`:

- **Matched**: the zone's own `webhook_secret` (falling back to that app's global
  `webhook_secret` if the zone didn't set one) is used for `verify_webhook_secret` auth instead
  of the app's global secret, and the resulting `ArrGrabRecord` is saved with `zone_id` set.
  Sonarr/Radarr → Plex notify-takeover (§6.1) also narrows to just that zone's
  `plex_node_names` instead of every configured Plex node.
- **Absent or unmatched** (no `zones` configured, param omitted, or the id doesn't match any
  configured zone): falls back to exactly today's behavior — the app's single global
  `webhook_secret`, with `zone_id` left `None` on the saved grab. This is what makes zones purely
  additive: a single-instance setup with no `?zone=` param sees no behavior change at all.
- Cross-app auto-detection/rerouting (`detect_inbound_media_type` — a Sonarr payload arriving on
  the Radarr endpoint or vice versa) carries the resolved `zone_id` through to whichever handler
  it re-dispatches to.
- `GET /api/arr/pipeline` and `GET /api/arr/grabs` both accept a matching `?zone=<id>` query
  param to filter by `zone_id` (`all` or omitted returns every zone's grabs combined).
- **Per-zone live Arr stats**: `GET /api/arr/stats` also accepts `?zone=<id>`, returning that
  zone's own Sonarr/Radarr/Lidarr live telemetry (library counts, queue length, disk usage)
  instead of the legacy global primary instance's. `engines::arr_stats_poller` computes and
  caches one `ArrStatsResponse` per configured zone alongside the existing global one
  (`ArrStatsSnapshot { global, zones }`), on the same 30s cycle. The `arr_stats` WebSocket topic
  itself stays global-only (unchanged wire contract) — the Dashboard polls the zone-scoped REST
  endpoint directly (also every 30s) while a specific zone is selected, rather than a per-zone
  push being added to the WS protocol.

---

## 7. Recommendations for Smarter Ingestion

**Closed in this pass** (see §6.2-6.4 for detail): the Ombi `Access-Token` auth bug, the Ombi `RequestDeclined` status-mapping bug, Ombi's `posterImage`/`providerId` field names, Ombi `media_type` normalization, Ombi `denyReason` + dedicated issue/partial-availability notification titles; Lidarr's dead `"AlbumDownload"`/`"TrackFileDelete"` match arms; Lidarr `ArtistAdd`/`ArtistDelete`/`AlbumDelete`/`DownloadFailure`/`ImportFailure` handling; Lidarr `Download` reading real `trackFiles[]`/`tracks[]` instead of a nonexistent `release`; a new `artist_id`/`album_id` column pair on `arr_grabs` so Lidarr re-search can target a real album/artist instead of silently sending `AlbumSearch` with an empty ID array (see below); Plex `admin.database.corrupted` and `media.rate` notifications plus `summary` surfacing.

Remaining, ranked roughly by leverage-to-effort:

1. **Extract Plex's `thumb` multipart attachment** — Plex sends the actual poster image as attached bytes on every media event; `plex_inbound` still takes `body: String` and discards it, so Conduit falls back to no image at all for Plex-originated cards (every other integration has a working poster URL path). Requires switching the handler to a real multipart body parser, a larger lift than the fixes made in this pass.
2. **Add typed payload structs** for Sonarr/Radarr/Lidarr (`serde(tag = "eventType")` externally-tagged enums mirroring §1/§2/§3) to replace the ad-hoc `payload.get("...")` chains — this turns silent `None`s into compile-time-checked field access and makes a new event type a match-arm compile error instead of a silent fallthrough. Lower priority for Ombi/Plex since their payloads are either intentionally flat (Ombi) or already handled via a generic `Value` for good reason (Plex's large, sparsely-populated `Metadata` object).
3. **Surface the rest of Ombi's issue-ticket fields** (`issueDescription`/`issueCategory`/`issueSubject`/`newIssueComment`/`issueUser`) and `partiallyAvailable*` season/episode detail, and `requestStatus` — event *type* is now distinguished, but the actual issue content and partial-availability specifics still aren't read.
4. **Read Lidarr's `artist.mbId`/`album.mbId`** (MusicBrainz IDs) and `artist.disambiguation`/`type`, `album.albumType`/`secondaryAlbumTypes[]` — would need its own text column if `mbId` is wanted (the new `artist_id`/`album_id` columns are Lidarr's own integer IDs, not MusicBrainz UUIDs).
5. **Surface Plex's remaining `Metadata` fields** — `contentRating`, `Genre[]`/`Director[]`, `Rating[]`/`audienceRating`, `librarySectionTitle` (per-library routing/muting), `Player.title`/`Player.local` (which device, local vs. remote).
6. Capture and diff a few **live webhook payloads** from the actual deployed Sonarr/Radarr/Lidarr/Plex/Ombi versions against this reference — Servarr apps in particular do drift payload shape across releases, and Plex's article couldn't be fully re-verified live (§4) since the support page returned an HTTP 403 to direct fetch during this research.
