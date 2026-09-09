// web/src/utils/mediaParser.ts
import { ArrGrabRecord } from '../types';

export interface ParsedMediaInfo {
  cleanTitle: string;
  seasonNumber?: number;
  episodeNumbers?: number[];
  episodeLabel?: string;
  isSeasonPack: boolean;
  isCompleteSeries: boolean;
  displayTitle: string;
  quality?: string;
  releaseGroup?: string;
  itemType?: 'series' | 'movie' | 'music' | 'unknown';
}

/**
 * Parses release name and optional Arr metadata to construct clear, deduplicated titles,
 * season/episode identifiers, season pack flags, and display labels.
 */
export function parseMediaRelease(rawName: string, arrGrab?: ArrGrabRecord): ParsedMediaInfo {
  const name = rawName || '';
  let seasonNumber = arrGrab?.season_number || undefined;
  let episodeNumbers: number[] = [];
  let isSeasonPack = false;
  let isCompleteSeries = false;

  // Parse episode numbers from arrGrab if present
  if (arrGrab?.episode_numbers) {
    try {
      const parsed = JSON.parse(arrGrab.episode_numbers);
      if (Array.isArray(parsed) && parsed.length > 0) {
        episodeNumbers = parsed.map(Number).filter((n) => !isNaN(n));
      }
    } catch {
      // Ignore JSON parse failure
    }
  }

  // 1. Detect Complete Series
  if (/\b(complete\.series|the\.complete\.series|all\.seasons|s\d{1,2}-s\d{1,2})\b/i.test(name)) {
    isCompleteSeries = true;
  }

  // 2. Parse Season & Episode from filename if not already in ArrGrab
  // Pattern: S01E02-E04 or S01E02
  const sEpMatch = name.match(/\bS(\d{1,2})E(\d{1,2})(?:[-~E](\d{1,2}))?\b/i);
  if (sEpMatch) {
    if (seasonNumber === undefined) {
      seasonNumber = parseInt(sEpMatch[1], 10);
    }
    if (episodeNumbers.length === 0) {
      const startEp = parseInt(sEpMatch[2], 10);
      if (sEpMatch[3]) {
        const endEp = parseInt(sEpMatch[3], 10);
        for (let i = startEp; i <= endEp; i++) {
          episodeNumbers.push(i);
        }
      } else {
        episodeNumbers.push(startEp);
      }
    }
  } else {
    // Pattern: 1x05
    const xEpMatch = name.match(/\b(\d{1,2})x(\d{1,2})\b/i);
    if (xEpMatch) {
      if (seasonNumber === undefined) {
        seasonNumber = parseInt(xEpMatch[1], 10);
      }
      if (episodeNumbers.length === 0) {
        episodeNumbers.push(parseInt(xEpMatch[2], 10));
      }
    } else {
      // Pattern: S05 (Season Pack) or Season 5
      const sPackMatch = name.match(/\b(?:S|Season[._\s]*)(\d{1,2})\b/i);
      if (sPackMatch) {
        if (seasonNumber === undefined) {
          seasonNumber = parseInt(sPackMatch[1], 10);
        }
        if (episodeNumbers.length === 0) {
          isSeasonPack = true;
        }
      }
    }
  }

  // If ArrGrab indicates multi-episode pack for a whole season
  if (arrGrab && arrGrab.item_type === 'series' && seasonNumber !== undefined && (episodeNumbers.length === 0 || episodeNumbers.length > 3)) {
    isSeasonPack = true;
  }

  // 3. Format Episode Label
  let episodeLabel: string | undefined;
  if (isCompleteSeries) {
    episodeLabel = 'Complete Series';
  } else if (isSeasonPack && seasonNumber !== undefined) {
    episodeLabel = `Season ${seasonNumber}`;
  } else if (seasonNumber !== undefined && episodeNumbers.length > 0) {
    const sStr = `S${String(seasonNumber).padStart(2, '0')}`;
    if (episodeNumbers.length === 1) {
      episodeLabel = `${sStr}E${String(episodeNumbers[0]).padStart(2, '0')}`;
    } else {
      const first = String(episodeNumbers[0]).padStart(2, '0');
      const last = String(episodeNumbers[episodeNumbers.length - 1]).padStart(2, '0');
      episodeLabel = `${sStr}E${first}-E${last}`;
    }
  } else if (seasonNumber !== undefined) {
    episodeLabel = `Season ${seasonNumber}`;
  }

  // 4. Clean Base Title (and prevent double year "(2012) (2012)") — always assigned below,
  // whether arrGrab has a title or we fall back to stripping tags from the raw release name.
  let rawTitle = arrGrab?.title?.trim();
  if (rawTitle && (rawTitle === arrGrab?.release_title || /\b(s\d{1,2}(e\d{1,2})?|1080p|720p|2160p|4k|h264|h265|x264|x265|web-dl|webrip)\b/i.test(rawTitle))) {
    const stripped = rawTitle
      .replace(/\b(2160p|1080p|720p|480p|uhd|4k|bluray|blu-ray|remux|web-dl|webrip|dvdrip|hevc|h264|h265|x264|x265|truehd|atmos|dts-hd|dts|ddp5\.1|ac3|flac|aac|s\d{1,2}(e\d{1,2})?|\d{1,2}x\d{1,2}|season[._\s]*\d{1,2}|repack|proper)\b.*/i, '')
      .replace(/[._-]+/g, ' ')
      .trim();
    if (stripped) {
      rawTitle = stripped;
    }
  }

  let cleanTitle: string;
  if (rawTitle) {
    let base = rawTitle.trim();
    // If base ends with a year without parentheses e.g. "Lanterns 2026", format as "Lanterns (2026)"
    const yearTrailingMatch = base.match(/^(.*?)\s+(19\d{2}|20\d{2})$/);
    if (yearTrailingMatch) {
      base = `${yearTrailingMatch[1].trim()} (${yearTrailingMatch[2]})`;
    }

    if (arrGrab?.year) {
      const yearStr = `(${arrGrab.year})`;
      if (base.endsWith(yearStr) || base.includes(yearStr)) {
        cleanTitle = base;
      } else {
        cleanTitle = `${base} ${yearStr}`;
      }
    } else {
      cleanTitle = base;
    }
  } else {
    // Strip common release tags to get a readable title
    let stripped = name
      .replace(/\b(2160p|1080p|720p|480p|uhd|4k|bluray|blu-ray|remux|web-dl|webrip|dvdrip|hevc|h264|h265|x264|x265|truehd|atmos|dts-hd|dts|ddp5\.1|ac3|flac|aac|s\d{1,2}(e\d{1,2})?|\d{1,2}x\d{1,2}|season[._\s]*\d{1,2}|repack|proper)\b.*/i, '')
      .replace(/[._-]+/g, ' ')
      .trim();

    const yearTrailingMatch = stripped.match(/^(.*?)\s+(19\d{2}|20\d{2})$/);
    if (yearTrailingMatch) {
      stripped = `${yearTrailingMatch[1].trim()} (${yearTrailingMatch[2]})`;
    }

    cleanTitle = stripped || name;
  }

  // 5. Construct full Display Title
  let displayTitle = cleanTitle;
  if (episodeLabel && !cleanTitle.toLowerCase().includes(episodeLabel.toLowerCase())) {
    displayTitle = `${cleanTitle} - ${episodeLabel}`;
  }

  // 6. Extract Quality
  let quality = arrGrab?.quality || undefined;
  if (!quality) {
    const qMatch = name.match(/\b(2160p|1080p|720p|480p|UHD|4K)(?:[._\s]*(WEB-DL|WEBRip|BluRay|REMUX|HDTV|DVDRip))?\b/i);
    if (qMatch) {
      quality = qMatch[2] ? `${qMatch[1].toUpperCase()} ${qMatch[2]}` : qMatch[1].toUpperCase();
    }
  }

  // 7. Extract Release Group (after last hyphen)
  let releaseGroup: string | undefined;
  const grpMatch = name.match(/-([A-Za-z0-9]+)(?:\[.*?\])?$/);
  if (grpMatch) {
    releaseGroup = grpMatch[1];
  }

  const isMusic = arrGrab?.item_type === 'music' || /\b(FLAC|MP3|AAC|ALAC|V0|320kbps|Lossless)\b/i.test(name);
  const itemType = arrGrab?.item_type === 'music'
    ? 'music'
    : arrGrab?.item_type === 'series'
    ? 'series'
    : arrGrab?.item_type === 'movie'
    ? 'movie'
    : isMusic
    ? 'music'
    : (seasonNumber !== undefined || isCompleteSeries ? 'series' : 'unknown');

  return {
    cleanTitle,
    seasonNumber,
    episodeNumbers: episodeNumbers.length > 0 ? episodeNumbers : undefined,
    episodeLabel,
    isSeasonPack,
    isCompleteSeries,
    displayTitle,
    quality,
    releaseGroup,
    itemType,
  };
}

