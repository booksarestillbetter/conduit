// web/src/pages/Stats.tsx
import React, { useEffect, useState } from 'react';
import {
  Layers,
  ArrowDownCircle,
  ArrowUpCircle,
  HardDrive,
  Users,
  TrendingUp,
  Server,
  Radio,
  Tv,
  Film,
  Music,
  HelpCircle,
  Laptop,
} from 'lucide-react';
import { StatsSnapshot, PeerClientSnapshot, MediaTypeBreakdown } from '../types';
import { fetchCurrentStats, fetchStatsHistory, fetchPeerClientHistory } from '../services/api';
import { StatsTrendChart } from '../components/StatsTrendChart';

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

function formatRelativeTime(msAgo: number): string {
  const secs = msAgo / 1000;
  if (secs < 90) return 'just now';
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  return `${Math.round(mins / 60)}h ago`;
}

const CLIENT_TYPE_LABEL: Record<string, string> = {
  transmission: 'Transmission',
  qbittorrent: 'qBittorrent',
  deluge: 'Deluge',
  synapse: 'Synapse',
};

const MEDIA_TYPE_META: Record<MediaTypeBreakdown['media_type'], { label: string; icon: React.ElementType; color: string }> = {
  tv: { label: 'TV', icon: Tv, color: '#38bdf8' },
  movie: { label: 'Movies', icon: Film, color: '#f59e0b' },
  music: { label: 'Music', icon: Music, color: '#34d399' },
  unknown: { label: 'Unknown', icon: HelpCircle, color: '#64748b' },
};

const RANGE_OPTIONS = [
  { hours: 24, label: '24h' },
  { hours: 24 * 7, label: '7d' },
  { hours: 24 * 30, label: '30d' },
] as const;

/** A simple horizontal-bar breakdown list — used for media type and peer client, where a
 * handful of named buckets each need a count/size shown relative to the largest one. */
function BreakdownBar({ label, icon: Icon, color, value, maxValue, detail }: { label: string; icon: React.ElementType; color: string; value: number; maxValue: number; detail: string }) {
  const pct = maxValue > 0 ? Math.max(2, Math.round((value / maxValue) * 100)) : 0;
  return (
    <div className="space-y-1">
      <div className="flex items-center justify-between text-xs">
        <span className="flex items-center space-x-1.5 text-slate-300 font-medium">
          <Icon className="h-3.5 w-3.5" style={{ color }} />
          <span>{label}</span>
        </span>
        <span className="font-mono text-slate-400">{detail}</span>
      </div>
      <div className="h-1.5 rounded-full bg-slate-800/80 overflow-hidden">
        <div className="h-full rounded-full transition-all" style={{ width: `${pct}%`, backgroundColor: color }} />
      </div>
    </div>
  );
}

