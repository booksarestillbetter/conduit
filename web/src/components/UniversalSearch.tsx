// web/src/components/UniversalSearch.tsx
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Search, X, Tv, Film, Ghost, Eye, Inbox, ExternalLink, AlertTriangle, Loader2 } from 'lucide-react';
import { universalSearch } from '../services/api';
import { ArrGrabRecord, ArrLookupResult, OmbiRequestRecord, PlexScrobbleRecord, UnifiedTorrent, UniversalSearchResponse } from '../types';
import { getEffectivePoster } from '../utils/mediaParser';

const DEBOUNCE_MS = 250;

interface UniversalSearchProps {
  isOpen: boolean;
  onClose: () => void;
  onOpenTorrent: (compoundId: string) => void;
  onNavigate: (path: string) => void;
}

type ResultItem =
  | { kind: 'torrent'; key: string; data: UnifiedTorrent }
  | { kind: 'sonarr' | 'radarr'; key: string; data: ArrLookupResult }
  | { kind: 'grab'; key: string; data: ArrGrabRecord; created_at: string }
  | { kind: 'scrobble'; key: string; data: PlexScrobbleRecord; created_at: string }
  | { kind: 'ombi'; key: string; data: OmbiRequestRecord; created_at: string };

const SECTION_LABEL: Record<ResultItem['kind'], string> = {
  torrent: 'Active & Seeding',
  sonarr: 'Sonarr',
  radarr: 'Radarr',
  grab: 'History',
  scrobble: 'History',
  ombi: 'History',
};

const HISTORY_LIMIT = 8;

function buildItems(res: UniversalSearchResponse | null): ResultItem[] {
  if (!res) return [];
  const items: ResultItem[] = [];

  for (const t of res.torrents) {
    items.push({ kind: 'torrent', key: `t:${t.compound_id}`, data: t });
  }
  for (const s of res.sonarr) {
    items.push({ kind: 'sonarr', key: `sonarr:${s.node_name}:${s.tvdb_id ?? s.title}`, data: s });
  }
  for (const r of res.radarr) {
    items.push({ kind: 'radarr', key: `radarr:${r.node_name}:${r.tmdb_id ?? r.title}`, data: r });
  }

  // Grabs, watch history and requests are three different tables but one chronological story
  // ("what do we know about this title") — interleave them by date rather than three separate
  // blocks the user has to scan independently.
  const history = [
    ...res.history.grabs.map((g) => ({ kind: 'grab' as const, key: `grab:${g.id}`, data: g, created_at: g.created_at })),
    ...res.history.scrobbles.map((s) => ({ kind: 'scrobble' as const, key: `scrobble:${s.id}`, data: s, created_at: s.created_at })),
    ...res.history.ombi_requests.map((o) => ({ kind: 'ombi' as const, key: `ombi:${o.id}`, data: o, created_at: o.created_at })),
  ];
  history.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
  items.push(...history.slice(0, HISTORY_LIMIT));

  return items;
}

function TorrentRow({ t }: { t: UnifiedTorrent }) {
  const dotColor =
    t.status === 'seeding' ? 'bg-emerald-500' : t.status === 'downloading' ? 'bg-sky-500' : t.status === 'error' ? 'bg-rose-500' : 'bg-slate-500';
  return (
    <div className="flex items-center space-x-3 px-4 py-2.5">
      <span className={`h-2 w-2 shrink-0 rounded-full ${dotColor}`} />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium text-slate-200">{t.name}</div>
        <div className="flex items-center space-x-2 text-[11px] text-slate-500">
          <span>{t.node}</span>
          <span>&middot;</span>
          <span className="capitalize">{t.status}</span>
          <span>&middot;</span>
          <span>{(t.percent_done * 100).toFixed(0)}%</span>
        </div>
      </div>
    </div>
  );
}

