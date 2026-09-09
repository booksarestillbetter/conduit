import React, { useState } from 'react';
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  AlertOctagon,
  Radio,
  Server,
  Zap,
  Pause,
  ArrowRight,
  RefreshCw,
  ExternalLink,
  ShieldCheck,
  HardDrive,
  Copy,
  Check,
  Cpu,
  X,
  Cog,
} from 'lucide-react';
import { SystemHealthOverview, EngineStatus } from '../types';
import { batchReplaceTrackers, forceTripCircuitBreaker, forceResetCircuitBreaker } from '../services/api';

interface HealthPanelProps {
  health: SystemHealthOverview | null;
  engines?: EngineStatus[] | null;
  onNavigate?: (route: string) => void;
  onViewTorrent?: (compoundId: string) => void;
  onFilterTracker?: (trackerHost: string) => void;
}

function timeAgo(iso: string | null): string {
  if (!iso) return 'never';
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(iso).getTime()) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  return `${Math.floor(seconds / 3600)}h ago`;
}

export const HealthPanel: React.FC<HealthPanelProps> = ({
  health,
  engines,
  onNavigate,
  onViewTorrent,
  onFilterTracker,
}) => {
  const [activeTab, setActiveTab] = useState<'trackers' | 'nodes' | 'metrics' | 'engines'>('trackers');
  const [trackerFilter, setTrackerFilter] = useState<'all' | 'broken' | 'warnings'>('all');
  const [copiedMetricsUrl, setCopiedMetricsUrl] = useState(false);
  const [isReplaceModalOpen, setIsReplaceModalOpen] = useState(false);
  const [replaceOldUrl, setReplaceOldUrl] = useState('');
  const [replaceNewUrl, setReplaceNewUrl] = useState('');
  const [replaceTargetNode, setReplaceTargetNode] = useState('all');
  const [replacing, setReplacing] = useState(false);
  const [breakerActionHost, setBreakerActionHost] = useState<string | null>(null);

  const handleForceTrip = async (host: string) => {
    if (!window.confirm(`Force-trip the circuit breaker for '${host}'? This pauses its active torrents immediately.`)) return;
    setBreakerActionHost(host);
    try {
      await forceTripCircuitBreaker(host);
    } catch {
      // Errors surface via the next health poll reflecting no change; nothing more to do here.
    } finally {
      setBreakerActionHost(null);
    }
  };

  const handleForceReset = async (host: string) => {
    if (!window.confirm(`Force-reset the circuit breaker for '${host}'? This resumes any torrents it paused.`)) return;
    setBreakerActionHost(host);
    try {
      await forceResetCircuitBreaker(host);
    } catch {
      // Errors surface via the next health poll reflecting no change; nothing more to do here.
    } finally {
      setBreakerActionHost(null);
    }
  };

  if (!health) {
    return (
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 flex items-center justify-center space-x-3 text-slate-400">
        <RefreshCw className="h-4 w-4 animate-spin text-brand-400" />
        <span className="text-xs">Loading cluster & tracker swarm telemetry...</span>
      </div>
    );
  }

  const brokenTrackers = health.trackers.filter((t) => t.is_circuit_broken);
  const issueTrackers = health.trackers.filter((t) => t.is_circuit_broken || (t.error_torrents && t.error_torrents > 0) || t.status !== 'healthy');

  const filteredTrackers = health.trackers.filter((t) => {
    if (trackerFilter === 'broken') return t.is_circuit_broken;
    if (trackerFilter === 'warnings') return t.is_circuit_broken || (t.error_torrents && t.error_torrents > 0) || t.status !== 'healthy';
    return true;
  });

  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-6 space-y-6 shadow-xl relative overflow-hidden">
      {/* Background ambient glow if circuit broken */}
      {brokenTrackers.length > 0 && (
        <div className="absolute top-0 right-0 w-96 h-96 bg-amber-500/5 rounded-full blur-3xl pointer-events-none" />
      )}

      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4 border-b border-slate-800/80 pb-4">
        <div className="flex items-center space-x-3">
          <div
            className={`rounded-xl p-2.5 border ${
              health.circuit_broken_trackers > 0
                ? 'bg-amber-500/10 text-amber-400 border-amber-500/20'
                : health.overall_status === 'degraded'
                ? 'bg-rose-500/10 text-rose-400 border-rose-500/20'
                : 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
            }`}
          >
            {health.circuit_broken_trackers > 0 ? (
              <Zap className="h-5 w-5" />
            ) : health.overall_status === 'degraded' ? (
              <AlertOctagon className="h-5 w-5" />
            ) : (
              <ShieldCheck className="h-5 w-5" />
            )}
          </div>
          <div>
            <div className="flex items-center space-x-2.5">
              <h2 className="text-base font-bold text-slate-100">Swarm & Client Health Panel</h2>
              <span
                className={`inline-flex items-center space-x-1 rounded-full px-2.5 py-0.5 text-[10px] font-extrabold uppercase tracking-wider ${
                  health.circuit_broken_trackers > 0
                    ? 'bg-amber-500/10 text-amber-400 border border-amber-500/30'
                    : health.overall_status === 'degraded'
                    ? 'bg-rose-500/10 text-rose-400 border border-rose-500/30'
                    : 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/30'
                }`}
              >
                <span
                  className={`h-1.5 w-1.5 rounded-full ${
                    health.circuit_broken_trackers > 0
                      ? 'bg-amber-400 animate-pulse'
                      : health.overall_status === 'degraded'
                      ? 'bg-rose-400'
                      : 'bg-emerald-400'
                  }`}
                />
                <span>
                  {health.circuit_broken_trackers > 0
                    ? `${health.circuit_broken_trackers} Pressure Relieved`
                    : health.overall_status.toUpperCase()}
                </span>
              </span>
            </div>
            <p className="text-xs text-slate-400 mt-0.5">
              Real-time tracker health, automated circuit breakers, active canary probes, and daemon latency.
            </p>
          </div>
        </div>

        {/* Tab Controls */}
        <div className="flex items-center space-x-2 bg-slate-950/60 p-1 rounded-xl border border-slate-800">
          <button
            onClick={() => setActiveTab('trackers')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors flex items-center space-x-1.5 ${
              activeTab === 'trackers'
                ? 'bg-slate-800 text-slate-100 shadow-sm'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Radio className="h-3.5 w-3.5" />
            <span>Trackers ({health.total_trackers})</span>
            {brokenTrackers.length > 0 && (
              <span className="ml-1 rounded-full bg-amber-500/20 px-1.5 py-0.2 text-[10px] font-bold text-amber-300">
                {brokenTrackers.length}
              </span>
            )}
          </button>

          <button
            onClick={() => setActiveTab('nodes')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors flex items-center space-x-1.5 ${
              activeTab === 'nodes'
                ? 'bg-slate-800 text-slate-100 shadow-sm'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Server className="h-3.5 w-3.5" />
            <span>Fetcher Daemons ({health.connected_nodes}/{health.total_nodes})</span>
          </button>

          <button
            onClick={() => setActiveTab('metrics')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors flex items-center space-x-1.5 ${
              activeTab === 'metrics'
                ? 'bg-slate-800 text-slate-100 shadow-sm'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Activity className="h-3.5 w-3.5 text-amber-400" />
            <span>Prometheus Metrics</span>
          </button>

          <button
            onClick={() => setActiveTab('engines')}
            className={`px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors flex items-center space-x-1.5 ${
              activeTab === 'engines'
                ? 'bg-slate-800 text-slate-100 shadow-sm'
                : 'text-slate-400 hover:text-slate-200'
            }`}
          >
            <Cog className="h-3.5 w-3.5 text-indigo-400" />
            <span>Platform Health{engines ? ` (${engines.length})` : ''}</span>
            {engines && engines.some((e) => e.state === 'crashed' || e.state === 'degraded') && (
              <span className="ml-1 rounded-full bg-amber-500/20 px-1.5 py-0.2 text-[10px] font-bold text-amber-300">
                {engines.filter((e) => e.state === 'crashed' || e.state === 'degraded').length}
              </span>
            )}
          </button>
        </div>
      </div>

      {/* TRACKERS TAB */}
      {activeTab === 'trackers' && (
        <div className="space-y-4">
          {/* Sub-filter pills */}
          <div className="flex items-center justify-between text-xs">
            <div className="flex items-center space-x-2">
              <span className="text-slate-500 font-semibold uppercase tracking-wider text-[10px]">Filter:</span>
              <button
                onClick={() => setTrackerFilter('all')}
                className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors ${
                  trackerFilter === 'all'
                    ? 'bg-brand-500/20 text-brand-300 border border-brand-500/30'
                    : 'bg-slate-800/60 text-slate-400 hover:text-slate-300'
                }`}
              >
                All ({health.total_trackers})
              </button>
              <button
                onClick={() => setTrackerFilter('broken')}
                className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors ${
                  trackerFilter === 'broken'
                    ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                    : 'bg-slate-800/60 text-slate-400 hover:text-slate-300'
                }`}
              >
                ⚡ Circuit Broken ({brokenTrackers.length})
              </button>
              <button
                onClick={() => setTrackerFilter('warnings')}
                className={`px-2.5 py-1 rounded-lg text-xs font-medium transition-colors ${
                  trackerFilter === 'warnings'
                    ? 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                    : 'bg-slate-800/60 text-slate-400 hover:text-slate-300'
                }`}
              >
                ⚠️ Announce Issues ({issueTrackers.length})
              </button>
            </div>

            <div className="flex items-center space-x-3">
              <button
                onClick={() => setIsReplaceModalOpen(true)}
                className="px-3 py-1 rounded-lg text-xs font-semibold bg-indigo-600/20 hover:bg-indigo-600/30 text-indigo-300 border border-indigo-500/30 transition-colors flex items-center space-x-1.5 cursor-pointer shadow-xs"
                title="Search and replace tracker announce URLs across torrents"
              >
                <RefreshCw className="h-3.5 w-3.5" />
                <span>Replace Trackers</span>
              </button>

              <span className="text-slate-500 text-[11px]">
                Active across {health.total_torrents} swarms
              </span>
            </div>
          </div>

          {/* Trackers List Grid */}
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            {filteredTrackers.map((tr) => {
              const isBroken = tr.is_circuit_broken;
              const total = tr.total_torrents;
              const errors = tr.error_torrents ?? (tr.status === 'healthy' ? 0 : (isBroken ? tr.paused_torrents : 1));
              const online = tr.online_torrents ?? Math.max(0, total - errors);
              const ratio = tr.error_ratio ?? (total > 0 ? errors / total : 0);
              const tier = tr.health_tier || (isBroken ? 'circuit_broken' : (errors === 0 ? 'healthy' : (ratio < 0.15 ? 'nominal' : (ratio < 0.5 ? 'warning' : 'critical'))));

              return (
                <div
                  key={tr.host}
                  className={`rounded-xl border p-4.5 space-y-3 transition-all ${
                    isBroken
                      ? 'border-purple-500/40 bg-purple-950/20 shadow-md'
                      : tier === 'critical'
                      ? 'border-rose-500/40 bg-rose-950/20'
                      : tier === 'warning'
                      ? 'border-amber-500/30 bg-amber-950/10'
                      : tier === 'nominal'
                      ? 'border-yellow-500/25 bg-yellow-950/10'
                      : 'border-slate-800/80 bg-slate-950/40 hover:border-slate-700'
                  }`}
                >
                  {/* Row 1: Tracker Host & Multi-tier Status Badge */}
                  <div className="flex items-start justify-between gap-2">
                    <div className="space-y-0.5">
                      <div className="flex items-center space-x-2">
                        <span className="font-bold text-sm text-slate-100 font-mono">{tr.host}</span>
                        <span
                          className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-wider border ${
                            tr.breaker_mode === 'passive'
                              ? 'bg-sky-500/15 text-sky-300 border-sky-500/30'
                              : 'bg-slate-700/40 text-slate-400 border-slate-600/40'
                          }`}
                          title={tr.breaker_mode === 'passive' ? "This node's own daemon manages its breaker natively" : 'Conduit manages this breaker externally'}
                        >
                          {tr.breaker_mode === 'passive' ? 'Native' : 'Conduit'}
                        </span>
                        {isBroken && (
                          <span className="inline-flex items-center space-x-1 rounded-md bg-purple-500/20 px-2 py-0.5 text-[10px] font-bold text-purple-300 border border-purple-500/30">
                            <Zap className="h-3 w-3" />
                            <span>{tr.cb_state === 'recovering' ? 'RECOVERING' : 'CIRCUIT BROKEN'}</span>
                          </span>
                        )}
                      </div>
                      <div className="flex items-center space-x-2 text-[11px] text-slate-400">
                        <span>{tr.total_torrents} torrents</span>
                        <span>•</span>
                        <span>Nodes: {tr.affected_nodes.join(', ') || 'cluster'}</span>
                      </div>
                    </div>

                    <div className="text-right">
                      {isBroken ? (
                        <div className="inline-flex items-center space-x-1 text-xs font-bold text-purple-300 bg-purple-500/20 px-2.5 py-1 rounded-lg border border-purple-500/30">
                          <Pause className="h-3 w-3" />
                          <span>{tr.paused_torrents} Paused</span>
                        </div>
                      ) : tier === 'critical' ? (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-rose-400 bg-rose-500/15 px-2.5 py-1 rounded-lg border border-rose-500/30">
                          <AlertTriangle className="h-3 w-3" />
                          <span>Critical ({errors} err • {(ratio * 100).toFixed(0)}%)</span>
                        </span>
                      ) : tier === 'warning' ? (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-amber-400 bg-amber-500/15 px-2.5 py-1 rounded-lg border border-amber-500/30">
                          <AlertTriangle className="h-3 w-3" />
                          <span>Degraded ({errors} err • {(ratio * 100).toFixed(0)}%)</span>
                        </span>
                      ) : tier === 'nominal' ? (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-yellow-300 bg-yellow-500/15 px-2.5 py-1 rounded-lg border border-yellow-500/30">
                          <CheckCircle2 className="h-3 w-3 text-yellow-400" />
                          <span>Nominal ({errors} err • {((1 - ratio) * 100).toFixed(0)}% OK)</span>
                        </span>
                      ) : (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-emerald-400 bg-emerald-500/10 px-2.5 py-1 rounded-lg border border-emerald-500/20">
                          <CheckCircle2 className="h-3 w-3" />
                          <span>100% Online</span>
                        </span>
                      )}
                    </div>
                  </div>

                  {/* Swarm Health Ratio Meter */}
                  <div className="space-y-1 bg-slate-900/60 p-2.5 rounded-lg border border-slate-800/80">
                    <div className="flex items-center justify-between text-[11px]">
                      <span className="text-slate-300 font-medium">
                        Swarm Health: <span className="font-semibold text-emerald-400">{online}</span> / <span className="text-slate-200">{total}</span> Online
                        {errors > 0 && (
                          <span className="text-rose-400 ml-1.5 font-mono text-[10px]">
                            ({errors} failing • {(ratio * 100).toFixed(1)}%)
                          </span>
                        )}
                      </span>
                      <span className="text-[10px] text-slate-500">
                        {isBroken ? '⚡ Canary probe active' : ratio >= 0.5 ? '⚠️ Breaker threshold reached' : 'Normal ratio (<50%)'}
                      </span>
                    </div>
                    <div className="w-full h-2 bg-slate-800/90 rounded-full overflow-hidden flex">
                      <div
                        style={{ width: `${total > 0 ? (online / total) * 100 : 100}%` }}
                        className={`h-full transition-all ${
                          isBroken
                            ? 'bg-purple-500'
                            : tier === 'critical'
                            ? 'bg-amber-500'
                            : tier === 'warning'
                            ? 'bg-amber-400'
                            : tier === 'nominal'
                            ? 'bg-yellow-400'
                            : 'bg-emerald-500'
                        }`}
                      />
                      {errors > 0 && (
                        <div
                          style={{ width: `${total > 0 ? (errors / total) * 100 : 0}%` }}
                          className={`h-full ${tier === 'critical' ? 'bg-rose-500' : tier === 'warning' ? 'bg-amber-500' : 'bg-rose-400/80'}`}
                        />
                      )}
                    </div>
                  </div>

                  {/* Active Canary Probe Box (if circuit broken) */}
                  {isBroken && tr.active_probe_id && (
                    <div className="rounded-lg bg-slate-900/90 border border-purple-500/30 p-3 space-y-1.5">
                      <div className="flex items-center justify-between text-[11px]">
                        <span className="font-bold text-purple-300 uppercase tracking-wider flex items-center space-x-1.5">
                          <Radio className="h-3 w-3 animate-pulse text-purple-400" />
                          <span>Active Canary Probe</span>
                        </span>
                        <span className="text-[10px] text-slate-400 font-mono">
                          ID: {tr.active_probe_id}
                        </span>
                      </div>
                      <div className="text-xs text-slate-200 font-medium truncate">
                        {tr.active_probe_name || tr.active_probe_id}
                      </div>
                      {onViewTorrent && (
                        <button
                          onClick={() => onViewTorrent(tr.active_probe_id!)}
                          className="text-[11px] text-brand-400 hover:text-brand-300 flex items-center space-x-1 mt-1 font-semibold"
                        >
                          <span>Inspect Probe Fetcher</span>
                          <ExternalLink className="h-2.5 w-2.5" />
                        </button>
                      )}
                    </div>
                  )}

                  {/* Recovery Ramp-Up Progress */}
                  {tr.cb_state === 'recovering' && tr.recovery_progress_pct != null && (
                    <div className="space-y-1 bg-slate-900/60 p-2.5 rounded-lg border border-sky-500/20">
                      <div className="flex items-center justify-between text-[11px]">
                        <span className="text-sky-300 font-medium">
                          Ramping traffic back up{tr.consecutive_successes != null ? ` • ${tr.consecutive_successes} consecutive successes` : ''}
                        </span>
                        <span className="text-[10px] text-slate-500 font-mono">{tr.recovery_progress_pct.toFixed(0)}%</span>
                      </div>
                      <div className="w-full h-2 bg-slate-800/90 rounded-full overflow-hidden">
                        <div
                          style={{ width: `${Math.min(100, Math.max(0, tr.recovery_progress_pct))}%` }}
                          className="h-full bg-sky-400 transition-all"
                        />
                      </div>
                    </div>
                  )}

                  {/* Current Tracker Announce Response */}
                  <div className="rounded-lg bg-slate-900/70 border border-slate-800 p-2.5 text-xs font-mono space-y-1">
                    <div className="text-[10px] uppercase font-sans tracking-wider text-slate-500 font-semibold">
                      Latest Announce Response
                    </div>
                    <div
                      className={`truncate ${
                        isBroken
                          ? 'text-purple-300'
                          : tier === 'critical' || tier === 'warning'
                          ? 'text-amber-300'
                          : tier === 'nominal'
                          ? 'text-yellow-200/90'
                          : 'text-slate-300'
                      }`}
                      title={tr.last_announce_result}
                    >
                      {tr.last_announce_result || (tr.last_announce_succeeded ? 'Success / OK' : 'No response')}
                    </div>
                  </div>

                  {/* Quick Action Footer */}
                  <div className="flex items-center justify-between pt-1">
                    <div className="flex items-center space-x-3">
                      {isBroken ? (
                        <button
                          onClick={() => handleForceReset(tr.host)}
                          disabled={breakerActionHost === tr.host}
                          className="text-[11px] text-emerald-400 hover:text-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed flex items-center space-x-1 font-medium"
                        >
                          <span>Force Reset</span>
                        </button>
                      ) : (
                        <button
                          onClick={() => handleForceTrip(tr.host)}
                          disabled={breakerActionHost === tr.host}
                          className="text-[11px] text-rose-400 hover:text-rose-300 disabled:opacity-50 disabled:cursor-not-allowed flex items-center space-x-1 font-medium"
                        >
                          <span>Force Trip</span>
                        </button>
                      )}
                    </div>
                    {onFilterTracker && (
                      <button
                        onClick={() => onFilterTracker(tr.host)}
                        className="text-[11px] text-slate-400 hover:text-slate-200 flex items-center space-x-1 font-medium"
                      >
                        <span>View {tr.total_torrents} Swarms</span>
                        <ArrowRight className="h-3 w-3" />
                      </button>
                    )}
                  </div>
                </div>
              );
            })}

            {filteredTrackers.length === 0 && (
              <div className="col-span-2 py-8 text-center text-slate-500 text-xs">
                No trackers match the selected filter.
              </div>
            )}
          </div>
        </div>
      )}

      {/* FETCHER NODES TAB */}
      {activeTab === 'nodes' && (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {health.nodes.map((node) => (
            <div
              key={node.name}
              className={`rounded-xl border p-5 space-y-4 ${
                !node.connected
                  ? 'border-rose-500/40 bg-rose-950/20'
                  : node.status === 'degraded'
                  ? 'border-amber-500/30 bg-amber-950/10'
                  : 'border-slate-800 bg-slate-950/40'
              }`}
            >
              {/* Header */}
              <div className="flex items-center justify-between border-b border-slate-800 pb-3">
                <div className="flex items-center space-x-2.5">
                  <div
                    className={`rounded-lg p-2 ${
                      node.connected
                        ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                        : 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                    }`}
                  >
                    <Server className="h-4 w-4" />
                  </div>
                  <div>
                    <h3 className="font-bold text-sm text-slate-100">{node.name}</h3>
                    <span className="text-xs text-slate-400">
                      {node.connected ? `${node.latency_ms}ms response latency` : 'Disconnected'}
                    </span>
                  </div>
                </div>

                <span
                  className={`inline-flex items-center space-x-1 rounded-full px-2.5 py-0.5 text-[10px] font-bold ${
                    node.connected
                      ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/30'
                      : 'bg-rose-500/10 text-rose-400 border border-rose-500/30'
                  }`}
                >
                  <span
                    className={`h-1.5 w-1.5 rounded-full ${
                      node.connected ? 'bg-emerald-400' : 'bg-rose-400'
                    }`}
                  />
                  <span>{node.connected ? 'ONLINE' : 'OFFLINE'}</span>
                </span>
              </div>

              {/* Stats Grid */}
              <div className="grid grid-cols-4 gap-2 text-center text-xs">
                <div className="rounded-lg bg-slate-900/80 p-2 border border-slate-800/80">
                  <div className="text-[10px] text-slate-400">Total</div>
                  <div className="text-sm font-bold font-mono text-slate-100 mt-0.5">
                    {node.total_torrents}
                  </div>
                </div>

                <div className="rounded-lg bg-slate-900/80 p-2 border border-slate-800/80">
                  <div className="text-[10px] text-slate-400">Active</div>
                  <div className="text-sm font-bold font-mono text-emerald-400 mt-0.5">
                    {node.active_torrents}
                  </div>
                </div>

                <div className="rounded-lg bg-slate-900/80 p-2 border border-slate-800/80">
                  <div className="text-[10px] text-slate-400">Paused</div>
                  <div className="text-sm font-bold font-mono text-amber-400 mt-0.5">
                    {node.paused_torrents}
                  </div>
                </div>

                <div className="rounded-lg bg-slate-900/80 p-2 border border-slate-800/80">
                  <div className="text-[10px] text-slate-400">Errors</div>
                  <div
                    className={`text-sm font-bold font-mono mt-0.5 ${
                      node.error_torrents > 0 ? 'text-rose-400' : 'text-slate-400'
                    }`}
                  >
                    {node.error_torrents}
                  </div>
                </div>
              </div>

              {/* Free Space */}
              <div className="flex items-center justify-between text-xs text-slate-300 rounded-lg bg-slate-900/60 p-2.5 border border-slate-800">
                <span className="flex items-center space-x-1.5 text-slate-400">
                  <HardDrive className="h-3.5 w-3.5 text-purple-400" />
                  <span>Free Disk Space:</span>
                </span>
                <span className="font-bold font-mono text-purple-300">
                  {node.free_space_gb.toFixed(2)} GB ({((node.free_space_gb / 1024)).toFixed(2)} TB)
                </span>
              </div>

              {/* Error warning if disconnected */}
              {node.last_error && (
                <div className="rounded-lg bg-rose-950/40 border border-rose-500/30 p-2 text-xs text-rose-300 font-mono">
                  {node.last_error}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {/* PROMETHEUS METRICS TAB */}
      {activeTab === 'metrics' && (
        <div className="space-y-5">
          <div className="rounded-xl border border-slate-800 bg-slate-900/80 p-4 space-y-3">
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-sm font-bold text-slate-100 flex items-center space-x-2">
                  <Activity className="h-4 w-4 text-amber-400" />
                  <span>Prometheus Metrics Scrape Endpoint</span>
                </h3>
                <p className="text-xs text-slate-400 mt-0.5">
                  Standard OpenMetrics / Prometheus exposition format available at <code>/metrics</code>.
                </p>
              </div>
              <div className="flex items-center space-x-2">
                <a
                  href="/metrics"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-sky-400 flex items-center space-x-1.5 transition-colors"
                >
                  <ExternalLink className="h-3.5 w-3.5" />
                  <span>Open /metrics</span>
                </a>
                <button
                  type="button"
                  onClick={() => {
                    navigator.clipboard.writeText(`${window.location.origin}/metrics`);
                    setCopiedMetricsUrl(true);
                    setTimeout(() => setCopiedMetricsUrl(false), 2000);
                  }}
                  className="px-3 py-1.5 rounded-lg bg-slate-800 hover:bg-slate-700 text-xs font-semibold text-slate-200 flex items-center space-x-1.5 transition-colors"
                >
                  {copiedMetricsUrl ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
                  <span>{copiedMetricsUrl ? 'Copied' : 'Copy Endpoint'}</span>
                </button>
              </div>
            </div>

            <div className="rounded-lg bg-slate-950 p-3 font-mono text-xs text-amber-300 border border-slate-800/80 overflow-x-auto select-all">
              {window.location.origin}/metrics
            </div>
          </div>

          {/* Sample Prometheus Scrape Configuration */}
          <div className="rounded-xl border border-slate-800 bg-slate-900/80 p-4 space-y-2">
            <h4 className="text-xs font-bold uppercase tracking-wider text-slate-400">Sample Prometheus Job Configuration</h4>
            <pre className="rounded-lg bg-slate-950 p-3 font-mono text-[11px] text-slate-300 border border-slate-800/80 overflow-x-auto">
{`- job_name: 'conduit'
  scrape_interval: 15s
  static_configs:
    - targets: ['${window.location.host}']`}
            </pre>
          </div>
        </div>
      )}

      {/* PLATFORM HEALTH (BACKGROUND ENGINES) TAB */}
      {activeTab === 'engines' && (
        <div className="space-y-4">
          <p className="text-xs text-slate-400">
            Conduit's own internal background engines — pollers, sync loops, the space manager, and other schedulers. Distinct from Arr-stack or fetcher-node connectivity above.
          </p>
          {!engines || engines.length === 0 ? (
            <div className="rounded-xl border border-slate-800 bg-slate-950/40 p-6 text-center text-xs text-slate-500">
              No engine telemetry available yet.
            </div>
          ) : (
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
              {engines.map((eng) => {
                const isCrashed = eng.state === 'crashed';
                const isDegraded = eng.state === 'degraded';
                const isStarting = eng.state === 'starting';
                return (
                  <div
                    key={eng.name}
                    className={`rounded-xl border p-4 space-y-2 transition-all ${
                      isCrashed
                        ? 'border-rose-500/40 bg-rose-950/20'
                        : isDegraded
                        ? 'border-amber-500/30 bg-amber-950/10'
                        : isStarting
                        ? 'border-slate-700 bg-slate-950/40'
                        : 'border-slate-800/80 bg-slate-950/40 hover:border-slate-700'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <span className="font-bold text-sm text-slate-100 font-mono">{eng.name}</span>
                      {isCrashed ? (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-rose-400 bg-rose-500/15 px-2.5 py-1 rounded-lg border border-rose-500/30">
                          <AlertOctagon className="h-3 w-3" />
                          <span>Crashed (restarted {eng.restart_count}x)</span>
                        </span>
                      ) : isDegraded ? (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-amber-400 bg-amber-500/15 px-2.5 py-1 rounded-lg border border-amber-500/30">
                          <AlertTriangle className="h-3 w-3" />
                          <span>Degraded</span>
                        </span>
                      ) : isStarting ? (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-slate-400 bg-slate-800 px-2.5 py-1 rounded-lg border border-slate-700">
                          <RefreshCw className="h-3 w-3" />
                          <span>Starting</span>
                        </span>
                      ) : (
                        <span className="inline-flex items-center space-x-1 text-xs font-semibold text-emerald-400 bg-emerald-500/10 px-2.5 py-1 rounded-lg border border-emerald-500/20">
                          <CheckCircle2 className="h-3 w-3" />
                          <span>Running</span>
                        </span>
                      )}
                    </div>
                    <div className="flex items-center space-x-3 text-[11px] text-slate-400">
                      <span>Last tick: {timeAgo(eng.last_tick_at)}</span>
                      <span>•</span>
                      <span>{eng.tick_count.toLocaleString()} cycles</span>
                      {eng.restart_count > 0 && (
                        <>
                          <span>•</span>
                          <span className="text-amber-400">{eng.restart_count} restart{eng.restart_count === 1 ? '' : 's'}</span>
                        </>
                      )}
                    </div>
                    {eng.last_error && (
                      <div className="rounded-lg bg-rose-950/40 border border-rose-500/30 p-2 text-xs text-rose-300 font-mono truncate" title={eng.last_error}>
                        {eng.last_error}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}

      {/* Batch Replace Trackers Modal */}
      {isReplaceModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 p-4 backdrop-blur-xs">
          <div className="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-900 p-6 shadow-2xl space-y-5 animate-in fade-in zoom-in-95 duration-150">
            <div className="flex items-center justify-between border-b border-slate-800 pb-3">
              <div className="flex items-center space-x-2 text-indigo-400">
                <RefreshCw className="h-5 w-5" />
                <h3 className="text-base font-bold text-slate-100">Batch Replace Trackers</h3>
              </div>
              <button
                onClick={() => setIsReplaceModalOpen(false)}
                className="rounded-lg p-1 text-slate-400 hover:bg-slate-800 hover:text-white"
              >
                <X className="h-5 w-5" />
              </button>
            </div>

            <p className="text-xs text-slate-300 leading-relaxed">
              Find and update tracker announce URLs across all matching torrents. Perfect for migrating domains, upgrading protocols (http → https), or fixing dead announce URLs.
            </p>

            <form
              onSubmit={async (e) => {
                e.preventDefault();
                if (!replaceOldUrl.trim() || !replaceNewUrl.trim()) {
                  alert('Please enter both current and replacement tracker URLs');
                  return;
                }
                setReplacing(true);
                try {
                  const res = await batchReplaceTrackers({
                    node: replaceTargetNode !== 'all' ? replaceTargetNode : undefined,
                    old_url: replaceOldUrl.trim(),
                    new_url: replaceNewUrl.trim(),
                  });
                  alert(`✅ Tracker URLs updated across ${res.replaced_torrents} active fetchers.`);
                  setIsReplaceModalOpen(false);
                  setReplaceOldUrl('');
                  setReplaceNewUrl('');
                } catch (err: any) {
                  alert(`Replacement failed: ${err.message}`);
                } finally {
                  setReplacing(false);
                }
              }}
              className="space-y-4 text-xs"
            >
              <div>
                <label className="block font-semibold text-slate-400 mb-1">
                  Target Fetcher Node
                </label>
                <select
                  value={replaceTargetNode}
                  onChange={(e) => setReplaceTargetNode(e.target.value)}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 text-slate-200 focus:outline-none focus:border-brand-500"
                >
                  <option value="all">All Connected Nodes ({health.nodes.length})</option>
                  {health.nodes.map((n) => (
                    <option key={n.name} value={n.name}>
                      {n.name} {n.connected ? `(${n.total_torrents} torrents)` : '(offline)'}
                    </option>
                  ))}
                </select>
              </div>

              <div>
                <label className="block font-semibold text-slate-400 mb-1">
                  Current Announce URL or Substring to Find
                </label>
                <input
                  type="text"
                  placeholder="e.g. http://tracker.olddomain.org:2710/announce"
                  value={replaceOldUrl}
                  onChange={(e) => setReplaceOldUrl(e.target.value)}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 text-slate-200 font-mono focus:outline-none focus:border-brand-500"
                  required
                />
              </div>

              <div>
                <label className="block font-semibold text-slate-400 mb-1">
                  New Announce URL to Replace With
                </label>
                <input
                  type="text"
                  placeholder="e.g. https://tracker.newdomain.org:443/announce"
                  value={replaceNewUrl}
                  onChange={(e) => setReplaceNewUrl(e.target.value)}
                  className="w-full rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 text-slate-200 font-mono focus:outline-none focus:border-brand-500"
                  required
                />
              </div>

              <div className="flex items-center justify-end space-x-3 pt-3 border-t border-slate-800">
                <button
                  type="button"
                  onClick={() => setIsReplaceModalOpen(false)}
                  className="px-4 py-2 rounded-lg bg-slate-800 text-slate-300 hover:bg-slate-700 text-xs font-semibold"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={replacing}
                  className="px-5 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-bold shadow-lg shadow-indigo-600/20 disabled:opacity-50"
                >
                  {replacing ? 'Updating Swarms...' : 'Execute Replacement'}
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};