export const DOG_POSTERS = [
  '/placeholders/post_no_country_for_old_pugs.jpeg',
  '/placeholders/poster_a_beautiful_bark.jpeg',
  '/placeholders/poster_a_corgis_life.jpeg',
  '/placeholders/poster_american_beauty.jpeg',
  '/placeholders/poster_barko_polo.jpeg',
  '/placeholders/poster_citizen_canine.jpeg',
  '/placeholders/poster_forrest_stump.jpeg',
  '/placeholders/poster_guardians_of_the_Bark.jpeg',
  '/placeholders/poster_lord_of_the_bones.jpeg',
  '/placeholders/poster_paws.jpeg',
  '/placeholders/poster_poodle_fiction.jpeg',
  '/placeholders/poster_schindlers_leash.jpeg',
  '/placeholders/poster_silence_of_the_labs.jpeg',
  '/placeholders/poster_star_trek.jpeg',
  '/placeholders/poster_the_bark_night.jpeg',
  '/placeholders/poster_the_barkfast_club.jpeg',
  '/placeholders/poster_the_kings_chew_toy.jpeg',
  '/placeholders/poster_the_wofl_of_bark_street.jpeg',
  '/placeholders/poster_top_dog.jpeg',
  '/placeholders/poster_woof_street.jpeg',
];

/**
 * Returns a consistent randomized dog poster for an item based on its name or ID seed.
 */
export function getPosterPlaceholder(seed?: string): string {
  if (!seed || seed.length === 0) return DOG_POSTERS[0];
  let hash = 0;
  for (let i = 0; i < seed.length; i++) {
    hash = (hash << 5) - hash + seed.charCodeAt(i);
    hash |= 0;
  }
  const idx = Math.abs(hash) % DOG_POSTERS.length;
  return DOG_POSTERS[idx];
}

/**
 * Returns the enriched poster URL if present, or falls back to a deterministic dog poster.
 */
export function getEffectivePoster(posterUrl?: string | null, seed?: string): string {
  if (posterUrl && posterUrl.trim().length > 0) {
    return posterUrl;
  }
  return getPosterPlaceholder(seed);
}
