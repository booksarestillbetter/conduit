import React, { useState, useEffect, useCallback } from 'react';
import {
  Sparkles,
  Search,
  RefreshCw,
  Film,
  Tv,
  Music,
  CheckCircle,
  Clock,
  AlertTriangle,
  RotateCw,
  Trash2,
  ChevronRight,
  ExternalLink,
  Code,
  Layers,
  ArrowRight,
  Sliders,
  Database,
  Radio,
} from 'lucide-react';
import { ArrGrabRecord } from '../types';
import { fetchPipelineItems, deletePipelineItem, purgePipelineArchive, reSearchPipelineItem } from '../services/api';
import { getEffectivePoster, getPosterPlaceholder } from '../utils/mediaParser';
import { MediaDetailModal } from '../components/MediaDetailModal';

export interface PipelineBrowserProps {
  onViewTorrent?: (compoundId: string) => void;
  // WS-pushed (see hooks/useTelemetry) — not consumed directly since fetchPipelineItems applies
  // server-side app/status/search filtering this array doesn't reflect, but its arrival is used
  // as a "something changed" signal to trigger a debounced, spinner-free refetch instead of the
  // unconditional 5s poll this used to run.
  pipelineItems?: ArrGrabRecord[];
  // Multi-tenant zone filter (undefined = "All Zones", the unfiltered default).
  zone?: string;
}