export const Stats: React.FC = () => {
  const [current, setCurrent] = useState<StatsSnapshot | null>(null);
  const [rangeHours, setRangeHours] = useState<number>(24);
  const [history, setHistory] = useState<StatsSnapshot[]>([]);
  const [peerHistory, setPeerHistory] = useState<PeerClientSnapshot[]>([]);

  // Live totals + breakdowns — cheap to compute server-side (no extra network calls), so a
  // short poll keeps this feeling current without needing its own WebSocket topic.
  useEffect(() => {
    const load = () => fetchCurrentStats().then(setCurrent).catch((e) => console.error('Failed to load current stats:', e));
    load();
    const interval = setInterval(load, 15000);
    return () => clearInterval(interval);
  }, []);

  useEffect(() => {
    const load = () => fetchStatsHistory(rangeHours).then(setHistory).catch((e) => console.error('Failed to load stats history:', e));
    load();
    const interval = setInterval(load, 60000);
    return () => clearInterval(interval);
  }, [rangeHours]);

  // Sampled every ~15 minutes server-side (see stats::sample_peer_clients) — polling this more
  // often than that would just re-fetch the same rows.
  useEffect(() => {
    const load = () => fetchPeerClientHistory(rangeHours).then(setPeerHistory).catch((e) => console.error('Failed to load peer client history:', e));
    load();
    const interval = setInterval(load, 5 * 60000);
    return () => clearInterval(interval);
  }, [rangeHours]);

  const latestPeerSnapshot = peerHistory.length > 0 ? peerHistory[peerHistory.length - 1] : null;

  const timestamps = history.map((s) => s.time);
  const maxMediaCount = current ? Math.max(1, ...current.by_media_type.map((m) => m.torrents)) : 1;
  const maxPeerCount = latestPeerSnapshot ? Math.max(1, ...latestPeerSnapshot.by_client.map((c) => c.peer_count)) : 1;

  return (
    <div className="space-y-6">
      {/* Totals row */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6 gap-4">
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 shadow-lg">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">Torrents</span>
            <Layers className="h-4 w-4 text-brand-400" />
          </div>
          <div className="mt-2 text-2xl font-bold font-mono text-slate-100">{current?.total_torrents ?? 0}</div>
        </div>
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 shadow-lg">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">Total Size</span>
            <HardDrive className="h-4 w-4 text-purple-400" />
          </div>
          <div className="mt-2 text-2xl font-bold font-mono text-purple-300">{formatBytes(current?.total_size_bytes ?? 0)}</div>
        </div>
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 shadow-lg">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">Seeders</span>
            <TrendingUp className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="mt-2 text-2xl font-bold font-mono text-emerald-400">{current?.total_seeders ?? 0}</div>
        </div>
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 shadow-lg">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">Leechers</span>
            <Users className="h-4 w-4 text-amber-400" />
          </div>
          <div className="mt-2 text-2xl font-bold font-mono text-amber-400">{current?.total_leechers ?? 0}</div>
        </div>
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 shadow-lg">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">Download Rate</span>
            <ArrowDownCircle className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="mt-2 text-2xl font-bold font-mono text-emerald-400">{formatSpeed(current?.total_download_speed ?? 0)}</div>
        </div>
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 shadow-lg">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-slate-400">Upload Rate</span>
            <ArrowUpCircle className="h-4 w-4 text-sky-400" />
          </div>
          <div className="mt-2 text-2xl font-bold font-mono text-sky-400">{formatSpeed(current?.total_upload_speed ?? 0)}</div>
        </div>
      </div>

      {/* Range toggle for the trend charts + peer-client sample below */}
      <div className="flex items-center justify-end space-x-1 bg-slate-950/60 p-1 rounded-xl border border-slate-800/60 w-fit ml-auto">
        {RANGE_OPTIONS.map((opt) => (
          <button
            key={opt.hours}
            onClick={() => setRangeHours(opt.hours)}
            className={`px-2.5 py-1 rounded-lg text-[11px] font-semibold transition-all ${
              rangeHours === opt.hours
                ? 'bg-brand-500/20 text-brand-300 border border-brand-500/30'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            {opt.label}
          </button>
        ))}
      </div>

      {/* Trend charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg">
          {timestamps.length >= 2 ? (
            <StatsTrendChart
              title="Torrents Over Time"
              timestamps={timestamps}
              series={[{ key: 'torrents', label: 'Torrents', color: '#38bdf8', values: history.map((s) => s.total_torrents) }]}
            />
          ) : (
            <EmptyChartPlaceholder label="torrent-count history" />
          )}
        </div>
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg">
          {timestamps.length >= 2 ? (
            <StatsTrendChart
              title="Seeders vs. Leechers Over Time"
              timestamps={timestamps}
              series={[
                { key: 'seeders', label: 'Seeders', color: '#34d399', values: history.map((s) => s.total_seeders) },
                { key: 'leechers', label: 'Leechers', color: '#f59e0b', values: history.map((s) => s.total_leechers) },
              ]}
            />
          ) : (
            <EmptyChartPlaceholder label="seeder/leecher history" />
          )}
        </div>
      </div>

      {/* By Fetcher Node */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg">
        <div className="flex items-center space-x-2 mb-3">
          <Server className="h-4 w-4 text-brand-400" />
          <span className="text-sm font-bold text-slate-200 uppercase tracking-wider">By Fetcher Node</span>
        </div>
        {current && current.by_node.length > 0 ? (
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead>
                <tr className="text-left text-slate-500 uppercase tracking-wider text-[10px] border-b border-slate-800">
                  <th className="pb-2 pr-3">Node</th>
                  <th className="pb-2 pr-3">Client</th>
                  <th className="pb-2 pr-3 text-right">Torrents</th>
                  <th className="pb-2 pr-3 text-right">Size</th>
                  <th className="pb-2 pr-3 text-right">Download</th>
                  <th className="pb-2 text-right">Upload</th>
                </tr>
              </thead>
              <tbody>
                {current.by_node.map((n) => (
                  <tr key={n.node} className="border-b border-slate-800/60 text-slate-300">
                    <td className="py-2 pr-3 flex items-center space-x-2">
                      <span className={`h-2 w-2 rounded-full shrink-0 ${n.connected ? 'bg-emerald-500' : 'bg-rose-500'}`} />
                      <span className="truncate font-medium">{n.node}</span>
                    </td>
                    <td className="py-2 pr-3 text-slate-400">{CLIENT_TYPE_LABEL[n.client_type] ?? n.client_type}</td>
                    <td className="py-2 pr-3 text-right font-mono">{n.torrents}</td>
                    <td className="py-2 pr-3 text-right font-mono text-purple-300">{formatBytes(n.size_bytes)}</td>
                    <td className="py-2 pr-3 text-right font-mono text-emerald-400">{formatSpeed(n.rate_download)}</td>
                    <td className="py-2 text-right font-mono text-sky-400">{formatSpeed(n.rate_upload)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <p className="text-xs text-slate-500 italic">No fetcher nodes configured yet.</p>
        )}
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* By Tracker */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg">
          <div className="flex items-center space-x-2 mb-3">
            <Radio className="h-4 w-4 text-emerald-400" />
            <span className="text-sm font-bold text-slate-200 uppercase tracking-wider">By Tracker</span>
          </div>
          {current && current.by_tracker.length > 0 ? (
            <div className="max-h-72 overflow-y-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr className="text-left text-slate-500 uppercase tracking-wider text-[10px] border-b border-slate-800">
                    <th className="pb-2 pr-3">Host</th>
                    <th className="pb-2 pr-3 text-right">Torrents</th>
                    <th className="pb-2 pr-3 text-right">Seeders</th>
                    <th className="pb-2 text-right">Leechers</th>
                  </tr>
                </thead>
                <tbody>
                  {current.by_tracker.map((t) => (
                    <tr key={t.host} className="border-b border-slate-800/60 text-slate-300">
                      <td className="py-2 pr-3 truncate max-w-[200px]" title={t.host}>{t.host}</td>
                      <td className="py-2 pr-3 text-right font-mono">{t.torrents}</td>
                      <td className="py-2 pr-3 text-right font-mono text-emerald-400">{t.seeders}</td>
                      <td className="py-2 text-right font-mono text-amber-400">{t.leechers}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <p className="text-xs text-slate-500 italic">No tracker activity yet.</p>
          )}
        </div>

        {/* By Media Type */}
        <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg">
          <div className="flex items-center space-x-2 mb-3">
            <Film className="h-4 w-4 text-amber-400" />
            <span className="text-sm font-bold text-slate-200 uppercase tracking-wider">By Media Type</span>
          </div>
          {current && current.by_media_type.length > 0 ? (
            <div className="space-y-3">
              {current.by_media_type.map((m) => {
                const meta = MEDIA_TYPE_META[m.media_type] ?? MEDIA_TYPE_META.unknown;
                return (
                  <BreakdownBar
                    key={m.media_type}
                    label={meta.label}
                    icon={meta.icon}
                    color={meta.color}
                    value={m.torrents}
                    maxValue={maxMediaCount}
                    detail={`${m.torrents} · ${formatBytes(m.size_bytes)}`}
                  />
                );
              })}
              <p className="text-[10px] text-slate-500 italic pt-1">
                "Unknown" covers torrents not matched to a Sonarr/Radarr/Lidarr grab (e.g. added manually).
              </p>
            </div>
          ) : (
            <p className="text-xs text-slate-500 italic">No torrents yet.</p>
          )}
        </div>
      </div>

      {/* By Remote Peer Client */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-5 shadow-lg">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center space-x-2">
            <Laptop className="h-4 w-4 text-sky-400" />
            <span className="text-sm font-bold text-slate-200 uppercase tracking-wider">By Remote Peer Client</span>
          </div>
          {latestPeerSnapshot && (
            <span className="text-[10px] text-slate-500 font-mono">sampled {formatRelativeTime(Date.now() - latestPeerSnapshot.time)}</span>
          )}
        </div>
        {latestPeerSnapshot && latestPeerSnapshot.by_client.length > 0 ? (
          <div className="space-y-3">
            {latestPeerSnapshot.by_client.slice(0, 12).map((c) => (
              <BreakdownBar
                key={c.client_name}
                label={c.client_name}
                icon={Laptop}
                color="#0ea5e9"
                value={c.peer_count}
                maxValue={maxPeerCount}
                detail={`${c.peer_count} peers · ${c.torrents_seen_on} torrents`}
              />
            ))}
          </div>
        ) : (
          <p className="text-xs text-slate-500 italic">No peer samples yet — this is sampled from the most active torrents roughly every 15 minutes.</p>
        )}
        <p className="text-[10px] text-slate-500 italic pt-3">
          Sampled periodically from the most active torrents on each node, not every peer on every torrent — see the Stats API docs for why.
        </p>
      </div>
    </div>
  );
};

function EmptyChartPlaceholder({ label }: { label: string }) {
  return (
    <div className="flex items-center justify-center h-40 text-xs text-slate-500 italic">
      Not enough {label} yet — check back in a few minutes.
    </div>
  );
}
