// web/src/components/ArchivedGrabsView.tsx
import React, { useState, useEffect } from 'react';
import {
  Ghost,
  Search,
  RefreshCw,
  Tv,
  Film,
  CheckCircle2,
  Clock,
  RotateCcw,
  Trash2,
  ExternalLink,
  ChevronDown,
  ChevronRight,
  Database,
} from 'lucide-react';
import { fetchArrGrabs } from '../services/api';
import { ArrGrabRecord } from '../types';
import { GrabHistoryTimeline } from './GrabHistoryTimeline';

export interface ArchivedGrabsViewProps {
  // Multi-tenant zone filter (undefined = "All Zones", the unfiltered default).
  zone?: string;
}

export const ArchivedGrabsView: React.FC<ArchivedGrabsViewProps> = ({ zone }) => {
  const [grabs, setGrabs] = useState<ArrGrabRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const loadGrabs = async () => {
    setLoading(true);
    try {
      const data = await fetchArrGrabs({
        q: searchQuery.trim() || undefined,
        status: statusFilter !== 'all' ? statusFilter : undefined,
        limit: 300,
        zone,
      });
      setGrabs(data);
    } catch (e) {
      console.error('Failed to fetch archived grabs:', e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadGrabs();
  }, [statusFilter, searchQuery, zone]);

  const getStatusBadge = (status: string, reSearched: boolean) => {
    if (reSearched || status === 'replaced') {
      return (
        <span className="inline-flex items-center space-x-1 rounded-md bg-rose-500/10 px-2 py-0.5 text-xs font-bold text-rose-400 border border-rose-500/20">
          <RotateCcw className="h-3 w-3" />
          <span>REPLACED</span>
        </span>
      );
    }
    if (status === 'imported' || status === 'downloaded') {
      return (
        <span className="inline-flex items-center space-x-1 rounded-md bg-emerald-500/10 px-2 py-0.5 text-xs font-bold text-emerald-400 border border-emerald-500/20">
          <CheckCircle2 className="h-3 w-3" />
          <span>IMPORTED</span>
        </span>
      );
    }
    if (status === 'deleted' || status.toLowerCase().includes('delete')) {
      return (
        <span className="inline-flex items-center space-x-1 rounded-md bg-rose-500/10 px-2 py-0.5 text-xs font-bold text-rose-400 border border-rose-500/20">
          <Trash2 className="h-3 w-3" />
          <span>DELETED</span>
        </span>
      );
    }
    if (status === 'fetched') {
      return (
        <span className="inline-flex items-center space-x-1 rounded-md bg-sky-500/10 px-2 py-0.5 text-xs font-bold text-sky-400 border border-sky-500/20">
          <Clock className="h-3 w-3" />
          <span>FETCHED</span>
        </span>
      );
    }
    return (
      <span className="inline-flex items-center space-x-1 rounded-md bg-slate-800 px-2 py-0.5 text-xs font-semibold text-slate-300 border border-slate-700">
        <span>{status.toUpperCase()}</span>
      </span>
    );
  };

  return (
    <div className="flex-1 overflow-auto p-8 space-y-6 max-w-7xl mx-auto">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div className="flex items-center space-x-3">
          <div className="rounded-xl bg-purple-500/10 p-2.5 text-purple-400 border border-purple-500/20">
            <Ghost className="h-6 w-6" />
          </div>
          <div>
            <h1 className="text-2xl font-bold text-slate-100">Ghost Fetcher & Grab Archive</h1>
            <p className="text-xs text-slate-400 mt-0.5">
              Historical timeline of Sonarr, Radarr, and Lidarr grabs, completed intakes, and MediaReplacer re-searches even after deletion from fetcher daemons.
            </p>
          </div>
        </div>

        <button
          onClick={() => loadGrabs()}
          disabled={loading}
          className="flex items-center space-x-2 rounded-xl bg-slate-800 border border-slate-700 px-4 py-2 text-xs font-semibold text-slate-200 hover:bg-slate-700 disabled:opacity-50 transition-colors"
        >
          <RefreshCw className={`h-3.5 w-3.5 ${loading ? 'animate-spin' : ''}`} />
          <span>Refresh Archive</span>
        </button>
      </div>

      {/* Filter Toolbar */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 p-4 space-y-3">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          {/* Status Filter */}
          <div>
            <label className="block text-[11px] font-bold uppercase tracking-wider text-slate-400 mb-1">
              Grab Lifecycle Status
            </label>
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              className="w-full rounded-xl border border-slate-700 bg-slate-800 py-1.5 px-3 text-xs font-semibold text-slate-200 focus:border-brand-500 focus:outline-none"
            >
              <option value="all">📦 All Historical Grabs</option>
              <option value="imported">✅ Imported to Library</option>
              <option value="fetched">⏳ Fetched / Downloading</option>
              <option value="replaced">🔁 Replaced / Re-searched</option>
              <option value="deleted">🗑️ Deleted</option>
            </select>
          </div>

          {/* Search Bar */}
          <div className="md:col-span-2">
            <label className="block text-[11px] font-bold uppercase tracking-wider text-slate-400 mb-1">
              Search Titles, Indexers, or Infohashes
            </label>
            <div className="relative">
              <Search className="absolute left-3 top-2.5 h-3.5 w-3.5 text-slate-400" />
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="Search series name, movie title, indexer (e.g. PTP, BTN), or download hash..."
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

        {/* Count Bar */}
        <div className="flex items-center justify-between text-[11px] text-slate-400 border-t border-slate-800/80 pt-2 font-mono">
          <div className="flex items-center space-x-2">
            <span>Filter:</span>
            <span className="rounded bg-slate-800 px-2 py-0.5 text-slate-300 font-semibold">
              Status: {statusFilter}
            </span>
            {searchQuery && (
              <span className="rounded bg-slate-800 px-2 py-0.5 text-purple-300 font-semibold">
                Query: "{searchQuery}"
              </span>
            )}
          </div>
          <div>
            Showing <strong className="text-slate-200">{grabs.length}</strong> archived records
          </div>
        </div>
      </div>

      {/* Grabs Table */}
      <div className="rounded-2xl border border-slate-800 bg-slate-900/60 overflow-hidden shadow-xl">
        <table className="w-full text-left border-collapse text-xs">
          <thead className="bg-slate-950/80 border-b border-slate-800 text-[11px] font-bold text-slate-400 uppercase tracking-wider">
            <tr>
              <th className="py-3 px-4 w-44">Grab Date</th>
              <th className="py-3 px-4 w-28">Ecosystem</th>
              <th className="py-3 px-4">Scene Release / Content Title</th>
              <th className="py-3 px-4 w-32">Indexer</th>
              <th className="py-3 px-4 w-28">Client</th>
              <th className="py-3 px-4 w-28">Status</th>
              <th className="py-3 px-4 w-12 text-right"></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60 font-medium">
            {grabs.length === 0 ? (
              <tr>
                <td colSpan={7} className="py-16 text-center text-slate-500 font-normal">
                  <div className="flex flex-col items-center space-y-2">
                    <Ghost className="h-8 w-8 text-slate-600" />
                    <span>No archived grabs found matching your search.</span>
                  </div>
                </td>
              </tr>
            ) : (
              grabs.map((g) => {
                const isExpanded = expandedId === g.id;
                const isSeries = g.item_type === 'series';

                return (
                  <React.Fragment key={g.id}>
                    <tr
                      onClick={() => setExpandedId(isExpanded ? null : g.id)}
                      className="hover:bg-slate-800/30 transition-colors cursor-pointer"
                    >
                      <td className="py-3 px-4 font-mono text-[11px] text-slate-400 whitespace-nowrap">
                        {new Date(g.created_at).toLocaleString()}
                      </td>
                      <td className="py-3 px-4">
                        <span className={`inline-flex items-center space-x-1 rounded-md px-2 py-0.5 text-[11px] font-semibold border ${
                          isSeries ? 'bg-cyan-500/10 text-cyan-400 border-cyan-500/20' : 'bg-amber-500/10 text-amber-400 border-amber-500/20'
                        }`}>
                          {isSeries ? <Tv className="h-3 w-3" /> : <Film className="h-3 w-3" />}
                          <span>{isSeries ? 'Sonarr' : 'Radarr'}</span>
                        </span>
                      </td>
                      <td className="py-3 px-4">
                        <div className="font-semibold text-slate-100 truncate max-w-lg" title={g.scene_name}>
                          {g.scene_name}
                        </div>
                        {g.release_title && g.release_title !== g.scene_name && (
                          <div className="text-[11px] text-slate-400 truncate max-w-lg">{g.release_title}</div>
                        )}
                        {isSeries && g.season_number && (
                          <div className="text-[10px] text-slate-500 font-mono mt-0.5">
                            Season {g.season_number} {g.episode_numbers ? `• Ep ${g.episode_numbers}` : ''}
                          </div>
                        )}
                      </td>
                      <td className="py-3 px-4">
                        <span className="rounded bg-slate-800 px-2 py-0.5 font-mono text-[11px] text-slate-300 border border-slate-700">
                          {g.indexer || 'Direct'}
                        </span>
                      </td>
                      <td className="py-3 px-4 text-slate-400 text-[11px] truncate" title={g.download_client || 'Fetcher'}>
                        {g.download_client || 'Fetcher'}
                      </td>
                      <td className="py-3 px-4">
                        {getStatusBadge(g.status, g.re_searched)}
                      </td>
                      <td className="py-3 px-4 text-right">
                        <button type="button" className="text-slate-400 hover:text-slate-200">
                          {isExpanded ? <ChevronDown className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
                        </button>
                      </td>
                    </tr>
                    {isExpanded && (
                      <tr className="bg-slate-950/90 border-b border-slate-800">
                        <td colSpan={7} className="p-4">
                          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                            <div className="rounded-xl border border-slate-800 bg-slate-900 p-3 space-y-2 font-mono text-[11px] text-slate-300">
                              <div className="text-slate-400 uppercase tracking-wider font-sans font-bold text-[10px]">
                                Metadata Correlation
                              </div>
                              <div><strong>Download Hash / ID:</strong> <span className="text-sky-300">{g.download_id || g.id}</span></div>
                              <div><strong>Status:</strong> {g.status} (Re-searched: {g.re_searched ? `Yes (${g.re_search_count}x)` : 'No'})</div>
                              <div><strong>First Recorded:</strong> {new Date(g.created_at).toISOString()}</div>
                              <div><strong>Last Updated:</strong> {new Date(g.updated_at).toISOString()}</div>
                            </div>
                            <div className="rounded-xl border border-slate-800 bg-slate-900 p-3 space-y-1 font-mono text-[11px] text-slate-300">
                              <div className="text-slate-400 uppercase tracking-wider font-sans font-bold text-[10px]">
                                Raw Webhook Ingestion Payload
                              </div>
                              <pre className="text-sky-300 overflow-x-auto max-h-32 text-[10px] whitespace-pre-wrap">
                                {(() => {
                                  if (!g.payload_json) return 'No payload data available';
                                  try {
                                    return JSON.stringify(JSON.parse(g.payload_json), null, 2);
                                  } catch {
                                    return g.payload_json;
                                  }
                                })()}
                              </pre>
                            </div>
                          </div>
                          <div className="mt-4 rounded-xl border border-slate-800 bg-slate-900 p-3">
                            <div className="text-slate-400 uppercase tracking-wider font-sans font-bold text-[10px] mb-2">
                              Full Timeline (grab / import / delete / re-search)
                            </div>
                            <GrabHistoryTimeline grabId={g.id} />
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