function ArrRow({ r }: { r: ArrLookupResult }) {
  return (
    <div className="flex items-start space-x-3 px-4 py-2.5">
      <img
        src={getEffectivePoster(r.poster_url, r.title)}
        alt=""
        className="h-12 w-8 shrink-0 rounded object-cover bg-slate-800"
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center space-x-2">
          <span className="truncate text-sm font-medium text-slate-200">
            {r.title}
            {r.year ? <span className="text-slate-500"> ({r.year})</span> : null}
          </span>
          <ExternalLink className="h-3 w-3 shrink-0 text-slate-500" />
        </div>
        <div className="flex items-center space-x-2 text-[11px] text-slate-500">
          <span
            className={`rounded px-1.5 py-0.5 font-bold ${
              r.in_library ? 'bg-emerald-500/10 text-emerald-400' : 'bg-amber-500/10 text-amber-400'
            }`}
          >
            {r.in_library ? 'In Library' : 'Add New'}
          </span>
          <span>{r.node_name}</span>
          {r.network && (
            <>
              <span>&middot;</span>
              <span>{r.network}</span>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function GrabRow({ g }: { g: ArrGrabRecord }) {
  const Icon = g.item_type === 'movie' ? Film : g.item_type === 'series' ? Tv : Ghost;
  return (
    <div className="flex items-center space-x-3 px-4 py-2.5">
      <Icon className="h-4 w-4 shrink-0 text-slate-500" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-slate-200">{g.title || g.scene_name}</div>
        <div className="flex items-center space-x-2 text-[11px] text-slate-500">
          <span className="capitalize">{g.status}</span>
          <span>&middot;</span>
          <span>{new Date(g.created_at).toLocaleDateString()}</span>
        </div>
      </div>
    </div>
  );
}

function ScrobbleRow({ s }: { s: PlexScrobbleRecord }) {
  const label = s.series_title
    ? `${s.series_title}${s.season_number != null ? ` S${String(s.season_number).padStart(2, '0')}E${String(s.episode_number ?? 0).padStart(2, '0')}` : ''}`
    : s.title;
  return (
    <div className="flex items-center space-x-3 px-4 py-2.5">
      <Eye className="h-4 w-4 shrink-0 text-slate-500" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-slate-200">{label}</div>
        <div className="flex items-center space-x-2 text-[11px] text-slate-500">
          <span>{s.user_name}</span>
          <span>&middot;</span>
          <span className="capitalize">{s.event.replace('media.', '')}</span>
          <span>&middot;</span>
          <span>{new Date(s.created_at).toLocaleDateString()}</span>
        </div>
      </div>
    </div>
  );
}

function OmbiRow({ o }: { o: OmbiRequestRecord }) {
  return (
    <div className="flex items-center space-x-3 px-4 py-2.5">
      <Inbox className="h-4 w-4 shrink-0 text-slate-500" />
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm text-slate-200">{o.title}</div>
        <div className="flex items-center space-x-2 text-[11px] text-slate-500">
          <span>{o.requested_by}</span>
          <span>&middot;</span>
          <span className="capitalize">{o.status}</span>
          <span>&middot;</span>
          <span>{new Date(o.created_at).toLocaleDateString()}</span>
        </div>
      </div>
    </div>
  );
}

export const UniversalSearch: React.FC<UniversalSearchProps> = ({ isOpen, onClose, onOpenTorrent, onNavigate }) => {
  const [query, setQuery] = useState('');
  const [result, setResult] = useState<UniversalSearchResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const items = useMemo(() => buildItems(result), [result]);

  const select = useCallback(
    (item: ResultItem) => {
      switch (item.kind) {
        case 'torrent':
          onOpenTorrent(item.data.compound_id);
          break;
        case 'sonarr':
        case 'radarr':
          window.open(item.data.open_url, '_blank', 'noopener,noreferrer');
          break;
        case 'grab':
          onNavigate('/archive');
          break;
        case 'scrobble':
        case 'ombi':
          // No dedicated page to send these to yet — showing the match here already answers
          // "have we seen this before", so there's nothing further to do on select.
          break;
      }
      onClose();
    },
    [onOpenTorrent, onNavigate, onClose]
  );

  useEffect(() => {
    if (!isOpen) return;
    setQuery('');
    setResult(null);
    setActiveIndex(0);
    const t = setTimeout(() => inputRef.current?.focus(), 10);
    return () => clearTimeout(t);
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    abortRef.current?.abort();

    const q = query.trim();
    if (q.length === 0) {
      setResult(null);
      setLoading(false);
      return;
    }

    setLoading(true);
    debounceRef.current = setTimeout(() => {
      const controller = new AbortController();
      abortRef.current = controller;
      universalSearch(q, controller.signal)
        .then((res) => {
          setResult(res);
          setActiveIndex(0);
        })
        .catch((e: any) => {
          if (e?.name !== 'AbortError') console.error('Universal search failed:', e);
        })
        .finally(() => setLoading(false));
    }, DEBOUNCE_MS);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query, isOpen]);

  useEffect(() => {
    if (!isOpen) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        setActiveIndex((i) => Math.min(i + 1, items.length - 1));
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setActiveIndex((i) => Math.max(i - 1, 0));
      } else if (e.key === 'Enter') {
        e.preventDefault();
        const item = items[activeIndex];
        if (item) select(item);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isOpen, items, activeIndex, onClose, select]);

  if (!isOpen) return null;

  let lastSection: string | null = null;

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-slate-950/70 backdrop-blur-sm pt-[12vh] px-4" onClick={onClose}>
      <div
        className="w-full max-w-2xl overflow-hidden rounded-2xl border border-slate-800 bg-slate-900 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center space-x-3 border-b border-slate-800 px-4 py-3.5">
          {loading ? <Loader2 className="h-4 w-4 shrink-0 animate-spin text-slate-500" /> : <Search className="h-4 w-4 shrink-0 text-slate-500" />}
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search fetchers, Sonarr, Radarr, and history..."
            className="flex-1 bg-transparent text-sm text-slate-100 placeholder-slate-500 focus:outline-none"
          />
          <button onClick={onClose} className="shrink-0 rounded p-1 text-slate-500 hover:bg-slate-800 hover:text-slate-300">
            <X className="h-4 w-4" />
          </button>
        </div>

        {result && result.warnings.length > 0 && (
          <div className="flex items-center space-x-2 border-b border-slate-800 bg-amber-500/5 px-4 py-2 text-xs text-amber-400">
            <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">{result.warnings.join(' · ')}</span>
          </div>
        )}

        <div className="max-h-[60vh] overflow-y-auto">
          {query.trim().length === 0 && (
            <div className="px-4 py-10 text-center text-sm text-slate-500">
              Start typing to search active &amp; seeding fetchers, Sonarr, Radarr, and history.
            </div>
          )}

          {query.trim().length > 0 && !loading && items.length === 0 && (
            <div className="px-4 py-10 text-center text-sm text-slate-500">No results for &ldquo;{query.trim()}&rdquo;.</div>
          )}

          {items.map((item, idx) => {
            const section = SECTION_LABEL[item.kind];
            const showHeader = section !== lastSection;
            lastSection = section;
            return (
              <div key={item.key}>
                {showHeader && (
                  <div className="sticky top-0 bg-slate-900/95 px-4 pt-3 pb-1 text-[11px] font-bold uppercase tracking-wider text-slate-500">
                    {section}
                  </div>
                )}
                <div
                  role="option"
                  aria-selected={idx === activeIndex}
                  onMouseEnter={() => setActiveIndex(idx)}
                  onClick={() => select(item)}
                  className={`cursor-pointer transition-colors ${idx === activeIndex ? 'bg-slate-800/80' : 'hover:bg-slate-800/40'}`}
                >
                  {item.kind === 'torrent' && <TorrentRow t={item.data} />}
                  {(item.kind === 'sonarr' || item.kind === 'radarr') && <ArrRow r={item.data} />}
                  {item.kind === 'grab' && <GrabRow g={item.data} />}
                  {item.kind === 'scrobble' && <ScrobbleRow s={item.data} />}
                  {item.kind === 'ombi' && <OmbiRow o={item.data} />}
                </div>
              </div>
            );
          })}
        </div>

        <div className="flex items-center justify-end space-x-3 border-t border-slate-800 px-4 py-2 text-[11px] text-slate-500">
          <span className="flex items-center space-x-1">
            <kbd className="rounded border border-slate-700 bg-slate-800 px-1">&uarr;&darr;</kbd>
            <span>navigate</span>
          </span>
          <span className="flex items-center space-x-1">
            <kbd className="rounded border border-slate-700 bg-slate-800 px-1">&crarr;</kbd>
            <span>open</span>
          </span>
          <span className="flex items-center space-x-1">
            <kbd className="rounded border border-slate-700 bg-slate-800 px-1">esc</kbd>
            <span>close</span>
          </span>
        </div>
      </div>
    </div>
  );
};
