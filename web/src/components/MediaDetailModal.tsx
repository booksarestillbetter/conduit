// web/src/components/MediaDetailModal.tsx
import React, { useState } from 'react';
import {
  X,
  Sparkles,
  Film,
  Tv,
  Music,
  Clock,
  CheckCircle2,
  AlertTriangle,
  Folder,
  Database,
  ExternalLink,
  Code,
  Copy,
  Check,
  RotateCw,
  Trash2,
  Layers,
  ArrowRight,
  HardDrive,
  Radio,
  FileText,
  Play,
  Terminal,
  Search,
  Zap,
  ShieldAlert,
  Loader2,
} from 'lucide-react';
import { ArrGrabRecord } from '../types';
import { getEffectivePoster, getPosterPlaceholder } from '../utils/mediaParser';
import { GrabHistoryTimeline } from './GrabHistoryTimeline';
import { getStoredToken } from '../services/api';

interface MediaDetailModalProps {
  item: ArrGrabRecord | null;
  onClose: () => void;
  onReSearch?: (item: ArrGrabRecord) => Promise<void>;
  onDelete?: (item: ArrGrabRecord) => Promise<void>;
  onViewTorrent?: (compoundId: string) => void;
}

export const MediaDetailModal: React.FC<MediaDetailModalProps> = ({
  item,
  onClose,
  onReSearch,
  onDelete,
  onViewTorrent,
}) => {
  const [activeTab, setActiveTab] = useState<'overview' | 'timeline' | 'releases' | 'payload'>('overview');
  const [copiedHash, setCopiedHash] = useState(false);
  const [copiedPayload, setCopiedPayload] = useState(false);
  const [reSearching, setReSearching] = useState(false);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [loadingReleases, setLoadingReleases] = useState(false);
  const [releases, setReleases] = useState<any[]>([]);
  const [releasesError, setReleasesError] = useState<string | null>(null);
  const [grabbingGuid, setGrabbingGuid] = useState<string | null>(null);

  if (!item) return null;

  const isMovie = item.item_type === 'movie';
  const isMusic = item.item_type === 'music';
  const displayTitle = item.title || item.release_title;

  const formatSize = (bytes?: number) => {
    if (!bytes || bytes <= 0) return 'N/A';
    const gb = bytes / (1024 * 1024 * 1024);
    if (gb >= 1) return `${gb.toFixed(2)} GB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const getStageStep = (record: ArrGrabRecord) => {
    if (record.status === 'imported' || record.event_type === 'Download' || record.event_type === 'AlbumDownload') return 4;
    if (record.status === 'staged') return 3;
    if (record.status === 'fetched' || record.event_type === 'Grab') return 2;
    return 1;
  };

  const currentStep = getStageStep(item);

  const handleCopyHash = () => {
    if (!item.download_id) return;
    navigator.clipboard.writeText(item.download_id);
    setCopiedHash(true);
    setTimeout(() => setCopiedHash(false), 2000);
  };

  const handleCopyPayload = () => {
    if (!item.payload_json) return;
    navigator.clipboard.writeText(item.payload_json);
    setCopiedPayload(true);
    setTimeout(() => setCopiedPayload(false), 2000);
  };

  const handleTriggerReSearch = async () => {
    if (!onReSearch) return;
    setReSearching(true);
    setActionMessage(null);
    try {
      await onReSearch(item);
      setActionMessage('Re-search command dispatched to Arr client');
    } catch (e: any) {
      setActionMessage(`Re-search error: ${e.message}`);
    } finally {
      setReSearching(false);
    }
  };

  const handleFetchReleases = async () => {
    setLoadingReleases(true);
    setReleasesError(null);
    try {
      const params = new URLSearchParams();
      params.set('item_type', item.item_type || 'movie');
      if (item.movie_id) params.set('movie_id', String(item.movie_id));
      if (item.series_id) params.set('series_id', String(item.series_id));
      if (item.album_id) params.set('album_id', String(item.album_id));
      if (item.zone_id) params.set('zone_id', item.zone_id);

      const token = getStoredToken();
      const resp = await fetch(`/api/arr/search/releases?${params.toString()}`, {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
      });
      if (!resp.ok) {
        const err = await resp.json().catch(() => ({}));
        throw new Error(err.error || `HTTP ${resp.status}`);
      }
      const data = await resp.json();
      setReleases(Array.isArray(data) ? data : []);
    } catch (e: any) {
      setReleasesError(e.message || 'Failed to search releases');
    } finally {
      setLoadingReleases(false);
    }
  };

  const handleGrabRelease = async (rel: any) => {
    setGrabbingGuid(rel.guid);
    try {
      const token = getStoredToken();
      const resp = await fetch('/api/arr/search/grab', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({
          item_type: item.item_type || 'movie',
          guid: rel.guid,
          indexer_id: rel.indexerId || rel.indexer_id || 0,
          zone_id: item.zone_id,
        }),
      });
      if (!resp.ok) {
        const err = await resp.json().catch(() => ({}));
        throw new Error(err.error || `HTTP ${resp.status}`);
      }
      setActionMessage(`⚡ Grabbed release: ${rel.title}`);
    } catch (e: any) {
      setActionMessage(`Grab error: ${e.message}`);
    } finally {
      setGrabbingGuid(null);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 p-4 backdrop-blur-md animate-fadeIn">
      <div className="flex h-[90vh] w-full max-w-4xl flex-col rounded-3xl border border-slate-800 bg-slate-900 shadow-2xl overflow-hidden">
        {/* Top Header Banner */}
        <div className="relative border-b border-slate-800 bg-gradient-to-r from-slate-950 via-slate-900 to-slate-950 p-6 flex-shrink-0">
          <div className="flex items-start justify-between gap-4">
            <div className="flex items-start space-x-5 flex-1 min-w-0">
              {/* Media Poster */}
              <div className="w-20 h-28 md:w-24 md:h-36 rounded-2xl overflow-hidden bg-slate-950 border border-slate-800 flex-shrink-0 relative shadow-2xl">
                <img
                  src={getEffectivePoster(item.poster_url, item.scene_name || item.title || item.id)}
                  alt={displayTitle}
                  className="w-full h-full object-cover"
                  referrerPolicy="no-referrer"
                  onError={(e) => {
                    (e.currentTarget as HTMLImageElement).src = getPosterPlaceholder(item.scene_name || item.title || item.id);
                  }}
                />
                <span
                  className={`absolute top-1.5 left-1.5 px-2 py-0.5 rounded-md text-[10px] font-extrabold uppercase tracking-wider shadow-md ${
                    isMovie ? 'bg-indigo-600 text-white' : isMusic ? 'bg-emerald-600 text-white' : 'bg-blue-600 text-white'
                  }`}
                >
                  {isMovie ? 'Movie' : isMusic ? 'Music' : 'TV'}
                </span>
              </div>

              {/* Title & Media Metadata */}
              <div className="flex-1 min-w-0 space-y-2">
                <div className="flex items-center space-x-2 flex-wrap gap-y-1.5">
                  <span className="px-2.5 py-0.5 rounded-full text-[10px] font-bold bg-amber-500/15 text-amber-300 border border-amber-500/30 flex items-center space-x-1">
                    <span>🐕</span>
                    <span>Conduit Scent Tracked</span>
                  </span>
                  <span className={`px-2.5 py-0.5 rounded-full text-[10px] font-bold ${
                    isMusic
                      ? 'bg-emerald-500/15 text-emerald-300 border border-emerald-500/30'
                      : isMovie
                      ? 'bg-indigo-500/15 text-indigo-300 border border-indigo-500/30'
                      : 'bg-blue-500/15 text-blue-300 border border-blue-500/30'
                  }`}>
                    {isMusic ? 'LIDARR' : isMovie ? 'RADARR' : 'SONARR'}
                  </span>
                  {item.quality && (
                    <span className="px-2.5 py-0.5 rounded-full text-[10px] font-bold bg-emerald-500/15 text-emerald-300 border border-emerald-500/30">
                      🏷️ {item.quality}
                    </span>
                  )}
                  {item.indexer && (
                    <span className="px-2.5 py-0.5 rounded-full text-[10px] font-bold bg-purple-500/15 text-purple-300 border border-purple-500/30">
                      📡 {item.indexer}
                    </span>
                  )}
                </div>

                <div className="flex items-baseline space-x-2.5">
                  <h2 className="text-xl md:text-2xl font-black text-white tracking-tight truncate max-w-xl">
                    {displayTitle}
                  </h2>
                  {item.year && (
                    <span className="text-sm font-semibold text-slate-400">
                      ({item.year})
                    </span>
                  )}
                </div>

                <div className="text-xs font-mono text-slate-400 truncate max-w-xl">
                  {item.release_title}
                </div>

                {item.overview && (
                  <p className="text-xs text-slate-300 line-clamp-2 leading-relaxed font-sans max-w-2xl">
                    {item.overview}
                  </p>
                )}

                <div className="flex items-center space-x-4 text-xs text-slate-400 pt-0.5 flex-wrap gap-y-1">
                  <span>💾 <strong>Size:</strong> {formatSize(item.size_bytes)}</span>
                  {item.genres && <span>🎬 <strong>Genres:</strong> {item.genres}</span>}
                  {item.runtime_mins && <span>⏱️ <strong>Runtime:</strong> {item.runtime_mins}m</span>}
                  {item.imdb_id && (
                    <a
                      href={`https://www.imdb.com/title/${item.imdb_id}`}
                      target="_blank"
                      rel="noreferrer"
                      className="text-amber-400 hover:underline inline-flex items-center space-x-1 font-semibold"
                    >
                      <span>IMDb ↗</span>
                    </a>
                  )}
                  {item.tmdb_id && (
                    <a
                      href={`https://www.themoviedb.org/movie/${item.tmdb_id}`}
                      target="_blank"
                      rel="noreferrer"
                      className="text-sky-400 hover:underline inline-flex items-center space-x-1 font-semibold"
                    >
                      <span>TMDb ↗</span>
                    </a>
                  )}
                </div>
              </div>
            </div>

            {/* Action buttons on header */}
            <div className="flex items-center space-x-2 flex-shrink-0">
              {onReSearch && (
                <button
                  onClick={handleTriggerReSearch}
                  disabled={reSearching}
                  title="Dispatch automated interactive re-search in Sonarr/Radarr/Lidarr"
                  className="flex items-center space-x-1.5 px-3 py-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 hover:text-amber-400 border border-slate-700 text-xs font-semibold transition-all active:scale-95 disabled:opacity-50"
                >
                  <RotateCw className={`w-3.5 h-3.5 ${reSearching ? 'animate-spin' : ''}`} />
                  <span>Re-Search</span>
                </button>
              )}

              {onDelete && (
                <button
                  onClick={() => onDelete(item)}
                  title="Delete pipeline record"
                  className="p-2 rounded-xl bg-slate-800 hover:bg-rose-950/50 text-slate-400 hover:text-rose-400 border border-slate-700 transition-all"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              )}

              <button
                onClick={onClose}
                className="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-400 hover:text-white transition-all"
              >
                <X className="w-5 h-5" />
              </button>
            </div>
          </div>

          {actionMessage && (
            <div className="mt-3 text-xs px-3 py-1.5 rounded-xl bg-amber-500/10 text-amber-300 border border-amber-500/20 font-medium">
              {actionMessage}
            </div>
          )}
        </div>

        {/* Conduit's Scent Trail Stepper */}
        <div className="bg-slate-950/60 border-b border-slate-800 px-6 py-3.5 flex-shrink-0">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <Clock className="w-4 h-4 text-amber-400" />
              <span className="text-xs font-bold uppercase tracking-wider text-slate-300">
                Conduit's Scent Trail Stage
              </span>
            </div>
            <div className="text-xs text-slate-400 font-mono">
              Status: <strong className="text-emerald-400 uppercase">{item.status}</strong>
            </div>
          </div>

          <div className="grid grid-cols-4 gap-2 mt-3">
            {[
              { step: 1, label: 'Sniffed / Grabbed', icon: '🦴', color: 'amber' },
              { step: 2, label: 'Fetching', icon: '🐾', color: 'blue' },
              { step: 3, label: 'Staged for Intake', icon: '🎾', color: 'purple' },
              { step: 4, label: 'Retrieved / Imported', icon: '🏆', color: 'emerald' },
            ].map((s) => {
              const isPastOrCurrent = currentStep >= s.step;
              const isCurrent = currentStep === s.step;
              return (
                <div
                  key={s.step}
                  className={`p-2.5 rounded-xl border flex flex-col justify-between space-y-1 transition-all ${
                    isCurrent
                      ? 'bg-brand-500/15 border-brand-500/40 text-brand-300 shadow-md ring-1 ring-brand-500/30'
                      : isPastOrCurrent
                      ? 'bg-slate-900 border-slate-700/80 text-slate-200'
                      : 'bg-slate-950/40 border-slate-800/40 text-slate-600 opacity-60'
                  }`}
                >
                  <div className="flex items-center justify-between text-xs font-bold">
                    <span>{s.icon} Step {s.step}</span>
                    {isPastOrCurrent && <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />}
                  </div>
                  <div className="text-[11px] font-semibold truncate">{s.label}</div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Tab Navigation */}
        <div className="flex border-b border-slate-800 px-6 space-x-2 bg-slate-950/30 flex-shrink-0">
          {[
            { id: 'overview', label: 'Media & Intake Specs', icon: FileText },
            { id: 'timeline', label: 'Lifecycle & Timeline', icon: Clock },
            { id: 'releases', label: 'Interactive Release Search', icon: Search },
            { id: 'payload', label: 'Raw Webhook Payload', icon: Code },
          ].map((t) => {
            const Icon = t.icon;
            const active = activeTab === t.id;
            return (
              <button
                key={t.id}
                onClick={() => {
                  setActiveTab(t.id as any);
                  if (t.id === 'releases' && releases.length === 0) {
                    handleFetchReleases();
                  }
                }}
                className={`flex items-center space-x-2 px-4 py-3 text-xs font-bold border-b-2 transition-all ${
                  active
                    ? 'border-brand-500 text-brand-400 bg-brand-500/5'
                    : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-800/40'
                }`}
              >
                <Icon className="w-3.5 h-3.5" />
                <span>{t.label}</span>
              </button>
            );
          })}
        </div>

        {/* Tab Content */}
        <div className="p-6 overflow-y-auto space-y-5 flex-1">
          {activeTab === 'overview' && (
            <div className="space-y-4">
              {/* Active Torrent / Fetcher Link Card */}
              {item.download_id && (
                <div className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <div className="flex items-center space-x-2 text-xs font-bold text-slate-200 uppercase tracking-wider">
                      <HardDrive className="w-4 h-4 text-brand-400" />
                      <span>Cluster Fetcher & Download ID</span>
                    </div>
                    {item.download_client && (
                      <span className="px-2.5 py-0.5 rounded-full text-[10px] font-mono font-bold bg-slate-800 text-slate-300 border border-slate-700">
                        Node: {item.download_client}
                      </span>
                    )}
                  </div>

                  <div className="flex flex-col md:flex-row md:items-center justify-between gap-3 bg-slate-900/80 p-3 rounded-xl border border-slate-800">
                    <div className="space-y-0.5 min-w-0">
                      <div className="text-[10px] uppercase font-mono text-slate-500 font-bold">InfoHash / Download ID</div>
                      <div className="text-xs font-mono text-amber-300 break-all flex items-center gap-2">
                        <span>{item.download_id}</span>
                        <button
                          onClick={handleCopyHash}
                          className="text-slate-400 hover:text-white p-1"
                          title="Copy hash"
                        >
                          {copiedHash ? <Check className="w-3 h-3 text-emerald-400" /> : <Copy className="w-3 h-3" />}
                        </button>
                      </div>
                    </div>

                    {onViewTorrent && item.download_client && (
                      <button
                        onClick={() => onViewTorrent(`${item.download_client}:${item.download_id}`)}
                        className="flex items-center space-x-1.5 px-4 py-2 rounded-xl bg-brand-600 hover:bg-brand-500 text-white text-xs font-bold shadow-lg shadow-brand-500/20 transition-all active:scale-95 shrink-0"
                      >
                        <ExternalLink className="w-3.5 h-3.5" />
                        <span>Inspect Live Torrent</span>
                      </button>
                    )}
                  </div>
                </div>
              )}

              {/* 2-Column Grid for Technical Details */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
                {/* Staging & Intake Card */}
                <div className="rounded-2xl border border-slate-800 bg-slate-950/50 p-4 space-y-2.5">
                  <div className="text-xs font-bold uppercase tracking-wider text-purple-400 flex items-center space-x-1.5">
                    <Folder className="w-4 h-4" />
                    <span>Intake & Queue Routing</span>
                  </div>
                  <div className="space-y-2 text-slate-300 font-mono">
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase block">Event Type</span>
                      <strong className="text-slate-200">{item.event_type}</strong>
                    </div>
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase block">Quality Profile</span>
                      <strong className="text-slate-200">{item.quality || 'N/A'}</strong>
                    </div>
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase block">Indexer</span>
                      <strong className="text-purple-300">{item.indexer || 'N/A'}</strong>
                    </div>
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase block">Scene Name</span>
                      <span className="text-slate-400 break-all">{item.scene_name || 'N/A'}</span>
                    </div>
                  </div>
                </div>

                {/* Media Entity Specs */}
                <div className="rounded-2xl border border-slate-800 bg-slate-950/50 p-4 space-y-2.5">
                  <div className="text-xs font-bold uppercase tracking-wider text-sky-400 flex items-center space-x-1.5">
                    <Database className="w-4 h-4" />
                    <span>Database & Arr Identifiers</span>
                  </div>
                  <div className="space-y-2 text-slate-300 font-mono">
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase block">Conduit Pipeline UUID</span>
                      <span className="text-slate-400 break-all">{item.id}</span>
                    </div>
                    {item.series_id && (
                      <div>
                        <span className="text-slate-500 text-[10px] uppercase block">Series ID / Season / Episode</span>
                        <strong className="text-slate-200">
                          Series #{item.series_id} {item.season_number ? `• S${item.season_number}` : ''} {item.episode_numbers ? `E${item.episode_numbers}` : ''}
                        </strong>
                      </div>
                    )}
                    {item.movie_id && (
                      <div>
                        <span className="text-slate-500 text-[10px] uppercase block">Movie ID</span>
                        <strong className="text-slate-200">Movie #{item.movie_id}</strong>
                      </div>
                    )}
                    <div>
                      <span className="text-slate-500 text-[10px] uppercase block">First Recorded</span>
                      <span className="text-slate-400">{new Date(item.created_at).toLocaleString()}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}

          {activeTab === 'timeline' && (
            <div className="space-y-4">
              <div className="relative border-l-2 border-slate-800 ml-4 space-y-6 py-2">
                <div className="relative pl-6">
                  <span className="absolute -left-[9px] top-1 h-4 w-4 rounded-full bg-amber-400 ring-4 ring-slate-900 flex items-center justify-center text-[9px]">
                    🦴
                  </span>
                  <div className="text-xs font-bold text-amber-300">Intake Scent Captured ({item.event_type})</div>
                  <div className="text-xs text-slate-400 mt-0.5">
                    Grabbed from {item.indexer || 'Indexer'} by {isMusic ? 'Lidarr' : isMovie ? 'Radarr' : 'Sonarr'} client.
                  </div>
                  <div className="text-[10px] font-mono text-slate-500 mt-1">
                    {new Date(item.created_at).toLocaleString()}
                  </div>
                </div>

                <div className="relative pl-6">
                  <span className={`absolute -left-[9px] top-1 h-4 w-4 rounded-full ring-4 ring-slate-900 flex items-center justify-center text-[9px] ${
                    currentStep >= 2 ? 'bg-blue-400' : 'bg-slate-700'
                  }`}>
                    🐾
                  </span>
                  <div className={`text-xs font-bold ${currentStep >= 2 ? 'text-blue-300' : 'text-slate-500'}`}>
                    Active Fetcher Handshake
                  </div>
                  <div className="text-xs text-slate-400 mt-0.5">
                    Dispatched to node <strong className="text-slate-300 font-mono">{item.download_client || 'Fetcher'}</strong>.
                  </div>
                </div>

                <div className="relative pl-6">
                  <span className={`absolute -left-[9px] top-1 h-4 w-4 rounded-full ring-4 ring-slate-900 flex items-center justify-center text-[9px] ${
                    currentStep >= 3 ? 'bg-purple-400' : 'bg-slate-700'
                  }`}>
                    🎾
                  </span>
                  <div className={`text-xs font-bold ${currentStep >= 3 ? 'text-purple-300' : 'text-slate-500'}`}>
                    Local Queue Staging & Hardlink Pipeline
                  </div>
                  <div className="text-xs text-slate-400 mt-0.5">
                    Download completed and processed through Conduit intake hook scripts.
                  </div>
                </div>

                <div className="relative pl-6">
                  <span className={`absolute -left-[9px] top-1 h-4 w-4 rounded-full ring-4 ring-slate-900 flex items-center justify-center text-[9px] ${
                    currentStep >= 4 ? 'bg-emerald-400' : 'bg-slate-700'
                  }`}>
                    🏆
                  </span>
                  <div className={`text-xs font-bold ${currentStep >= 4 ? 'text-emerald-300' : 'text-slate-500'}`}>
                    Media Library Import Completed
                  </div>
                  <div className="text-xs text-slate-400 mt-0.5">
                    Verified imported and indexed into central media catalog.
                  </div>
                </div>
              </div>

              <div className="rounded-2xl border border-slate-800 bg-slate-950/50 p-4">
                <div className="text-xs font-bold uppercase tracking-wider text-slate-300 mb-3 flex items-center space-x-1.5">
                  <Clock className="w-4 h-4 text-brand-400" />
                  <span>Recorded Event History</span>
                </div>
                <p className="text-[11px] text-slate-500 mb-3">
                  The stage overview above is a snapshot derived from current status. This is the actual event-by-event record — every grab, import, delete, and re-search Conduit has seen for this specific media item, even across replacements.
                </p>
                <GrabHistoryTimeline grabId={item.id} />
              </div>
            </div>
          )}

          {activeTab === 'releases' && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <h3 className="text-sm font-bold text-slate-200">Interactive Indexer Release Picker</h3>
                  <p className="text-xs text-slate-400">Directly query Sonarr/Radarr indexers and pick a specific release without leaving Conduit.</p>
                </div>
                <button
                  type="button"
                  onClick={handleFetchReleases}
                  disabled={loadingReleases}
                  className="flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-brand-500/10 text-brand-400 border border-brand-500/20 text-xs font-semibold hover:bg-brand-500/20 disabled:opacity-50"
                >
                  <RotateCw className={`w-3.5 h-3.5 ${loadingReleases ? 'animate-spin' : ''}`} />
                  <span>{loadingReleases ? 'Searching Indexers...' : 'Search Indexers'}</span>
                </button>
              </div>

              {releasesError && (
                <div className="p-3 rounded-xl bg-rose-500/10 text-rose-300 border border-rose-500/20 text-xs flex items-center space-x-2">
                  <ShieldAlert className="w-4 h-4 shrink-0" />
                  <span>{releasesError}</span>
                </div>
              )}

              {loadingReleases && (
                <div className="flex flex-col items-center justify-center py-12 space-y-2 text-slate-400">
                  <Loader2 className="w-6 h-6 animate-spin text-brand-400" />
                  <span className="text-xs">Querying connected indexers via {isMovie ? 'Radarr' : isMusic ? 'Lidarr' : 'Sonarr'}...</span>
                </div>
              )}

              {!loadingReleases && releases.length === 0 && !releasesError && (
                <div className="text-center py-10 text-xs text-slate-500">
                  Click <strong>Search Indexers</strong> above to retrieve available releases from your indexers.
                </div>
              )}

              {!loadingReleases && releases.length > 0 && (
                <div className="space-y-2.5 max-h-[50vh] overflow-y-auto pr-1">
                  {releases.map((rel, idx) => {
                    const isGrabbing = grabbingGuid === rel.guid;
                    const isRejected = rel.rejections && rel.rejections.length > 0;
                    const formatScore = rel.customFormatScore || 0;
                    const sizeGb = rel.size ? (rel.size / (1024 * 1024 * 1024)).toFixed(2) : '0';

                    return (
                      <div
                        key={rel.guid || idx}
                        className={`p-3.5 rounded-xl border transition-all flex flex-col md:flex-row md:items-center justify-between gap-3 ${
                          isRejected
                            ? 'bg-slate-950/40 border-slate-800/60 opacity-75'
                            : 'bg-slate-900/90 border-slate-800 hover:border-slate-700'
                        }`}
                      >
                        <div className="space-y-1.5 flex-1 min-w-0">
                          <div className="flex items-center space-x-2 flex-wrap gap-y-1">
                            <span className="text-xs font-bold text-slate-200 font-mono break-all">{rel.title}</span>
                          </div>

                          <div className="flex items-center space-x-3 text-[11px] text-slate-400 flex-wrap gap-y-1">
                            <span className="font-semibold text-purple-400 font-mono">📡 {rel.indexer || 'Indexer'}</span>
                            <span>💾 {sizeGb} GB</span>
                            {rel.seeders !== undefined && (
                              <span className="text-emerald-400 font-medium">🌱 {rel.seeders} seeds / 🔻 {rel.leechers || 0} peers</span>
                            )}
                            {rel.quality?.quality?.name && (
                              <span className="px-1.5 py-0.5 rounded bg-slate-800 text-slate-300 font-mono text-[10px]">
                                {rel.quality.quality.name}
                              </span>
                            )}
                            {formatScore !== 0 && (
                              <span className={`px-1.5 py-0.5 rounded text-[10px] font-bold ${
                                formatScore > 0 ? 'bg-emerald-500/20 text-emerald-300' : 'bg-rose-500/20 text-rose-300'
                              }`}>
                                CF: {formatScore > 0 ? `+${formatScore}` : formatScore}
                              </span>
                            )}
                            {rel.ageMinutes !== undefined && (
                              <span className="text-slate-500">⏳ {Math.round(rel.ageMinutes / 60 / 24)}d ago</span>
                            )}
                          </div>

                          {isRejected && (
                            <div className="text-[10px] text-amber-400/90 flex items-center space-x-1 font-mono">
                              <AlertTriangle className="w-3 h-3 shrink-0" />
                              <span>{rel.rejections.join(' • ')}</span>
                            </div>
                          )}
                        </div>

                        <div className="flex items-center space-x-2 shrink-0">
                          <button
                            type="button"
                            onClick={() => handleGrabRelease(rel)}
                            disabled={isGrabbing}
                            className={`flex items-center space-x-1.5 px-3 py-2 rounded-xl text-xs font-bold transition-all active:scale-95 ${
                              isRejected
                                ? 'bg-slate-800 hover:bg-slate-700 text-slate-300 border border-slate-700'
                                : 'bg-brand-600 hover:bg-brand-500 text-white shadow-lg shadow-brand-600/20'
                            }`}
                          >
                            <Zap className={`w-3.5 h-3.5 ${isGrabbing ? 'animate-bounce text-amber-300' : ''}`} />
                            <span>{isGrabbing ? 'Grabbing...' : 'Grab Release'}</span>
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          )}

          {activeTab === 'payload' && (
            <div className="space-y-2">
              <div className="flex items-center justify-between text-xs text-slate-400">
                <span className="font-semibold">Original Arr Webhook Payload:</span>
                <button
                  onClick={handleCopyPayload}
                  className="flex items-center space-x-1 text-slate-300 hover:text-white bg-slate-800 px-2.5 py-1 rounded-lg border border-slate-700"
                >
                  {copiedPayload ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                  <span>{copiedPayload ? 'Copied' : 'Copy JSON'}</span>
                </button>
              </div>

              <pre className="p-4 bg-slate-950 text-emerald-400 rounded-2xl border border-slate-800 overflow-x-auto text-[11px] font-mono leading-relaxed max-h-[45vh]">
                {(() => {
                  try {
                    return JSON.stringify(JSON.parse(item.payload_json || '{}'), null, 2);
                  } catch {
                    return item.payload_json || '{}';
                  }
                })()}
              </pre>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
