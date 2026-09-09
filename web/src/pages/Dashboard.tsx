// web/src/pages/Dashboard.tsx
import React, { useEffect, useState } from 'react';
import {
  Layers,
  ArrowDownCircle,
  ArrowUpCircle,
  HardDrive,
  Activity,
  Server,
  Zap,
  Tv,
  Film,
  FolderSync,
  RefreshCw,
  CheckCircle2,
  AlertCircle,
  RotateCcw,
  ArrowRight,
} from 'lucide-react';
import { AggregateStats, ArrStats, EventLogRecord, AppConfig, ArrGrabRecord, SystemHealthOverview, EngineStatus } from '../types';
import { fetchEvents, fetchArrStats, triggerArrSyncNow, fetchSettings, fetchPipelineItems, fetchSystemHealth, fetchEngineHealth, fetchBandwidthHistory } from '../services/api';
import { BandwidthChart, BandwidthDataPoint } from '../components/BandwidthChart';
import { HealthPanel } from '../components/HealthPanel';

interface DashboardProps {
  stats: AggregateStats | null;
  // Pushed live over the shared WebSocket connection (see hooks/useTelemetry) — ambient updates
  // for these four replace what used to be a dedicated 5s REST poll of the same data.
  pipelineItems?: ArrGrabRecord[];
  recentEvents?: EventLogRecord[];
  arrStats?: ArrStats | null;
  engines?: EngineStatus[] | null;
  // Multi-tenant zone filter (undefined = "All Zones", the unfiltered default). Scopes both the
  // pipeline/recent-activity feed and the Sonarr/Radarr live stats cards to that zone's own
  // instance (see GET /api/arr/stats?zone=<id>) instead of the legacy global primary.
  zone?: string;
  onNavigate: (route: string) => void;
  onViewDetails?: (compoundId: string) => void;
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${units[i]}`;
}

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return '0 B/s';
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
  const i = Math.floor(Math.log(bytesPerSec) / Math.log(1024));
  return `${(bytesPerSec / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

export const Dashboard: React.FC<DashboardProps> = ({
  stats,
  pipelineItems: pipelineItemsProp,
  recentEvents: recentEventsProp,
  arrStats: arrStatsProp,
  engines: enginesProp,
  zone,
  onNavigate,
  onViewDetails,
}) => {
  const [events, setEvents] = useState<EventLogRecord[]>([]);
  const [pipelineItems, setPipelineItems] = useState<ArrGrabRecord[]>([]);
  const [arrStats, setArrStats] = useState<ArrStats | null>(null);
  const [health, setHealth] = useState<SystemHealthOverview | null>(null);
  const [engines, setEngines] = useState<EngineStatus[] | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);

  // Ambient live updates pushed from the shared WebSocket connection (see App.tsx / useTelemetry).
  // pipelineItems arrives as a system-wide feed (not zone-scoped at the WS level), so it's
  // filtered by the active zone client-side here — same approach the Fetchers/Pipeline views use.
  useEffect(() => { if (recentEventsProp) setEvents(recentEventsProp); }, [recentEventsProp]);
  useEffect(() => {
    if (!pipelineItemsProp) return;
    setPipelineItems(zone ? pipelineItemsProp.filter((i) => i.zone_id === zone) : pipelineItemsProp);
  }, [pipelineItemsProp, zone]);
  // arrStatsProp is pushed over WS as global-only (see backend arr_stats_poller) — only apply it
  // while viewing "All Zones"; a specific zone's stats come from the zone-scoped poll below
  // instead, since there's no per-zone WS push.
  useEffect(() => { if (arrStatsProp && !zone) setArrStats(arrStatsProp); }, [arrStatsProp, zone]);
  useEffect(() => { if (enginesProp) setEngines(enginesProp); }, [enginesProp]);
  const [syncingApp, setSyncingApp] = useState<string | null>(null);
  const [activityFilter, setActivityFilter] = useState<'all' | 'arr' | 'sync' | 'breaker'>('all');
  const [bandwidthHistory, setBandwidthHistory] = useState<BandwidthDataPoint[]>([]);

  // Long-range bandwidth history — 'live' uses the ambient 5-minute in-memory buffer above
  // (bandwidthHistory), any other value fetches the 1-min-averaged 72h SQLite-backed history
  // (see Database::insert_bandwidth_point) on selection and every 60s thereafter.
  const [bandwidthRange, setBandwidthRange] = useState<'live' | 3 | 12 | 24 | 72>('live');
  const [longRangeBandwidth, setLongRangeBandwidth] = useState<BandwidthDataPoint[]>([]);
  useEffect(() => {
    if (bandwidthRange === 'live') return;
    const load = () => fetchBandwidthHistory(bandwidthRange).then(setLongRangeBandwidth).catch(() => {});
    load();
    const interval = setInterval(load, 60000);
    return () => clearInterval(interval);
  }, [bandwidthRange]);

  useEffect(() => {
    if (!stats) return;
    if (stats.bandwidth_history && stats.bandwidth_history.length > 0) {
      setBandwidthHistory(stats.bandwidth_history.slice(-300));
    } else {
      setBandwidthHistory((prev) => {
        const next = [
          ...prev,
          {
            time: Date.now(),
            downloadSpeed: stats.total_download_speed || 0,
            uploadSpeed: stats.total_upload_speed || 0,
          },
        ];
        return next.slice(-300);
      });
    }
  }, [stats]);

  const [loadErrored, setLoadErrored] = useState(false);

  // Manual/on-demand refresh only now — events/pipeline/arr-stats/engines arrive ambiently via
  // the WS props above, so this is for the explicit "refresh" button and right after a manual
  // sync trigger, where a REST round trip beats waiting for the next push. Settings (used on
  // this page only to show the file-sync interval) is fetched once separately below — it barely
  // ever changes and its full payload includes every configured Arr/Plex/notification API key,
  // so it was being fetched needlessly often here.
  const loadLiveData = () => {
    Promise.allSettled([
      fetchEvents({ limit: 40 }).then(setEvents),
      fetchPipelineItems({ limit: 30, zone }).then(setPipelineItems),
      fetchArrStats(zone).then(setArrStats),
      fetchSystemHealth().then(setHealth),
      fetchEngineHealth().then(setEngines),
    ]).then((results) => {
      const anyFailed = results.some((r) => r.status === 'rejected');
      setLoadErrored(anyFailed);
      if (anyFailed) {
        for (const r of results) {
          if (r.status === 'rejected') console.error('Dashboard data load failed:', r.reason);
        }
      }
    });
  };

  // A specific zone's Arr stats have no WS push (see the arrStatsProp effect above), so poll them
  // at the same 30s cadence the backend's arr_stats_poller refreshes its cache on — otherwise a
  // zone's card would only ever update on mount/manual-refresh/zone-switch.
  useEffect(() => {
    if (!zone) return;
    const interval = setInterval(() => {
      fetchArrStats(zone).then(setArrStats).catch(() => {});
    }, 30000);
    return () => clearInterval(interval);
  }, [zone]);

  // Cluster/tracker health (HealthPanel's "Trackers"/"Nodes" tabs) isn't part of the WS channel
  // system yet, so it keeps its own light poll; everything else in loadLiveData above only runs
  // once up front (WS props take over from there) plus on explicit user action.
  useEffect(() => {
    loadLiveData();
    const interval = setInterval(() => {
      fetchSystemHealth().then(setHealth).catch(() => {});
    }, 5000);
    return () => clearInterval(interval);
    // Re-runs (and re-fetches everything) on zone switch — a deliberate user action, so a full
    // refresh here is fine rather than plumbing zone into every individual fetch call above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [zone]);

  useEffect(() => {
    fetchSettings().then(setConfig).catch((e) => console.error('Failed to load settings:', e));
  }, []);

  const handleTriggerSync = async (appType: string) => {
    setSyncingApp(appType);
    try {
      await triggerArrSyncNow(appType);
      loadLiveData();
    } catch (e: any) {
      alert(`Sync failed: ${e.message}`);
    } finally {
      setSyncingApp(null);
    }
  };

  // Filter out noisy raw test notifications
  const cleanEvents = events.filter((ev) => {
    if (ev.event_type === 'notification' && ev.message.includes('Dispatched test notification')) {
      return false;
    }
    return true;
  });

  // When a zone is active, the Arr ecosystem cards below describe that zone's own Sonarr/Radarr
  // instance (per compute_zone_arr_stats on the backend) rather than the legacy global primary —
  // label/online-state/replica display needs to follow suit instead of showing global config.
  const activeZoneConfig = zone ? config?.zones?.find((z) => z.id === zone) ?? null : null;
  const sonarrOnline = zone ? !!activeZoneConfig?.sonarr : !!config?.sonarr.enabled;
  const sonarrLabel = zone
    ? (activeZoneConfig?.sonarr ? (activeZoneConfig.sonarr.name || 'Configured') : 'Not configured in this zone')
    : (config?.sonarr.enabled ? (config?.sonarr.primary?.name || config?.sonarr.master?.name || 'Primary Configured') : 'Disabled');
  const radarrOnline = zone ? !!activeZoneConfig?.radarr : !!config?.radarr.enabled;
  const radarrLabel = zone
    ? (activeZoneConfig?.radarr ? (activeZoneConfig.radarr.name || 'Configured') : 'Not configured in this zone')
    : (config?.radarr.enabled ? (config?.radarr.primary?.name || config?.radarr.master?.name || 'Primary Configured') : 'Disabled');

  return (
    <div className="flex-1 overflow-auto p-8 space-y-8">
      {/* Top Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-slate-100">Conduit Media Command Center</h1>
          <p className="text-sm text-slate-400 mt-1">Multi-node fetchers, Arr ecosystems, staging file queues, and telemetry.</p>
        </div>
        <button
          onClick={loadLiveData}
          className="flex items-center space-x-2 rounded-xl bg-slate-800/80 px-4 py-2 text-xs font-semibold text-slate-300 hover:bg-slate-700 transition-colors"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          <span>Refresh</span>
        </button>
      </div>

      {loadErrored && (
        <div className="flex items-center space-x-2 rounded-xl border border-amber-800/50 bg-amber-950/30 px-4 py-2.5 text-sm text-amber-300">
          <AlertCircle className="h-4 w-4 flex-shrink-0" />
          <span>Some dashboard data failed to load — showing the last known values. Retrying automatically.</span>
        </div>
      )}

      {/* Top 4 Telemetry Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-5">
        {/* Total Fetchers */}
        <div
          onClick={() => onNavigate('/fetchers')}
          className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg relative overflow-hidden cursor-pointer hover:border-slate-700 transition-colors"
        >
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">Total Fetchers</span>
            <div className="rounded-xl bg-brand-500/10 p-2.5 text-brand-400 border border-brand-500/20">
              <Layers className="h-5 w-5" />
            </div>
          </div>
          <div className="mt-4 flex items-baseline justify-between">
            <span className="text-3xl font-bold font-mono text-slate-100">{stats?.total_torrents || 0}</span>
            <span className="text-xs text-brand-400 hover:underline flex items-center space-x-1">
              <span>Manage</span>
              <ArrowRight className="h-3 w-3" />
            </span>
          </div>
          <div className="mt-2 text-xs text-slate-400">
            Across {stats?.connected_nodes || 0} of {stats?.total_nodes || 0} online nodes
          </div>
        </div>

        {/* Download Rate */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg relative overflow-hidden">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">Download Rate</span>
            <div className="rounded-xl bg-emerald-500/10 p-2.5 text-emerald-400 border border-emerald-500/20">
              <ArrowDownCircle className="h-5 w-5" />
            </div>
          </div>
          <div className="mt-4">
            <span className="text-3xl font-bold font-mono text-emerald-400">
              {formatSpeed(stats?.total_download_speed || 0)}
            </span>
          </div>
          <div className="mt-2 text-xs text-slate-400">Aggregated inbound bandwidth</div>
        </div>

        {/* Upload Rate */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg relative overflow-hidden">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">Upload Rate</span>
            <div className="rounded-xl bg-sky-500/10 p-2.5 text-sky-400 border border-sky-500/20">
              <ArrowUpCircle className="h-5 w-5" />
            </div>
          </div>
          <div className="mt-4">
            <span className="text-3xl font-bold font-mono text-sky-400">
              {formatSpeed(stats?.total_upload_speed || 0)}
            </span>
          </div>
          <div className="mt-2 text-xs text-slate-400">Aggregated outbound bandwidth</div>
        </div>

        {/* Total Managed Storage */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg relative overflow-hidden">
          <div className="flex items-center justify-between">
            <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">Managed Media</span>
            <div className="rounded-xl bg-purple-500/10 p-2.5 text-purple-400 border border-purple-500/20">
              <HardDrive className="h-5 w-5" />
            </div>
          </div>
          <div className="mt-4">
            <span className="text-3xl font-bold font-mono text-purple-300">
              {formatBytes(stats?.total_size_bytes || 0)}
            </span>
          </div>
          <div className="mt-2 text-xs text-slate-400">Active fetcher payload size</div>
        </div>
      </div>

      {/* Cluster Bandwidth Real-time Area Chart */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg space-y-3">
        <div className="flex items-center justify-end space-x-1 bg-slate-950/60 p-1 rounded-xl border border-slate-800/60 w-fit ml-auto">
          {([['live', 'Live 5m'], [3, '3h'], [12, '12h'], [24, '24h'], [72, '72h']] as const).map(([val, label]) => (
            <button
              key={String(val)}
              onClick={() => setBandwidthRange(val)}
              className={`px-2.5 py-1 rounded-lg text-[11px] font-semibold transition-all ${
                bandwidthRange === val
                  ? 'bg-brand-500/20 text-brand-300 border border-brand-500/30'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              {label}
            </button>
          ))}
        </div>
        <BandwidthChart
          history={bandwidthRange === 'live' ? bandwidthHistory : longRangeBandwidth}
          height={130}
          timeWindowSecs={bandwidthRange === 'live' ? 300 : bandwidthRange * 3600}
          title="Cluster Bandwidth Live Throughput & History"
          onViewTorrent={onViewDetails}
        />
      </div>

      {/* Cluster & Tracker Health Panel */}
      <HealthPanel
        health={health}
        engines={engines}
        onNavigate={onNavigate}
        onViewTorrent={onViewDetails}
        onFilterTracker={(trackerHost) => {
          onNavigate(`/fetchers?tracker=${encodeURIComponent(trackerHost)}`);
        }}
      />

      {/* Middle Row: Arr Ecosystems & File Sync Queue */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        {/* Sonarr TV Ecosystem */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-6 space-y-4 shadow-md">
          <div className="flex items-center justify-between border-b border-slate-800 pb-3">
            <div className="flex items-center space-x-2.5">
              <div className="rounded-lg bg-sky-500/10 p-2 text-sky-400 border border-sky-500/20">
                <Tv className="h-4 w-4" />
              </div>
              <div>
                <h3 className="font-bold text-sm text-slate-100">Sonarr TV{activeZoneConfig && ` — ${activeZoneConfig.name}`}</h3>
                <span className="text-xs text-slate-400">{sonarrLabel}</span>
              </div>
            </div>
            <span className={`h-2.5 w-2.5 rounded-full ${sonarrOnline ? 'bg-emerald-500' : 'bg-slate-600'}`} />
          </div>

          {arrStats?.sonarr_live ? (
            <div className="space-y-3">
              <div className="grid grid-cols-3 gap-2 text-center text-xs">
                <div className="rounded-xl bg-slate-800/60 p-2.5">
                  <div className="text-[11px] text-slate-400">Library Series</div>
                  <div className="text-base font-bold font-mono text-slate-100 mt-0.5">
                    {arrStats.sonarr_live.series_count}
                  </div>
                  <div className="text-[10px] text-slate-500">{arrStats.sonarr_live.monitored_series_count} monitored</div>
                </div>
                <div className="rounded-xl bg-slate-800/60 p-2.5">
                  <div className="text-[11px] text-slate-400">Missing Ep.</div>
                  <div className={`text-base font-bold font-mono mt-0.5 ${arrStats.sonarr_live.missing_episodes_count > 0 ? 'text-amber-400' : 'text-slate-100'}`}>
                    {arrStats.sonarr_live.missing_episodes_count}
                  </div>
                  <div className="text-[10px] text-slate-500">Wanted queue</div>
                </div>
                <div className="rounded-xl bg-slate-800/60 p-2.5">
                  <div className="text-[11px] text-slate-400">Active Queue</div>
                  <div className={`text-base font-bold font-mono mt-0.5 ${arrStats.sonarr_live.queue_count > 0 ? 'text-sky-400' : 'text-slate-100'}`}>
                    {arrStats.sonarr_live.queue_count}
                  </div>
                  <div className="text-[10px] text-slate-500">Intake grabs</div>
                </div>
              </div>
              <div className="flex items-center justify-between text-xs text-slate-400 px-1">
                <span>Storage: <strong className="text-slate-200">{formatBytes(arrStats.sonarr_live.size_on_disk_bytes)}</strong></span>
                {arrStats.sonarr_live.version && (
                  <span className="font-mono text-[11px] text-slate-500">v{arrStats.sonarr_live.version}</span>
                )}
              </div>
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-3 text-center">
              <div className="rounded-xl bg-slate-800/60 p-3">
                <div className="text-xs text-slate-400">Tracked Grabs</div>
                <div className="text-lg font-bold font-mono text-slate-100 mt-0.5">
                  {arrStats?.sonarr_series_count || 0}
                </div>
              </div>
              <div className="rounded-xl bg-slate-800/60 p-3">
                <div className="text-xs text-slate-400">{zone ? 'Zone' : 'Replicas'}</div>
                <div className="text-lg font-bold font-mono text-slate-100 mt-0.5">
                  {zone ? (activeZoneConfig?.name || zone) : (config?.sonarr.replicas?.length ?? config?.sonarr.slaves?.length ?? 0)}
                </div>
              </div>
            </div>
          )}

          <div className="flex items-center justify-between pt-2 border-t border-slate-800">
            <button
              onClick={() => onNavigate('/settings')}
              className="text-xs text-brand-400 hover:underline"
            >
              {zone ? 'Configure Zone →' : 'Configure Primary & Replicas →'}
            </button>
            <button
              onClick={() => handleTriggerSync('sonarr')}
              disabled={syncingApp === 'sonarr' || !sonarrOnline || !!zone}
              title={zone ? 'Library replication only applies to the global primary/replica setup, not zones' : undefined}
              className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-sky-400 hover:bg-slate-700 disabled:opacity-50"
            >
              <RotateCcw className={`h-3 w-3 ${syncingApp === 'sonarr' ? 'animate-spin' : ''}`} />
              <span>Sync Now</span>
            </button>
          </div>
        </div>

        {/* Radarr Movie Ecosystem */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-6 space-y-4 shadow-md">
          <div className="flex items-center justify-between border-b border-slate-800 pb-3">
            <div className="flex items-center space-x-2.5">
              <div className="rounded-lg bg-amber-500/10 p-2 text-amber-400 border border-amber-500/20">
                <Film className="h-4 w-4" />
              </div>
              <div>
                <h3 className="font-bold text-sm text-slate-100">Radarr Movies{activeZoneConfig && ` — ${activeZoneConfig.name}`}</h3>
                <span className="text-xs text-slate-400">{radarrLabel}</span>
              </div>
            </div>
            <span className={`h-2.5 w-2.5 rounded-full ${radarrOnline ? 'bg-emerald-500' : 'bg-slate-600'}`} />
          </div>

          {arrStats?.radarr_live ? (
            <div className="space-y-3">
              <div className="grid grid-cols-3 gap-2 text-center text-xs">
                <div className="rounded-xl bg-slate-800/60 p-2.5">
                  <div className="text-[11px] text-slate-400">Library Movies</div>
                  <div className="text-base font-bold font-mono text-slate-100 mt-0.5">
                    {arrStats.radarr_live.movie_count}
                  </div>
                  <div className="text-[10px] text-slate-500">{arrStats.radarr_live.monitored_movie_count} monitored</div>
                </div>
                <div className="rounded-xl bg-slate-800/60 p-2.5">
                  <div className="text-[11px] text-slate-400">Missing Mov.</div>
                  <div className={`text-base font-bold font-mono mt-0.5 ${arrStats.radarr_live.missing_movies_count > 0 ? 'text-amber-400' : 'text-slate-100'}`}>
                    {arrStats.radarr_live.missing_movies_count}
                  </div>
                  <div className="text-[10px] text-slate-500">Wanted queue</div>
                </div>
                <div className="rounded-xl bg-slate-800/60 p-2.5">
                  <div className="text-[11px] text-slate-400">Active Queue</div>
                  <div className={`text-base font-bold font-mono mt-0.5 ${arrStats.radarr_live.queue_count > 0 ? 'text-amber-400' : 'text-slate-100'}`}>
                    {arrStats.radarr_live.queue_count}
                  </div>
                  <div className="text-[10px] text-slate-500">Intake grabs</div>
                </div>
              </div>
              <div className="flex items-center justify-between text-xs text-slate-400 px-1">
                <span>Storage: <strong className="text-slate-200">{formatBytes(arrStats.radarr_live.size_on_disk_bytes)}</strong></span>
                {arrStats.radarr_live.version && (
                  <span className="font-mono text-[11px] text-slate-500">v{arrStats.radarr_live.version}</span>
                )}
              </div>
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-3 text-center">
              <div className="rounded-xl bg-slate-800/60 p-3">
                <div className="text-xs text-slate-400">Tracked Grabs</div>
                <div className="text-lg font-bold font-mono text-slate-100 mt-0.5">
                  {arrStats?.radarr_movie_count || 0}
                </div>
              </div>
              <div className="rounded-xl bg-slate-800/60 p-3">
                <div className="text-xs text-slate-400">{zone ? 'Zone' : 'Replicas'}</div>
                <div className="text-lg font-bold font-mono text-slate-100 mt-0.5">
                  {zone ? (activeZoneConfig?.name || zone) : (config?.radarr.replicas?.length ?? config?.radarr.slaves?.length ?? 0)}
                </div>
              </div>
            </div>
          )}

          <div className="flex items-center justify-between pt-2 border-t border-slate-800">
            <button
              onClick={() => onNavigate('/settings')}
              className="text-xs text-brand-400 hover:underline"
            >
              {zone ? 'Configure Zone →' : 'Configure Primary & Replicas →'}
            </button>
            <button
              onClick={() => handleTriggerSync('radarr')}
              disabled={syncingApp === 'radarr' || !radarrOnline || !!zone}
              title={zone ? 'Library replication only applies to the global primary/replica setup, not zones' : undefined}
              className="flex items-center space-x-1 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-amber-400 hover:bg-slate-700 disabled:opacity-50"
            >
              <RotateCcw className={`h-3 w-3 ${syncingApp === 'radarr' ? 'animate-spin' : ''}`} />
              <span>Sync Now</span>
            </button>
          </div>
        </div>

        {/* File Staging & Sync Engine */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-6 space-y-4 shadow-md">
          <div className="flex items-center justify-between border-b border-slate-800 pb-3">
            <div className="flex items-center space-x-2.5">
              <div className="rounded-lg bg-emerald-500/10 p-2 text-emerald-400 border border-emerald-500/20">
                <FolderSync className="h-4 w-4" />
              </div>
              <div>
                <h3 className="font-bold text-sm text-slate-100">File Sync & Staging</h3>
                <span className="text-xs text-slate-400">
                  {config?.file_sync.enabled ? `Interval: ${config.file_sync.interval_secs}s` : 'Disabled'}
                </span>
              </div>
            </div>
            <span className={`h-2.5 w-2.5 rounded-full ${config?.file_sync.enabled ? 'bg-emerald-500' : 'bg-slate-600'}`} />
          </div>

          <div className="space-y-2 text-xs text-slate-300">
            <div className="flex justify-between">
              <span className="text-slate-400">TV Post:</span>
              <span className="font-mono text-slate-200 truncate max-w-[160px]">{config?.file_sync.tv_post || 'Not set'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">Movie Post:</span>
              <span className="font-mono text-slate-200 truncate max-w-[160px]">{config?.file_sync.movie_post || 'Not set'}</span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">Queue Cleanup:</span>
              <span className="font-mono text-slate-200">{config?.file_sync.clean_queue_days || 30} days max</span>
            </div>
          </div>

          <div className="pt-2 border-t border-slate-800">
            <button
              onClick={() => onNavigate('/settings')}
              className="text-xs text-brand-400 hover:underline"
            >
              Configure Staging Paths &rarr;
            </button>
          </div>
        </div>
      </div>

      {/* Nodes Health Cards */}
      <div className="space-y-4">
        <h2 className="text-lg font-bold text-slate-200 flex items-center space-x-2">
          <Server className="h-5 w-5 text-brand-400" />
          <span>Nodes Storage & Telemetry</span>
        </h2>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
          {stats?.nodes.map((node) => (
            <div
              key={node.node}
              className="rounded-2xl border border-slate-800 bg-slate-900/50 p-5 space-y-4 shadow-md"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-2.5">
                  <span className={`h-2.5 w-2.5 rounded-full ${node.connected ? 'bg-emerald-500' : 'bg-rose-500'}`} />
                  <span className="font-bold text-base text-slate-100">{node.node}</span>
                </div>
                <span className="text-xs font-mono text-slate-400">
                  {node.connected ? `${node.latency_ms}ms` : 'Disconnected'}
                </span>
              </div>

              {/* Free Space Gauge */}
              <div>
                <div className="flex items-center justify-between text-xs mb-1.5">
                  <span className="text-slate-400">Free Disk Space</span>
                  <span className="font-bold font-mono text-slate-200">{node.free_space_gb.toFixed(1)} GB free</span>
                </div>
                <div className="w-full bg-slate-800 rounded-full h-2 overflow-hidden">
                  <div
                    className="h-full bg-brand-500 rounded-full"
                    style={{ width: `${Math.min((node.free_space_gb / 500) * 100, 100)}%` }}
                  />
                </div>
              </div>

              {/* Speeds & Torrents */}
              <div className="grid grid-cols-3 gap-2 pt-2 border-t border-slate-800 text-center">
                <div className="rounded-lg bg-slate-800/60 p-2">
                  <div className="text-xs text-slate-400">Fetchers</div>
                  <div className="text-sm font-bold font-mono text-slate-100 mt-0.5">{node.total_torrents}</div>
                </div>
                <div className="rounded-lg bg-slate-800/60 p-2">
                  <div className="text-xs text-emerald-400">Down</div>
                  <div className="text-sm font-bold font-mono text-emerald-400 mt-0.5">
                    {formatSpeed(node.rate_download)}
                  </div>
                </div>
                <div className="rounded-lg bg-slate-800/60 p-2">
                  <div className="text-xs text-sky-400">Up</div>
                  <div className="text-sm font-bold font-mono text-sky-400 mt-0.5">
                    {formatSpeed(node.rate_upload)}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Activity Feed & Ratio Distribution */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Ratio Distribution */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-6 space-y-4">
          <h3 className="text-base font-bold text-slate-200 flex items-center space-x-2">
            <Zap className="h-5 w-5 text-amber-400" />
            <span>Seeding Ratio Distribution</span>
          </h3>

          <div className="space-y-3">
            {stats?.nodes.map((n) => (
              <div key={n.node} className="space-y-1.5">
                <div className="text-xs font-semibold text-slate-300">{n.node}</div>
                <div className="flex h-4 w-full rounded-full overflow-hidden bg-slate-800">
                  <div style={{ width: `${(n.ratio_0 / Math.max(n.total_torrents, 1)) * 100}%` }} className="bg-rose-500" title="Ratio 0" />
                  <div style={{ width: `${(n.ratio_lt1 / Math.max(n.total_torrents, 1)) * 100}%` }} className="bg-amber-500" title="Ratio < 1" />
                  <div style={{ width: `${(n.ratio_gt1 / Math.max(n.total_torrents, 1)) * 100}%` }} className="bg-emerald-500" title="Ratio 1 - 2" />
                  <div style={{ width: `${(n.ratio_gt2 / Math.max(n.total_torrents, 1)) * 100}%` }} className="bg-sky-500" title="Ratio >= 2" />
                </div>
              </div>
            ))}

            <div className="flex items-center justify-between text-xs text-slate-400 pt-2">
              <div className="flex items-center space-x-1.5">
                <span className="h-2.5 w-2.5 rounded-full bg-rose-500" />
                <span>Ratio 0</span>
              </div>
              <div className="flex items-center space-x-1.5">
                <span className="h-2.5 w-2.5 rounded-full bg-amber-500" />
                <span>Ratio &lt; 1</span>
              </div>
              <div className="flex items-center space-x-1.5">
                <span className="h-2.5 w-2.5 rounded-full bg-emerald-500" />
                <span>Ratio 1 - 2</span>
              </div>
              <div className="flex items-center space-x-1.5">
                <span className="h-2.5 w-2.5 rounded-full bg-sky-500" />
                <span>Ratio &gt;= 2</span>
              </div>
            </div>
          </div>
        </div>

        {/* Live Ecosystem Activity Feed */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/50 p-6 space-y-4">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <Activity className="h-5 w-5 text-amber-400" />
              <h3 className="text-base font-bold text-slate-200">🐕 Conduit Pipeline & Activity Feed</h3>
            </div>

            {/* Filter Pills */}
            <div className="flex space-x-1 bg-slate-800/80 p-0.5 rounded-lg text-[11px]">
              {(['all', 'arr', 'sync', 'breaker'] as const).map((f) => (
                <button
                  key={f}
                  onClick={() => setActivityFilter(f)}
                  className={`px-2.5 py-1 rounded-md font-semibold capitalize transition-colors ${
                    activityFilter === f ? 'bg-amber-600 text-white' : 'text-slate-400 hover:text-white'
                  }`}
                >
                  {f === 'arr' ? '🐕 Arr Grabs' : f === 'breaker' ? '⚡ Breakers' : f === 'sync' ? '🎾 Staged' : 'All Trail'}
                </button>
              ))}
            </div>
          </div>

          <div className="space-y-3 divide-y divide-slate-800/60 max-h-96 overflow-y-auto pr-1">
            {/* 1. Pipeline Media Items */}
            {activityFilter !== 'sync' && activityFilter !== 'breaker' && pipelineItems.slice(0, 8).map((item) => {
              const isImported = item.status === 'imported';
              const targetId = item.download_id || `pipeline:${item.id}`;

              return (
                <div
                  key={item.id}
                  onClick={() => onViewDetails?.(targetId)}
                  className="pt-3 first:pt-0 flex items-start space-x-3 cursor-pointer hover:bg-slate-800/30 p-2 rounded-xl transition-colors group"
                >
                  {item.poster_url ? (
                    <div className="w-10 h-14 rounded-lg overflow-hidden bg-slate-950 border border-slate-800 shrink-0 shadow-sm">
                      <img
                        src={item.poster_url}
                        alt={item.title || item.release_title}
                        className="w-full h-full object-cover"
                        referrerPolicy="no-referrer"
                      />
                    </div>
                  ) : (
                    <div className="w-10 h-14 rounded-lg bg-slate-800 border border-slate-700/60 flex items-center justify-center shrink-0 text-slate-400">
                      {item.item_type === 'series' ? <Tv className="h-5 w-5 text-blue-400" /> : <Film className="h-5 w-5 text-indigo-400" />}
                    </div>
                  )}

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center space-x-2 flex-wrap gap-y-1">
                      <span className={`px-2 py-0.5 rounded-full text-[10px] font-bold ${
                        isImported
                          ? 'bg-emerald-500/10 text-emerald-300 border border-emerald-500/20'
                          : 'bg-amber-500/10 text-amber-300 border border-amber-500/20'
                      }`}>
                        {isImported ? '🏆 Retrieved' : '🦴 Sniffed'}
                      </span>
                      <span className="text-[10px] font-bold px-1.5 py-0.5 rounded bg-slate-800 text-slate-300 border border-slate-700">
                        {item.item_type === 'series' ? 'Sonarr' : 'Radarr'}
                      </span>
                      {item.quality && (
                        <span className="text-[10px] font-mono text-slate-400">
                          {item.quality}
                        </span>
                      )}
                      {item.indexer && (
                        <span className="text-[10px] font-mono text-purple-400">
                          📡 {item.indexer}
                        </span>
                      )}
                    </div>

                    <h4 className="text-xs font-bold text-slate-100 group-hover:text-amber-400 transition-colors truncate mt-1">
                      {item.title ? `${item.title} ${item.year ? `(${item.year})` : ''}` : item.release_title}
                    </h4>

                    <div className="flex items-center justify-between text-[11px] text-slate-500 mt-1">
                      <span className="truncate max-w-[200px]" title={item.release_title}>
                        {item.release_title}
                      </span>
                      <span className="font-mono text-slate-400 shrink-0">
                        {new Date(item.created_at).toLocaleTimeString()}
                      </span>
                    </div>
                  </div>
                </div>
              );
            })}

            {/* 2. System & Circuit Breaker Events */}
            {cleanEvents
              .filter((ev) => {
                if (activityFilter === 'arr') return false;
                if (activityFilter === 'breaker') return ev.event_type.includes('breaker') || ev.event_type.includes('tracker');
                if (activityFilter === 'sync') return ev.event_type.includes('sync') || ev.event_type.includes('file');
                return true;
              })
              .slice(0, 10)
              .map((ev) => (
                <div key={ev.id} className="pt-3 first:pt-0 flex items-start space-x-3 p-2">
                  <span className={`mt-1 h-2 w-2 rounded-full shrink-0 ${
                    ev.level === 'warn' || ev.level === 'error' ? 'bg-rose-500' : 'bg-sky-500'
                  }`} />
                  <div className="flex-1 min-w-0">
                    <p className="text-xs text-slate-200 leading-snug">{ev.message}</p>
                    <span className="text-[10px] text-slate-500 font-mono">
                      {new Date(ev.created_at).toLocaleTimeString()} &bull; {ev.event_type}
                    </span>
                  </div>
                </div>
              ))}

            {pipelineItems.length === 0 && cleanEvents.length === 0 && (
              <p className="text-sm text-slate-500 py-4 text-center">No recent pipeline activity records found.</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
