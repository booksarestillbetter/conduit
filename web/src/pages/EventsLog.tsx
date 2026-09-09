// web/src/pages/EventsLog.tsx
import React, { useEffect, useState, useRef } from 'react';
import {
  ScrollText,
  RefreshCw,
  Search,
  Filter,
  AlertTriangle,
  AlertOctagon,
  Info,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Clock,
  Layers,
} from 'lucide-react';
import { fetchEvents } from '../services/api';
import { EventLogRecord } from '../types';

export interface EventsLogProps {
  // WS-pushed (see hooks/useTelemetry) — not consumed directly since fetchEvents applies
  // server-side level/module/search filtering this array doesn't reflect; its arrival is used
  // as a "something new happened" signal to trigger a debounced, spinner-free refetch instead
  // of the unconditional 5s poll this used to run.
  recentEvents?: EventLogRecord[];
}

export const EventsLog: React.FC<EventsLogProps> = ({ recentEvents: wsRecentEvents }) => {
  const [events, setEvents] = useState<EventLogRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [levelFilter, setLevelFilter] = useState<string>('all');
  const [moduleFilter, setModuleFilter] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [autoRefresh, setAutoRefresh] = useState<boolean>(true);
  const [expandedRow, setExpandedRow] = useState<number | null>(null);

  const timerRef = useRef<any>(null);

  const loadEvents = async (silent = false) => {
    if (!silent) setLoading(true);
    try {
      const data = await fetchEvents({
        limit: 300,
        level: levelFilter !== 'all' ? levelFilter : undefined,
        event_type: moduleFilter !== 'all' ? moduleFilter : undefined,
        q: searchQuery.trim() || undefined,
      });
      setEvents(data);
    } catch (e) {
      console.error('Failed to load event logs:', e);
    } finally {
      if (!silent) setLoading(false);
    }
  };

  useEffect(() => {
    loadEvents();
  }, [levelFilter, moduleFilter, searchQuery]);

  // Slow safety net (only matters while the WS is disconnected — otherwise the debounced
  // WS-driven refetch below fires long before this would).
  useEffect(() => {
    if (autoRefresh) {
      timerRef.current = setInterval(() => {
        loadEvents(true);
      }, 60000);
    } else if (timerRef.current) {
      clearInterval(timerRef.current);
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [autoRefresh, levelFilter, moduleFilter, searchQuery]);

  // WS-driven refresh: a new event pushed means something happened, so re-fetch (respecting
  // whatever filters are active) — debounced so a burst of events triggers one refetch, not one
  // per event. Silent (no loading-spinner flicker) since data is already on screen.
  useEffect(() => {
    if (!autoRefresh || !wsRecentEvents) return;
    const t = setTimeout(() => loadEvents(true), 500);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wsRecentEvents, autoRefresh]);

  const getLevelBadge = (level: string) => {
    const l = level.toLowerCase();
    if (l === 'error') {
      return (
        <span className="inline-flex items-center space-x-1 rounded-md bg-rose-500/10 px-2 py-0.5 text-xs font-bold text-rose-400 border border-rose-500/20">
          <AlertOctagon className="h-3 w-3" />
          <span>ERROR</span>
        </span>
      );
    }
    if (l === 'warn' || l === 'warning') {
      return (
        <span className="inline-flex items-center space-x-1 rounded-md bg-amber-500/10 px-2 py-0.5 text-xs font-bold text-amber-400 border border-amber-500/20">
          <AlertTriangle className="h-3 w-3" />
          <span>WARN</span>
        </span>
      );
    }
    if (l === 'debug') {
      return (
        <span className="inline-flex items-center space-x-1 rounded-md bg-purple-500/10 px-2 py-0.5 text-xs font-bold text-purple-400 border border-purple-500/20">
          <span>DEBUG</span>
        </span>
      );
    }
    return (
      <span className="inline-flex items-center space-x-1 rounded-md bg-sky-500/10 px-2 py-0.5 text-xs font-bold text-sky-400 border border-sky-500/20">
        <Info className="h-3 w-3" />
        <span>INFO</span>
      </span>
    );
  };

  const getModuleBadgeColor = (mod: string) => {
    const m = mod.toLowerCase();
    if (m.includes('sonarr') || m.includes('radarr') || m.includes('arr')) return 'bg-cyan-500/10 text-cyan-300 border-cyan-500/30';
    if (m.includes('trans') || m.includes('node') || m.includes('poll')) return 'bg-indigo-500/10 text-indigo-300 border-indigo-500/30';
    if (m.includes('sync') || m.includes('stage') || m.includes('file')) return 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30';
    if (m.includes('space') || m.includes('purge')) return 'bg-amber-500/10 text-amber-300 border-amber-500/30';
    if (m.includes('pipeline') || m.includes('replace')) return 'bg-rose-500/10 text-rose-300 border-rose-500/30';
    if (m.includes('plex') || m.includes('trakt')) return 'bg-orange-500/10 text-orange-300 border-orange-500/30';
    return 'bg-slate-800 text-slate-300 border-slate-700';
  };

  return (
    <div className="flex-1 overflow-auto p-8 space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="flex items-center space-x-2.5">
            <div className="rounded-xl bg-brand-500/10 p-2.5 text-brand-400 border border-brand-500/20">
              <ScrollText className="h-6 w-6" />
            </div>
            <div>
              <h1 className="text-2xl font-bold text-slate-100">System Logs & Event History</h1>
              <p className="text-xs text-slate-400 mt-0.5">
                Centralized audit trail for *arr grabs, fetcher events, copy2queue staging, space auto-purges, and replacement pipelines.
              </p>
            </div>
          </div>
        </div>

        <div className="flex items-center space-x-3">
          <label className="flex items-center space-x-2 text-xs font-semibold text-slate-300 bg-slate-900/80 border border-slate-800 rounded-xl px-3 py-2 cursor-pointer hover:border-slate-700">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="rounded border-slate-700 bg-slate-800 text-brand-600 focus:ring-brand-500 h-3.5 w-3.5"
            />
            <span>Auto-Refresh (5s)</span>
          </label>

          <button
            onClick={() => loadEvents()}
            disabled={loading}
            className="flex items-center space-x-2 rounded-xl bg-slate-800 border border-slate-700 px-4 py-2 text-xs font-semibold text-slate-200 hover:bg-slate-700 disabled:opacity-50 transition-colors"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
            <span>Refresh</span>
          </button>
        </div>
      </div>

      {/* Filter Toolbar */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 space-y-3">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
          {/* Level Filter */}
          <div>
            <label className="block text-[11px] font-bold uppercase tracking-wider text-slate-400 mb-1">
              Log Severity Level
            </label>
            <select
              value={levelFilter}
              onChange={(e) => setLevelFilter(e.target.value)}
              className="w-full rounded-xl border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs font-semibold text-slate-200 focus:border-brand-500 focus:outline-none"
            >
              <option value="all">📑 All Log Levels (Default)</option>
              <option value="errors_warnings">⚠️ Errors & Warnings Only</option>
              <option value="error">🛑 Errors Only</option>
              <option value="warn">⚠️ Warnings Only</option>
              <option value="info">ℹ️ Info Only</option>
              <option value="debug">🔍 Debug Only</option>
            </select>
          </div>

          {/* Module / Subsystem Filter */}
          <div>
            <label className="block text-[11px] font-bold uppercase tracking-wider text-slate-400 mb-1">
              Subsystem Module
            </label>
            <select
              value={moduleFilter}
              onChange={(e) => setModuleFilter(e.target.value)}
              className="w-full rounded-xl border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs font-semibold text-slate-200 focus:border-brand-500 focus:outline-none"
            >
              <option value="all">🌐 All Subsystems</option>
              <option value="sonarr">📺 Sonarr</option>
              <option value="radarr">🎬 Radarr</option>
              <option value="arr">📡 All *arr Stack</option>
              <option value="sync">📦 Staging & Queue Routing</option>
              <option value="file_sync">🔄 File Sync & Cleaner</option>
              <option value="space">🧹 Space Management / Auto-Purge</option>
              <option value="pipeline">🔁 Pipeline & MediaReplacer</option>
              <option value="transmission">⚡ Fetcher Nodes</option>
              <option value="plex">🍿 Plex Integration</option>
              <option value="trakt">⭐ Trakt Sync</option>
              <option value="auth">🔐 Security & Auth</option>
            </select>
          </div>

          {/* Search Bar */}
          <div className="md:col-span-2">
            <label className="block text-[11px] font-bold uppercase tracking-wider text-slate-400 mb-1">
              Search Messages & Details
            </label>
            <div className="relative">
              <Search className="absolute left-3 top-2.5 h-3.5 w-3.5 text-slate-400" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search releases, error strings, infohashes, torrent names..."
                className="w-full rounded-xl border border-slate-700 bg-slate-800 py-1.5 pl-9 pr-3 text-xs text-slate-200 placeholder-slate-500 focus:border-brand-500 focus:outline-none font-mono"
              />
              {searchQuery && (
                <button
                  onClick={() => setSearchQuery('')}
                  className="absolute right-3 top-2 text-xs text-slate-400 hover:text-slate-200"
                >
                  &times;
                </button>
              )}
            </div>
          </div>
        </div>

        {/* Active Filter Chips Bar */}
        <div className="flex items-center justify-between text-[11px] text-slate-400 border-t border-slate-800/80 pt-2 font-mono">
          <div className="flex items-center space-x-2">
            <span>Filter:</span>
            <span className="rounded bg-slate-800 px-2 py-0.5 text-slate-300 font-semibold">
              Level: {levelFilter}
            </span>
            <span className="rounded bg-slate-800 px-2 py-0.5 text-slate-300 font-semibold">
              Module: {moduleFilter}
            </span>
            {searchQuery && (
              <span className="rounded bg-slate-800 px-2 py-0.5 text-brand-300 font-semibold">
                Query: "{searchQuery}"
              </span>
            )}
          </div>
          <div>
            Showing <strong className="text-slate-200">{events.length}</strong> events
          </div>
        </div>
      </div>

      {/* Events Table */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 overflow-hidden shadow-xl">
        <table className="w-full text-left border-collapse text-xs">
          <thead className="bg-slate-950/80 border-b border-slate-800 text-[11px] font-bold text-slate-400 uppercase tracking-wider">
            <tr>
              <th className="py-3 px-4 w-44">Timestamp</th>
              <th className="py-3 px-4 w-36">Subsystem</th>
              <th className="py-3 px-4 w-28">Severity</th>
              <th className="py-3 px-4">Event Message</th>
              <th className="py-3 px-4 w-16 text-right">Details</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60 font-medium">
            {events.length === 0 ? (
              <tr>
                <td colSpan={5} className="py-16 text-center text-slate-500 font-normal">
                  <div className="flex flex-col items-center space-y-2">
                    <ScrollText className="h-8 w-8 text-slate-600" />
                    <span>No log entries match the current filter criteria.</span>
                    {levelFilter === 'errors_warnings' && (
                      <button
                        onClick={() => setLevelFilter('all')}
                        className="text-xs text-brand-400 underline hover:text-brand-300"
                      >
                        Switch to "All Log Levels"
                      </button>
                    )}
                  </div>
                </td>
              </tr>
            ) : (
              events.map((ev) => {
                const isExpanded = expandedRow === ev.id;
                return (
                  <React.Fragment key={ev.id}>
                    <tr
                      onClick={() => ev.details_json && setExpandedRow(isExpanded ? null : ev.id)}
                      className={`hover:bg-slate-800/30 transition-colors ${ev.details_json ? 'cursor-pointer' : ''}`}
                    >
                      <td className="py-3 px-4 font-mono text-[11px] text-slate-400 whitespace-nowrap">
                        {new Date(ev.created_at).toLocaleString()}
                      </td>
                      <td className="py-3 px-4">
                        <span className={`rounded-md px-2 py-0.5 text-[11px] font-mono border font-semibold ${getModuleBadgeColor(ev.event_type)}`}>
                          {ev.event_type}
                        </span>
                      </td>
                      <td className="py-3 px-4">
                        {getLevelBadge(ev.level)}
                      </td>
                      <td className="py-3 px-4 text-slate-200 font-normal leading-relaxed">
                        {ev.message}
                      </td>
                      <td className="py-3 px-4 text-right">
                        {ev.details_json && (
                          <button
                            type="button"
                            className="text-slate-400 hover:text-slate-200"
                            title="Toggle Details"
                          >
                            {isExpanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
                          </button>
                        )}
                      </td>
                    </tr>
                    {isExpanded && ev.details_json && (
                      <tr className="bg-slate-950/90 border-b border-slate-800">
                        <td colSpan={5} className="p-4">
                          <div className="rounded-xl border border-slate-800 bg-slate-900 p-3 space-y-1.5 font-mono text-[11px]">
                            <div className="text-slate-400 uppercase tracking-wider font-sans font-bold text-[10px]">
                              Event Payload Details
                            </div>
                            <pre className="text-sky-300 overflow-x-auto whitespace-pre-wrap">
                              {(() => {
                                try {
                                  return JSON.stringify(JSON.parse(ev.details_json), null, 2);
                                } catch {
                                  return ev.details_json;
                                }
                              })()}
                            </pre>
                          </div>
                        </td>
                      </tr>
                    )}
                  </React.Fragment>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
};