export const PipelineBrowser: React.FC<PipelineBrowserProps> = ({ onViewTorrent, pipelineItems: wsPipelineItems, zone }) => {
  const [items, setItems] = useState<ArrGrabRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Filters
  const [appFilter, setAppFilter] = useState<'all' | 'radarr' | 'sonarr' | 'lidarr'>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [activeItemDetails, setActiveItemDetails] = useState<ArrGrabRecord | null>(null);

  // Action status
  const [actionLoadingId, setActionLoadingId] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<{ id: string; text: string; success: boolean } | null>(null);

  const loadData = useCallback(async (opts?: { silent?: boolean }) => {
    try {
      if (!opts?.silent) setLoading(true);
      setError(null);
      const data = await fetchPipelineItems({
        app: appFilter !== 'all' ? appFilter : undefined,
        status: statusFilter !== 'all' ? statusFilter : undefined,
        q: searchQuery.trim() || undefined,
        limit: 150,
        zone,
      });
      setItems(data);
    } catch (err: any) {
      setError(err.message || 'Failed to load pipeline data');
    } finally {
      if (!opts?.silent) setLoading(false);
    }
  }, [appFilter, statusFilter, searchQuery, zone]);

  // Fresh (spinner-visible) fetch whenever filters change or on mount.
  useEffect(() => {
    loadData();
  }, [loadData]);

  // WS-driven refresh: a pipeline push means something changed server-side, so re-fetch
  // (respecting whatever filters are active) — debounced so a burst of pushes (e.g. a season
  // pack importing several episodes at once) triggers one refetch, not one per item. Silent
  // (no loading-spinner flicker) since data is already on screen.
  useEffect(() => {
    if (!wsPipelineItems) return;
    const t = setTimeout(() => loadData({ silent: true }), 500);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wsPipelineItems]);

  // Slow safety net in case the WS is disconnected for a while.
  useEffect(() => {
    const interval = setInterval(() => loadData({ silent: true }), 60000);
    return () => clearInterval(interval);
  }, [loadData]);

  const handleReSearch = async (item: ArrGrabRecord) => {
    try {
      setActionLoadingId(item.id);
      const res = await reSearchPipelineItem(item.id);
      setActionMessage({
        id: item.id,
        text: res.details || 'Re-search command dispatched to Arr stack',
        success: res.executed,
      });
      loadData();
    } catch (err: any) {
      setActionMessage({
        id: item.id,
        text: err.message || 'Failed to trigger re-search',
        success: false,
      });
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleDelete = async (item: ArrGrabRecord) => {
    if (!window.confirm(`Remove '${item.release_title}' from Conduit pipeline tracking?`)) return;
    try {
      setActionLoadingId(item.id);
      await deletePipelineItem(item.id);
      setItems((prev) => prev.filter((i) => i.id !== item.id));
      if (activeItemDetails?.id === item.id) setActiveItemDetails(null);
    } catch (err: any) {
      alert(err.message || 'Failed to delete item');
    } finally {
      setActionLoadingId(null);
    }
  };

  const handlePurgeArchive = async () => {
    const scopeStr = appFilter !== 'all' ? `${appFilter.toUpperCase()} records` : 'ALL pipeline records';
    if (!window.confirm(`Are you sure you want to purge ${scopeStr} from Conduit pipeline history? This cannot be undone.`)) return;
    try {
      setLoading(true);
      const res = await purgePipelineArchive(appFilter !== 'all' ? appFilter : undefined);
      alert(`Successfully purged ${res.purged} records.`);
      loadData();
    } catch (err: any) {
      alert(err.message || 'Failed to purge pipeline archive');
    } finally {
      setLoading(false);
    }
  };

  // Metrics
  const totalCount = items.length;
  const sniffedCount = items.filter((i) => i.status === 'fetched' || i.event_type === 'Grab').length;
  const importedCount = items.filter((i) => i.status === 'imported' || i.event_type === 'Download' || i.event_type === 'AlbumDownload').length;
  const movieCount = items.filter((i) => i.item_type === 'movie').length;
  const seriesCount = items.filter((i) => i.item_type === 'series' || i.item_type === 'tv').length;
  const musicCount = items.filter((i) => i.item_type === 'music').length;

  const formatSize = (bytes?: number) => {
    if (!bytes || bytes <= 0) return 'N/A';
    const gb = bytes / (1024 * 1024 * 1024);
    if (gb >= 1) return `${gb.toFixed(2)} GB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  const getStageStep = (item: ArrGrabRecord) => {
    if (item.status === 'imported' || item.event_type === 'Download' || item.event_type === 'AlbumDownload') return 4;
    if (item.status === 'staged') return 3;
    if (item.status === 'fetched' || item.event_type === 'Grab') return 2;
    return 1;
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 bg-slate-900/60 border border-slate-800/80 p-6 rounded-2xl backdrop-blur-xl shadow-xl">
        <div className="flex items-center space-x-4">
          <div className="w-14 h-14 rounded-2xl bg-gradient-to-tr from-amber-600 via-orange-500 to-amber-400 flex items-center justify-center shadow-lg shadow-amber-500/20 text-3xl">
            🐕
          </div>
          <div>
            <div className="flex items-center space-x-3">
              <h1 className="text-2xl font-bold tracking-tight text-white">
                Conduit's Scent Trail
              </h1>
              <span className="px-2.5 py-0.5 rounded-full text-xs font-semibold bg-amber-500/10 text-amber-400 border border-amber-500/20">
                Pipeline Intake
              </span>
            </div>
            <p className="text-sm text-slate-400 mt-1">
              Live telemetry tracking media sniffed from Arr indexers, fetched across swarm nodes, and staged into libraries.
            </p>
          </div>
        </div>

        <div className="flex items-center space-x-3">
          <button
            onClick={handlePurgeArchive}
            className="flex items-center space-x-2 px-3.5 py-2.5 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 text-rose-300 border border-rose-500/30 text-sm font-medium transition-all shadow-sm active:scale-95"
            title="Purge pipeline history"
          >
            <Trash2 className="w-4 h-4 text-rose-400" />
            <span>Purge Archive</span>
          </button>
          <button
            onClick={() => loadData()}
            className="flex items-center space-x-2 px-4 py-2.5 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-200 border border-slate-700/80 text-sm font-medium transition-all shadow-sm active:scale-95"
          >
            <RefreshCw className={`w-4 h-4 ${loading ? 'animate-spin text-amber-400' : ''}`} />
            <span>Refresh</span>
          </button>
        </div>
      </div>

      {/* Metrics Cards */}
      <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
        <div className="bg-slate-900/50 border border-slate-800/80 rounded-2xl p-4 flex items-center space-x-4">
          <div className="w-12 h-12 rounded-xl bg-amber-500/10 border border-amber-500/20 flex items-center justify-center text-2xl">
            🦴
          </div>
          <div>
            <div className="text-2xl font-bold text-white tracking-tight">{sniffedCount}</div>
            <div className="text-xs text-slate-400 font-medium">Sniffed (Grabbed)</div>
          </div>
        </div>

        <div className="bg-slate-900/50 border border-slate-800/80 rounded-2xl p-4 flex items-center space-x-4">
          <div className="w-12 h-12 rounded-xl bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-2xl">
            🏆
          </div>
          <div>
            <div className="text-2xl font-bold text-white tracking-tight">{importedCount}</div>
            <div className="text-xs text-slate-400 font-medium">Retrieved (Imported)</div>
          </div>
        </div>

        <div className="bg-slate-900/50 border border-slate-800/80 rounded-2xl p-4 flex items-center space-x-4">
          <div className="w-12 h-12 rounded-xl bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center text-2xl">
            🎬
          </div>
          <div>
            <div className="text-2xl font-bold text-white tracking-tight">{movieCount}</div>
            <div className="text-xs text-slate-400 font-medium">Radarr Movies</div>
          </div>
        </div>

        <div className="bg-slate-900/50 border border-slate-800/80 rounded-2xl p-4 flex items-center space-x-4">
          <div className="w-12 h-12 rounded-xl bg-blue-500/10 border border-blue-500/20 flex items-center justify-center text-2xl">
            📺
          </div>
          <div>
            <div className="text-2xl font-bold text-white tracking-tight">{seriesCount}</div>
            <div className="text-xs text-slate-400 font-medium">Sonarr Series</div>
          </div>
        </div>

        <div className="bg-slate-900/50 border border-slate-800/80 rounded-2xl p-4 flex items-center space-x-4">
          <div className="w-12 h-12 rounded-xl bg-emerald-500/10 border border-emerald-500/20 flex items-center justify-center text-2xl">
            🎵
          </div>
          <div>
            <div className="text-2xl font-bold text-white tracking-tight">{musicCount}</div>
            <div className="text-xs text-slate-400 font-medium">Lidarr Music</div>
          </div>
        </div>
      </div>

      {/* Filter and Search Bar */}
      <div className="flex flex-col md:flex-row items-stretch md:items-center justify-between gap-4 bg-slate-900/40 border border-slate-800/80 p-3.5 rounded-2xl">
        {/* App Filter Tabs */}
        <div className="flex items-center space-x-1.5 bg-slate-950/60 p-1 rounded-xl border border-slate-800/60 flex-wrap gap-y-1">
          <button
            onClick={() => setAppFilter('all')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all ${
              appFilter === 'all'
                ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            All Intake ({totalCount})
          </button>
          <button
            onClick={() => setAppFilter('radarr')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center space-x-1.5 transition-all ${
              appFilter === 'radarr'
                ? 'bg-indigo-500/20 text-indigo-300 border border-indigo-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Film className="w-3.5 h-3.5" />
            <span>Radarr Movies</span>
          </button>
          <button
            onClick={() => setAppFilter('sonarr')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center space-x-1.5 transition-all ${
              appFilter === 'sonarr'
                ? 'bg-blue-500/20 text-blue-300 border border-blue-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Tv className="w-3.5 h-3.5" />
            <span>Sonarr TV</span>
          </button>
          <button
            onClick={() => setAppFilter('lidarr')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center space-x-1.5 transition-all ${
              appFilter === 'lidarr'
                ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Music className="w-3.5 h-3.5" />
            <span>Lidarr Music</span>
          </button>
        </div>

        {/* Status Filter Tabs */}
        <div className="flex items-center space-x-1.5 bg-slate-950/60 p-1 rounded-xl border border-slate-800/60">
          {[
            { id: 'all', label: 'All Status' },
            { id: 'fetched', label: '🦴 Sniffed' },
            { id: 'imported', label: '🏆 Retrieved' },
            { id: 're-searched', label: '🔄 Re-searched' },
          ].map((st) => (
            <button
              key={st.id}
              onClick={() => setStatusFilter(st.id)}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-all ${
                statusFilter === st.id
                  ? 'bg-slate-800 text-slate-100 border border-slate-700'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {st.label}
            </button>
          ))}
        </div>

        {/* Search Field */}
        <div className="relative flex-1 max-w-xs">
          <Search className="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-slate-500" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search title, scene, indexer..."
            className="w-full pl-9 pr-4 py-1.5 bg-slate-950/60 border border-slate-800/80 rounded-xl text-xs text-slate-200 placeholder-slate-500 focus:outline-none focus:border-amber-500/50"
          />
        </div>
      </div>

      {/* Items List */}
      {loading && items.length === 0 ? (
        <div className="p-16 text-center text-slate-500 bg-slate-900/30 border border-slate-800/60 rounded-2xl flex flex-col items-center">
          <RefreshCw className="w-8 h-8 animate-spin text-amber-400 mb-3" />
          <span className="text-sm font-medium">Conduit is sniffing out intake history...</span>
        </div>
      ) : items.length === 0 ? (
        <div className="p-16 text-center bg-slate-900/30 border border-slate-800/60 rounded-2xl flex flex-col items-center">
          <div className="text-4xl mb-3">🐕🐾</div>
          <h3 className="text-base font-semibold text-slate-300">No media items found in Conduit's trail</h3>
          <p className="text-xs text-slate-500 mt-1 max-w-sm">
            Make sure your Sonarr, Radarr, or Lidarr webhooks are configured to point to{' '}
            <code className="text-amber-400 bg-slate-950 px-1.5 py-0.5 rounded">/api/sonarr/inbound</code>,{' '}
            <code className="text-amber-400 bg-slate-950 px-1.5 py-0.5 rounded">/api/radarr/inbound</code>, or{' '}
            <code className="text-amber-400 bg-slate-950 px-1.5 py-0.5 rounded">/api/lidarr/inbound</code>.
          </p>
        </div>
      ) : (
        <div className="space-y-3.5">
          {items.map((item) => {
            const step = getStageStep(item);
            const isMovie = item.item_type === 'movie';
            const isMusic = item.item_type === 'music';
            const displayTitle = item.title || item.release_title;
            const isActing = actionLoadingId === item.id;

            return (
              <div
                key={item.id}
                onClick={() => setActiveItemDetails(item)}
                className="bg-slate-900/50 hover:bg-slate-900/90 border border-slate-800/80 hover:border-amber-500/40 rounded-2xl p-4 transition-all duration-200 shadow-md flex flex-col lg:flex-row items-start lg:items-center justify-between gap-4 group cursor-pointer"
              >
                {/* Left: Poster + Core Info */}
                <div className="flex items-start space-x-4 flex-1 min-w-0">
                  {/* Poster Thumbnail */}
                  <div className="w-16 h-24 rounded-xl overflow-hidden bg-slate-950 border border-slate-800 flex-shrink-0 relative shadow-inner">
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
                      className={`absolute top-1 left-1 px-1.5 py-0.5 rounded text-[9px] font-bold uppercase tracking-wider ${
                        isMovie ? 'bg-indigo-500 text-white' : isMusic ? 'bg-emerald-500 text-white' : 'bg-blue-500 text-white'
                      }`}
                    >
                      {isMovie ? 'Movie' : isMusic ? 'Music' : 'TV'}
                    </span>
                  </div>

                  {/* Title & Specs */}
                  <div className="flex-1 min-w-0 space-y-1.5">
                    <div className="flex items-center space-x-2 flex-wrap gap-y-1">
                      <h3 className="text-base font-bold text-white group-hover:text-amber-300 transition-colors tracking-tight truncate max-w-md">
                        {displayTitle}
                      </h3>
                      {item.year && (
                        <span className="text-xs font-semibold text-slate-400">
                          ({item.year})
                        </span>
                      )}
                      {item.quality && (
                        <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-amber-500/10 text-amber-300 border border-amber-500/20">
                          {item.quality}
                        </span>
                      )}
                      {item.indexer && (
                        <span className="px-2 py-0.5 rounded-full text-[10px] font-bold bg-purple-500/10 text-purple-300 border border-purple-500/20">
                          📡 {item.indexer}
                        </span>
                      )}
                    </div>

                    <div className="text-xs text-slate-400 font-mono truncate max-w-xl">
                      {item.release_title}
                    </div>

                    {item.overview && (
                      <p className="text-xs text-slate-400 line-clamp-2 leading-relaxed max-w-2xl font-sans">
                        {item.overview}
                      </p>
                    )}

                    {/* Metadata tags */}
                    <div className="flex items-center space-x-3 text-[11px] text-slate-400 pt-0.5 flex-wrap gap-y-1">
                      <span>💾 {formatSize(item.size_bytes)}</span>
                      {item.genres && <span>🎬 {item.genres}</span>}
                      {item.runtime_mins && <span>⏱️ {item.runtime_mins}m</span>}
                      {item.download_client && (
                        <span>🐾 Node: <strong className="text-slate-300 font-mono">{item.download_client}</strong></span>
                      )}
                      <span className="text-slate-500">
                        • {new Date(item.created_at).toLocaleString()}
                      </span>
                    </div>

                    {actionMessage && actionMessage.id === item.id && (
                      <div
                        className={`text-xs px-2.5 py-1 rounded-lg mt-1 font-medium ${
                          actionMessage.success
                            ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                            : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                        }`}
                      >
                        {actionMessage.text}
                      </div>
                    )}
                  </div>
                </div>

                {/* Middle: Conduit Scent Trail Stepper */}
                <div className="flex items-center space-x-2 bg-slate-950/70 border border-slate-800/80 px-4 py-2.5 rounded-xl flex-shrink-0">
                  <div
                    className={`flex items-center space-x-1.5 text-xs font-semibold ${
                      step >= 1 ? 'text-amber-400' : 'text-slate-600'
                    }`}
                  >
                    <span>🦴</span>
                    <span>Sniffed</span>
                  </div>
                  <ArrowRight className="w-3 h-3 text-slate-700" />
                  <div
                    className={`flex items-center space-x-1.5 text-xs font-semibold ${
                      step >= 2 ? 'text-blue-400' : 'text-slate-600'
                    }`}
                  >
                    <span>🐾</span>
                    <span>Fetching</span>
                  </div>
                  <ArrowRight className="w-3 h-3 text-slate-700" />
                  <div
                    className={`flex items-center space-x-1.5 text-xs font-semibold ${
                      step >= 3 ? 'text-purple-400' : 'text-slate-600'
                    }`}
                  >
                    <span>🎾</span>
                    <span>Staged</span>
                  </div>
                  <ArrowRight className="w-3 h-3 text-slate-700" />
                  <div
                    className={`flex items-center space-x-1.5 text-xs font-semibold ${
                      step >= 4 ? 'text-emerald-400' : 'text-slate-600'
                    }`}
                  >
                    <span>🏆</span>
                    <span>Retrieved</span>
                  </div>
                </div>

                {/* Right: Actions */}
                <div className="flex items-center space-x-2 flex-shrink-0" onClick={(e) => e.stopPropagation()}>
                  <button
                    onClick={() => handleReSearch(item)}
                    disabled={isActing}
                    title="Dispatches automated interactive re-search in Sonarr/Radarr"
                    className="p-2 rounded-xl bg-slate-800/80 hover:bg-slate-700 text-slate-300 hover:text-amber-400 border border-slate-700/80 text-xs font-medium transition-all active:scale-95 disabled:opacity-50"
                  >
                    <RotateCw className={`w-4 h-4 ${isActing ? 'animate-spin' : ''}`} />
                  </button>

                  <button
                    onClick={() => setActiveItemDetails(item)}
                    title="View Media Details & Scent Trail"
                    className="p-2 rounded-xl bg-slate-800/80 hover:bg-slate-700 text-slate-300 hover:text-amber-400 border border-slate-700/80 text-xs font-medium transition-all active:scale-95"
                  >
                    <ChevronRight className="w-4 h-4" />
                  </button>

                  <button
                    onClick={() => handleDelete(item)}
                    disabled={isActing}
                    title="Remove from Conduit tracker"
                    className="p-2 rounded-xl bg-slate-800/80 hover:bg-rose-950/40 text-slate-400 hover:text-rose-400 border border-slate-700/80 hover:border-rose-800/50 text-xs font-medium transition-all active:scale-95 disabled:opacity-50"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Media Detail & Scent Trail Modal */}
      {activeItemDetails && (
        <MediaDetailModal
          item={activeItemDetails}
          onClose={() => setActiveItemDetails(null)}
          onReSearch={handleReSearch}
          onDelete={handleDelete}
          onViewTorrent={onViewTorrent}
        />
      )}
    </div>
  );
};
