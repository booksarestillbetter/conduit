// web/src/components/TorrentTable.tsx
import React, { useMemo, useState } from 'react';
import {
  Play,
  Pause,
  Trash2,
  CheckCircle,
  AlertCircle,
  Clock,
  ArrowDown,
  ArrowUp,
  Folder,
  ChevronDown,
  ChevronUp,
  Info,
  Tv,
  Film,
  RotateCcw,
  Zap,
} from 'lucide-react';
import { TorrentStatus, UnifiedTorrent } from '../types';
import { parseMediaRelease, getEffectivePoster, getPosterPlaceholder } from '../utils/mediaParser';

interface TorrentTableProps {
  torrents: UnifiedTorrent[];
  selectedIds: Set<string>;
  onToggleSelect: (compoundId: string) => void;
  onSelectAll: (select: boolean) => void;
  onStart: (compoundId: string) => void;
  onStop: (compoundId: string) => void;
  onDelete: (compoundId: string) => void;
  onViewDetails?: (compoundId: string) => void;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${units[i]}`;
}

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return '';
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
  const i = Math.floor(Math.log(bytesPerSec) / Math.log(1024));
  return `${(bytesPerSec / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

function formatEta(seconds: number): string {
  if (seconds < 0 || seconds > 86400 * 365) return '∞';
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
  return `${Math.floor(seconds / 86400)}d ${Math.floor((seconds % 86400) / 3600)}h`;
}

function getStatusBadge(status: TorrentStatus, errorString: string, isCircuitBroken?: boolean, isCanaryProbe?: boolean) {
  if (isCanaryProbe) {
    return (
      <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-amber-500/20 text-amber-300 border border-amber-500/30" title="Active Canary Probe monitoring tracker recovery">
        <Zap className="h-3 w-3 mr-1 text-amber-400 animate-pulse" />
        Canary Probe
      </span>
    );
  }
  if (isCircuitBroken) {
    return (
      <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/20" title="Paused by Tracker Circuit Breaker to relieve swarm pressure">
        <Zap className="h-3 w-3 mr-1 text-amber-400" />
        Pressure Relieved
      </span>
    );
  }
  switch (status) {
    case 'downloading':
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">Downloading</span>;
    case 'seeding':
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-sky-500/10 text-sky-400 border border-sky-500/20">Seeding</span>;
    case 'queued':
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-amber-500/10 text-amber-300 border border-amber-500/20">Queued</span>;
    case 'queuedseed':
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-sky-500/10 text-sky-300 border border-sky-500/20">Queued (Seed)</span>;
    case 'stopped':
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-slate-500/10 text-slate-400 border border-slate-500/20">Paused</span>;
    case 'checking':
    case 'checkwait':
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/20">Checking</span>;
    case 'idle':
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-indigo-500/10 text-indigo-400 border border-indigo-500/20">Idle</span>;
    case 'error':
      return (
        <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-rose-500/10 text-rose-400 border border-rose-500/20" title={errorString}>
          Error: {errorString.slice(0, 20)}...
        </span>
      );
    default:
      return <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-semibold bg-slate-500/10 text-slate-400">{status}</span>;
  }
}

interface TorrentRowProps {
  t: UnifiedTorrent;
  isSelected: boolean;
  onToggleSelect: (compoundId: string) => void;
  onStart: (compoundId: string) => void;
  onStop: (compoundId: string) => void;
  onDelete: (compoundId: string) => void;
  onViewDetails?: (compoundId: string) => void;
}

// Memoized so a WS telemetry tick that changes one torrent doesn't re-run parseMediaRelease's
// several regexes (and re-render the whole row) for every OTHER torrent in the table too — only
// rows whose own props actually changed re-render. This only pays off because the callback props
// below are stabilized with useCallback in the parent (App.tsx); otherwise a fresh function
// reference every render would defeat the memoization regardless of `t` being unchanged.
const TorrentRow: React.FC<TorrentRowProps> = React.memo(function TorrentRow({
  t,
  isSelected,
  onToggleSelect,
  onStart,
  onStop,
  onDelete,
  onViewDetails,
}) {
  const pct = (t.percent_done * 100).toFixed(1);
  const isDone = t.percent_done >= 1.0;
  const mediaInfo = parseMediaRelease(t.name, t.arr_grab);
  const posterSrc = getEffectivePoster(t.arr_grab?.poster_url, t.name);

  return (
    <tr
      className={`hover:bg-slate-800/40 transition-colors cursor-pointer ${
        isSelected ? 'bg-brand-950/20' : ''
      }`}
      onClick={(e) => {
        if ((e.target as HTMLElement).closest('input, button')) return;
        if (onViewDetails) onViewDetails(t.compound_id);
      }}
    >
      <td className="py-3 px-4" onClick={(e) => e.stopPropagation()}>
        <input
          type="checkbox"
          checked={isSelected}
          onChange={() => onToggleSelect(t.compound_id)}
          className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500 cursor-pointer"
        />
      </td>
      <td className="py-3 px-4 max-w-md">
        <div className="flex items-start space-x-3">
          <div className="w-9 h-13 rounded-lg overflow-hidden bg-slate-950 border border-slate-800 flex-shrink-0 shadow-sm hidden sm:block">
            <img
              src={posterSrc}
              alt={mediaInfo.cleanTitle || t.name}
              className="w-full h-full object-cover"
              referrerPolicy="no-referrer"
              onError={(e) => {
                (e.currentTarget as HTMLImageElement).src = getPosterPlaceholder(t.name);
              }}
            />
          </div>
          <div className="min-w-0 flex-1">
            <div
              className="truncate text-slate-100 font-bold hover:text-amber-400 transition-colors flex items-center space-x-2"
              title={t.name}
              onClick={() => onViewDetails && onViewDetails(t.compound_id)}
            >
              <span>{mediaInfo.displayTitle}</span>
            </div>

            <div className="text-[11px] font-mono text-slate-500 truncate max-w-sm" title={t.name}>
              {t.name}
            </div>

            {/* Arr Enrichment & Media Badges */}
            <div className="flex flex-wrap items-center gap-1.5 mt-1">
              {t.arr_grab && (
                <span className={`inline-flex items-center space-x-1 rounded px-1.5 py-0.5 text-[10px] font-bold border ${
                  mediaInfo.itemType === 'series'
                    ? 'bg-blue-500/10 text-blue-300 border-blue-500/30'
                    : 'bg-indigo-500/10 text-indigo-300 border-indigo-500/30'
                }`}>
                  {mediaInfo.itemType === 'series' ? <Tv className="h-2.5 w-2.5" /> : <Film className="h-2.5 w-2.5" />}
                  <span>{mediaInfo.itemType === 'series' ? 'Sonarr' : 'Radarr'}</span>
                </span>
              )}

              {mediaInfo.isSeasonPack && mediaInfo.seasonNumber !== undefined && (
                <span className="rounded bg-sky-500/15 px-1.5 py-0.5 text-[10px] font-bold text-sky-300 border border-sky-500/30">
                  📦 Season {mediaInfo.seasonNumber} Pack
                </span>
              )}

              {!mediaInfo.isSeasonPack && mediaInfo.episodeLabel && (
                <span className="rounded bg-sky-500/15 px-1.5 py-0.5 text-[10px] font-bold text-sky-300 border border-sky-500/30">
                  📺 {mediaInfo.episodeLabel}
                </span>
              )}

              {mediaInfo.isCompleteSeries && (
                <span className="rounded bg-indigo-500/15 px-1.5 py-0.5 text-[10px] font-bold text-indigo-300 border border-indigo-500/30">
                  📚 Complete Series
                </span>
              )}

              {mediaInfo.quality && (
                <span className="rounded bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-bold text-amber-300 border border-amber-500/20">
                  🏷️ {mediaInfo.quality}
                </span>
              )}

              {t.arr_grab?.indexer && (
                <span className="rounded bg-purple-500/10 px-1.5 py-0.5 text-[10px] font-bold text-purple-300 border border-purple-500/20">
                  📡 {t.arr_grab.indexer}
                </span>
              )}

              {t.arr_grab?.re_searched && (
                <span className="inline-flex items-center space-x-0.5 text-[10px] text-rose-400 font-semibold">
                  <RotateCcw className="h-2.5 w-2.5" />
                  <span>Re-searched</span>
                </span>
              )}
            </div>

            <div className="flex items-center space-x-3 text-xs text-slate-400 mt-0.5">
              <span className="font-mono">{t.tracker_stats[0]?.host || 'DHT/PEX'}</span>
              <span>&bull;</span>
              <span className="truncate" title={t.download_dir}>{t.download_dir}</span>
            </div>
          </div>
        </div>
      </td>
      <td className="py-3 px-4">
        <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-mono bg-slate-800 text-slate-300 border border-slate-700">
          {t.node}
        </span>
      </td>
      <td className="py-3 px-4 font-mono text-xs text-slate-300">
        {formatBytes(t.total_size)}
      </td>
      <td className="py-3 px-4">
        <div className="flex items-center justify-between text-xs mb-1">
          <span className="text-slate-300 font-mono">{pct}%</span>
          <span className="text-slate-400 font-mono">{formatBytes(t.uploaded_ever)} up</span>
        </div>
        <div className="w-full bg-slate-800 rounded-full h-1.5 overflow-hidden">
          <div
            className={`h-full rounded-full transition-all duration-300 ${
              t.status === 'error'
                ? 'bg-rose-500'
                : isDone
                ? 'bg-sky-500'
                : 'bg-emerald-500'
            }`}
            style={{ width: `${Math.min(parseFloat(pct), 100)}%` }}
          />
        </div>
      </td>
      <td className="py-3 px-4 font-mono text-xs">
        {t.rate_download > 0 && (
          <div className="flex items-center space-x-1 text-emerald-400">
            <ArrowDown className="h-3 w-3" />
            <span>{formatSpeed(t.rate_download)}</span>
          </div>
        )}
        {t.rate_upload > 0 && (
          <div className="flex items-center space-x-1 text-sky-400">
            <ArrowUp className="h-3 w-3" />
            <span>{formatSpeed(t.rate_upload)}</span>
          </div>
        )}
        {t.rate_download === 0 && t.rate_upload === 0 && (
          <span className="text-slate-500">-</span>
        )}
      </td>
      <td className="py-3 px-4 font-mono text-xs text-slate-300">
        {t.upload_ratio.toFixed(2)}
      </td>
      <td className="py-3 px-4 font-mono text-xs text-slate-400">
        {t.status === 'downloading' ? formatEta(t.eta) : '-'}
      </td>
      <td className="py-3 px-4">
        {getStatusBadge(t.status, t.error_string, t.is_circuit_broken, t.is_canary_probe)}
      </td>
      <td className="py-3 px-4 text-right" onClick={(e) => e.stopPropagation()}>
        <div className="flex items-center justify-end space-x-1.5">
          {onViewDetails && (
            <button
              onClick={() => onViewDetails(t.compound_id)}
              className="rounded p-1 text-slate-400 hover:bg-slate-700 hover:text-brand-400 transition-colors"
              title="Inspect Torrent Details & Timeline"
            >
              <Info className="h-4 w-4" />
            </button>
          )}
          {t.status === 'stopped' ? (
            <button
              onClick={() => onStart(t.compound_id)}
              className="rounded p-1 text-slate-400 hover:bg-slate-700 hover:text-emerald-400 transition-colors"
              title="Resume"
            >
              <Play className="h-4 w-4" />
            </button>
          ) : (
            <button
              onClick={() => onStop(t.compound_id)}
              className="rounded p-1 text-slate-400 hover:bg-slate-700 hover:text-amber-400 transition-colors"
              title="Pause"
            >
              <Pause className="h-4 w-4" />
            </button>
          )}
          <button
            onClick={() => onDelete(t.compound_id)}
            className="rounded p-1 text-slate-400 hover:bg-rose-950/40 hover:text-rose-400 transition-colors"
            title="Delete"
          >
            <Trash2 className="h-4 w-4" />
          </button>
        </div>
      </td>
    </tr>
  );
});

export const TorrentTable: React.FC<TorrentTableProps> = ({
  torrents,
  selectedIds,
  onToggleSelect,
  onSelectAll,
  onStart,
  onStop,
  onDelete,
  onViewDetails,
}) => {
  const [sortField, setSortField] = useState<keyof UnifiedTorrent>('added_date');
  const [sortAsc, setSortAsc] = useState(false);

  const handleSort = (field: keyof UnifiedTorrent) => {
    if (sortField === field) {
      setSortAsc(!sortAsc);
    } else {
      setSortField(field);
      setSortAsc(false);
    }
  };

  // Was recomputed (a full sort) on every render, including renders triggered by unrelated state
  // changes elsewhere in the app — with WS telemetry pushing updates roughly every second and
  // potentially hundreds of torrents, that's non-trivial repeated work for no benefit.
  const sortedTorrents = useMemo(() => {
    return [...torrents].sort((a, b) => {
      let valA = a[sortField];
      let valB = b[sortField];
      if (typeof valA === 'string') valA = (valA as string).toLowerCase();
      if (typeof valB === 'string') valB = (valB as string).toLowerCase();

      if (valA !== undefined && valB !== undefined) {
        if (valA < valB) return sortAsc ? -1 : 1;
        if (valA > valB) return sortAsc ? 1 : -1;
      }

      // Deterministic tie-breakers to prevent row jumping during live polling updates:
      // 1. Secondary: name (case-insensitive)
      const nameA = a.name.toLowerCase();
      const nameB = b.name.toLowerCase();
      if (nameA < nameB) return -1;
      if (nameA > nameB) return 1;

      // 2. Tertiary: compound_id
      return a.compound_id.localeCompare(b.compound_id);
    });
  }, [torrents, sortField, sortAsc]);

  const allSelected = sortedTorrents.length > 0 && selectedIds.size === sortedTorrents.length;

  return (
    <div className="flex-1 overflow-auto bg-slate-900 border-t border-slate-800">
      <table className="w-full text-left text-sm text-slate-300 border-collapse">
        <thead className="sticky top-0 bg-slate-950/95 backdrop-blur z-10 text-xs font-semibold uppercase tracking-wider text-slate-400 border-b border-slate-800 select-none">
          <tr>
            <th className="py-3 px-4 w-10">
              <input
                type="checkbox"
                checked={allSelected}
                onChange={(e) => onSelectAll(e.target.checked)}
                className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500 cursor-pointer"
              />
            </th>
            <th className="py-3 px-4 cursor-pointer" onClick={() => handleSort('name')}>
              <div className="flex items-center space-x-1">
                <span>Name & Content</span>
                {sortField === 'name' && (sortAsc ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
              </div>
            </th>
            <th className="py-3 px-4 w-28 cursor-pointer" onClick={() => handleSort('node')}>
              <div className="flex items-center space-x-1">
                <span>Node</span>
                {sortField === 'node' && (sortAsc ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
              </div>
            </th>
            <th className="py-3 px-4 w-28 cursor-pointer" onClick={() => handleSort('total_size')}>
              <div className="flex items-center space-x-1">
                <span>Size</span>
                {sortField === 'total_size' && (sortAsc ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
              </div>
            </th>
            <th className="py-3 px-4 w-44 cursor-pointer" onClick={() => handleSort('percent_done')}>
              <div className="flex items-center space-x-1">
                <span>Progress</span>
                {sortField === 'percent_done' && (sortAsc ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
              </div>
            </th>
            <th className="py-3 px-4 w-32 cursor-pointer" onClick={() => handleSort('rate_download')}>
              <div className="flex items-center space-x-1">
                <span>Down / Up</span>
                {sortField === 'rate_download' && (sortAsc ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
              </div>
            </th>
            <th className="py-3 px-4 w-24 cursor-pointer" onClick={() => handleSort('upload_ratio')}>
              <div className="flex items-center space-x-1">
                <span>Ratio</span>
                {sortField === 'upload_ratio' && (sortAsc ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
              </div>
            </th>
            <th className="py-3 px-4 w-24 cursor-pointer" onClick={() => handleSort('eta')}>
              <div className="flex items-center space-x-1">
                <span>ETA</span>
                {sortField === 'eta' && (sortAsc ? <ChevronUp className="h-3 w-3" /> : <ChevronDown className="h-3 w-3" />)}
              </div>
            </th>
            <th className="py-3 px-4 w-28">Status</th>
            <th className="py-3 px-4 w-28 text-right">Actions</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-800/60 font-medium">
          {sortedTorrents.length === 0 ? (
            <tr>
              <td colSpan={10} className="py-12 text-center text-slate-500 font-normal">
                No torrents found matching criteria.
              </td>
            </tr>
          ) : (
            sortedTorrents.map((t) => (
              <TorrentRow
                key={t.compound_id}
                t={t}
                isSelected={selectedIds.has(t.compound_id)}
                onToggleSelect={onToggleSelect}
                onStart={onStart}
                onStop={onStop}
                onDelete={onDelete}
                onViewDetails={onViewDetails}
              />
            ))
          )}
        </tbody>
      </table>
    </div>
  );
};
