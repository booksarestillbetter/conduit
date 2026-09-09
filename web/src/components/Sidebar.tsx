// web/src/components/Sidebar.tsx
import React, { useState, useEffect } from 'react';
import {
  LayoutDashboard,
  Layers,
  Settings,
  ScrollText,
  FileCode2,
  Server,
  BookOpen,
  Ghost,
  ChevronDown,
  ChevronRight,
  Radio,
  SlidersHorizontal,
  Compass,
  Sparkles,
  Tv,
  Film,
  Music,
  ExternalLink,
  Globe,
  Boxes,
} from 'lucide-react';
import { AggregateStats, AppConfig, ZoneConfig } from '../types';
import { fetchSettings } from '../services/api';

interface SidebarProps {
  currentRoute: string;
  onNavigate: (route: string) => void;
  activeNode: string;
  onSelectNode: (node: string) => void;
  statusFilter: string;
  onSelectStatusFilter: (status: string) => void;
  trackerFilter?: string;
  onSelectTrackerFilter?: (tracker: string) => void;
  trackerCounts?: Array<{ host: string; count: number }>;
  stats: AggregateStats | null;
  torrentCounts: {
    all: number;
    downloading: number;
    seeding: number;
    active: number;
    queued?: number;
    paused: number;
    checking: number;
    error: number;
  };
  zones?: ZoneConfig[];
  activeZone?: string;
  onSelectZone?: (zoneId: string) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentRoute,
  onNavigate,
  activeNode,
  onSelectNode,
  statusFilter,
  onSelectStatusFilter,
  trackerFilter = 'all',
  onSelectTrackerFilter,
  trackerCounts = [],
  stats,
  torrentCounts,
  zones = [],
  activeZone = 'all',
  onSelectZone,
}) => {
  const [isNodesOpen, setIsNodesOpen] = useState(true);
  const [isZonesOpen, setIsZonesOpen] = useState(true);
  const [isStatusOpen, setIsStatusOpen] = useState(true);
  const [isTrackersOpen, setIsTrackersOpen] = useState(true);
  const [trackerSearch, setTrackerSearch] = useState('');
  const [config, setConfig] = useState<AppConfig | null>(null);

  useEffect(() => {
    fetchSettings().then(setConfig).catch((e) => console.error('Sidebar failed to load settings (quick links will be hidden):', e));
  }, []);

  const sonarrUrl = config?.sonarr?.master?.base_url || config?.sonarr?.primary?.base_url || (config?.sonarr?.enabled ? 'http://localhost:8989' : undefined);
  const radarrUrl = config?.radarr?.master?.base_url || config?.radarr?.primary?.base_url || (config?.radarr?.enabled ? 'http://localhost:7878' : undefined);
  const lidarrUrl = config?.lidarr?.master?.base_url || config?.lidarr?.primary?.base_url || (config?.lidarr?.enabled ? 'http://localhost:8686' : undefined);
  const requestPortalUrl = config?.system?.request_portal_url;
  const requestPortalLabel = config?.system?.request_portal_label || 'Media Requests';
  const mediaPortalUrl = config?.system?.media_portal_url;
  const mediaPortalLabel = config?.system?.media_portal_label || 'Media Portal';

  const mainNav = [
    { path: '/dashboard', label: 'Dashboard', icon: LayoutDashboard },
    { path: '/fetchers', label: 'Fetchers', icon: Layers },
    { path: '/pipeline', label: "🐕 Conduit Pipeline", icon: Compass },
    { path: '/archive', label: 'Ghost Archive', icon: Ghost },
    { path: '/settings', label: 'Conduit Settings', icon: Settings },
    { path: '/logs', label: 'System Logs', icon: ScrollText },
    { path: '/docs', label: 'Documentation', icon: BookOpen },
  ];

  const statusFilters = [
    { id: 'all', label: 'All', count: torrentCounts.all },
    { id: 'downloading', label: 'Downloading', count: torrentCounts.downloading, color: 'text-emerald-400' },
    { id: 'seeding', label: 'Seeding', count: torrentCounts.seeding, color: 'text-sky-400' },
    { id: 'active', label: 'Active', count: torrentCounts.active, color: 'text-brand-400' },
    { id: 'queued', label: 'Queued', count: torrentCounts.queued ?? 0, color: 'text-amber-300' },
    { id: 'paused', label: 'Paused', count: torrentCounts.paused, color: 'text-slate-400' },
    { id: 'checking', label: 'Checking', count: torrentCounts.checking, color: 'text-amber-400' },
    { id: 'error', label: 'Errors', count: torrentCounts.error, color: 'text-rose-400' },
  ];

  const filteredTrackers = trackerCounts.filter((tr) =>
    tr.host.toLowerCase().includes(trackerSearch.toLowerCase())
  );

  return (
    <aside className="w-64 border-r border-slate-800 bg-slate-900/40 p-4 flex flex-col justify-between shrink-0 overflow-y-auto">
      <div className="space-y-6">
        {/* Main Navigation */}
        <div className="space-y-1">
          <p className="px-3 text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2">Main Menu</p>
          {mainNav.map((item) => {
            const Icon = item.icon;
            const active = currentRoute === item.path || (item.path === '/dashboard' && currentRoute === '/');
            return (
              <button
                key={item.path}
                onClick={() => onNavigate(item.path)}
                className={`flex w-full items-center space-x-3 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${
                  active
                    ? 'bg-brand-600/20 text-brand-400 border border-brand-500/30 font-semibold'
                    : 'text-slate-300 hover:bg-slate-800 hover:text-white'
                }`}
              >
                <Icon className="h-4 w-4" />
                <span>{item.label}</span>
              </button>
            );
          })}
        </div>

        {/* Media Stack External Quick Links */}
        {(requestPortalUrl || mediaPortalUrl || sonarrUrl || radarrUrl || lidarrUrl) && (
        <div className="space-y-1 border-t border-slate-800/80 pt-4">
          <p className="px-3 text-xs font-semibold text-slate-400 uppercase tracking-wider mb-2 flex items-center justify-between">
            <span>🐕 Media Stack</span>
            <span className="text-[10px] text-slate-500 font-mono">External</span>
          </p>
          <div className="space-y-1">
            {requestPortalUrl && (
              <a
                href={requestPortalUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center justify-between rounded-lg px-3 py-1.5 text-xs text-pink-300 hover:bg-pink-500/10 border border-slate-800/60 hover:border-pink-500/30 transition-all group"
                title={`Open ${requestPortalLabel} (${requestPortalUrl})`}
              >
                <div className="flex items-center space-x-2">
                  <span className="text-sm group-hover:scale-110 transition-transform">🍖</span>
                  <span className="font-semibold">{requestPortalLabel}</span>
                </div>
                <ExternalLink className="h-3 w-3 text-slate-500 group-hover:text-pink-400" />
              </a>
            )}

            {sonarrUrl && (
              <a
                href={sonarrUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center justify-between rounded-lg px-3 py-1.5 text-xs text-sky-300 hover:bg-sky-500/10 border border-slate-800/60 hover:border-sky-500/30 transition-all group"
                title={`Open Sonarr (${sonarrUrl})`}
              >
                <div className="flex items-center space-x-2">
                  <Tv className="h-3.5 w-3.5 text-sky-400 group-hover:scale-110 transition-transform" />
                  <span className="font-semibold">Sonarr TV</span>
                </div>
                <ExternalLink className="h-3 w-3 text-slate-500 group-hover:text-sky-400" />
              </a>
            )}
            {radarrUrl && (
              <a
                href={radarrUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center justify-between rounded-lg px-3 py-1.5 text-xs text-amber-300 hover:bg-amber-500/10 border border-slate-800/60 hover:border-amber-500/30 transition-all group"
                title={`Open Radarr (${radarrUrl})`}
              >
                <div className="flex items-center space-x-2">
                  <Film className="h-3.5 w-3.5 text-amber-400 group-hover:scale-110 transition-transform" />
                  <span className="font-semibold">Radarr Movies</span>
                </div>
                <ExternalLink className="h-3 w-3 text-slate-500 group-hover:text-amber-400" />
              </a>
            )}
            {lidarrUrl && (
              <a
                href={lidarrUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center justify-between rounded-lg px-3 py-1.5 text-xs text-emerald-300 hover:bg-emerald-500/10 border border-slate-800/60 hover:border-emerald-500/30 transition-all group"
                title={`Open Lidarr (${lidarrUrl})`}
              >
                <div className="flex items-center space-x-2">
                  <Music className="h-3.5 w-3.5 text-emerald-400 group-hover:scale-110 transition-transform" />
                  <span className="font-semibold">Lidarr Music</span>
                </div>
                <ExternalLink className="h-3 w-3 text-slate-500 group-hover:text-emerald-400" />
              </a>
            )}

            {mediaPortalUrl && (
              <a
                href={mediaPortalUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center justify-between rounded-lg px-3 py-1.5 text-xs text-slate-300 hover:bg-slate-800 hover:text-white border border-slate-800/60 transition-all group"
                title={`Open ${mediaPortalLabel} (${mediaPortalUrl})`}
              >
                <div className="flex items-center space-x-2">
                  <Globe className="h-3.5 w-3.5 text-brand-400 group-hover:scale-110 transition-transform" />
                  <span className="font-medium">{mediaPortalLabel}</span>
                </div>
                <ExternalLink className="h-3 w-3 text-slate-500 group-hover:text-brand-400" />
              </a>
            )}
          </div>
        </div>
        )}

        {/* Zones Filter (only shown once the user has configured at least one) */}
        {zones.length > 0 && onSelectZone && (
          <div className="space-y-1 border-t border-slate-800/80 pt-4">
            <button
              type="button"
              onClick={() => setIsZonesOpen(!isZonesOpen)}
              className="flex items-center justify-between w-full px-3 mb-2 text-xs font-semibold text-slate-400 uppercase tracking-wider hover:text-slate-200"
            >
              <span className="flex items-center space-x-1.5">
                <Boxes className="h-3.5 w-3.5 text-purple-400" />
                <span>Zones</span>
              </span>
              {isZonesOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            </button>

            {isZonesOpen && (
              <div className="space-y-1">
                <button
                  onClick={() => onSelectZone('all')}
                  className={`flex w-full items-center justify-between rounded-lg px-3 py-1.5 text-xs transition-colors ${
                    activeZone === 'all'
                      ? 'bg-slate-800 font-semibold text-white border border-slate-700'
                      : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
                  }`}
                >
                  <span>All Zones</span>
                </button>
                {zones.map((zone) => (
                  <button
                    key={zone.id}
                    onClick={() => onSelectZone(zone.id)}
                    className={`flex w-full items-center justify-between rounded-lg px-3 py-1.5 text-xs transition-colors ${
                      activeZone === zone.id
                        ? 'bg-slate-800 font-semibold text-white border border-slate-700'
                        : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
                    }`}
                  >
                    <span className="truncate">{zone.name}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Nodes Filter List */}
        <div className="space-y-1 border-t border-slate-800/80 pt-4">
          <button
            type="button"
            onClick={() => setIsNodesOpen(!isNodesOpen)}
            className="flex items-center justify-between w-full px-3 mb-2 text-xs font-semibold text-slate-400 uppercase tracking-wider hover:text-slate-200"
          >
            <span className="flex items-center space-x-1.5">
              <Server className="h-3.5 w-3.5 text-brand-400" />
              <span>Nodes Filter</span>
            </span>
            <div className="flex items-center space-x-1">
              <span className="text-[10px] text-slate-500 font-mono lowercase">
                {stats?.connected_nodes || 0}/{stats?.total_nodes || 0}
              </span>
              {isNodesOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            </div>
          </button>

          {isNodesOpen && (
            <div className="space-y-1">
              <button
                onClick={() => {
                  onSelectNode('all');
                  if (currentRoute !== '/fetchers') onNavigate('/fetchers');
                }}
                className={`flex w-full items-center justify-between rounded-lg px-3 py-1.5 text-xs transition-colors ${
                  activeNode === 'all'
                    ? 'bg-slate-800 font-semibold text-white border border-slate-700'
                    : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
                }`}
              >
                <span>All Fetchers</span>
                <span className="font-mono text-slate-400">{stats?.total_torrents || 0}</span>
              </button>

              {stats?.nodes.map((node) => {
                const isSelected = activeNode === node.node;
                return (
                  <button
                    key={node.node}
                    onClick={() => {
                      onSelectNode(node.node);
                      if (currentRoute !== '/fetchers') onNavigate(`/fetchers?node=${node.node}`);
                    }}
                    className={`flex w-full items-center justify-between rounded-lg px-3 py-1.5 text-xs transition-colors ${
                      isSelected
                        ? 'bg-slate-800 font-semibold text-white border border-slate-700'
                        : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
                    }`}
                  >
                    <div className="flex items-center space-x-2 truncate">
                      <span className={`h-2 w-2 rounded-full shrink-0 ${node.connected ? 'bg-emerald-500' : 'bg-rose-500'}`} />
                      <span className="truncate">{node.node}</span>
                    </div>
                    <span className="font-mono text-slate-400 shrink-0">{node.total_torrents}</span>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        {/* Status Filters (Active on Fetchers page) */}
        {currentRoute.startsWith('/fetchers') && (
          <div className="space-y-1 border-t border-slate-800/80 pt-4">
            <button
              type="button"
              onClick={() => setIsStatusOpen(!isStatusOpen)}
              className="flex items-center justify-between w-full px-3 mb-2 text-xs font-semibold text-slate-400 uppercase tracking-wider hover:text-slate-200"
            >
              <span className="flex items-center space-x-1.5">
                <SlidersHorizontal className="h-3.5 w-3.5 text-sky-400" />
                <span>Status Filter</span>
              </span>
              {isStatusOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            </button>

            {isStatusOpen && (
              <div className="space-y-1">
                {statusFilters.map((f) => {
                  const active = statusFilter === f.id;
                  return (
                    <button
                      key={f.id}
                      onClick={() => onSelectStatusFilter(f.id)}
                      className={`flex w-full items-center justify-between rounded-lg px-3 py-1.5 text-xs transition-colors ${
                        active
                          ? 'bg-slate-800 font-semibold text-white border border-slate-700'
                          : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
                      }`}
                    >
                      <span className={f.color || ''}>{f.label}</span>
                      <span className="rounded-full bg-slate-800/80 px-2 py-0.5 text-[10px] text-slate-300 font-mono">
                        {f.count}
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        )}

        {/* Trackers Filter (Active on Fetchers page) */}
        {currentRoute.startsWith('/fetchers') && trackerCounts.length > 0 && onSelectTrackerFilter && (
          <div className="space-y-1 border-t border-slate-800/80 pt-4">
            <button
              type="button"
              onClick={() => setIsTrackersOpen(!isTrackersOpen)}
              className="flex items-center justify-between w-full px-3 mb-2 text-xs font-semibold text-slate-400 uppercase tracking-wider hover:text-slate-200"
            >
              <span className="flex items-center space-x-1.5">
                <Radio className="h-3.5 w-3.5 text-emerald-400" />
                <span>Trackers ({trackerCounts.length})</span>
              </span>
              {isTrackersOpen ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
            </button>

            {isTrackersOpen && (
              <div className="space-y-1.5">
                {trackerCounts.length > 6 && (
                  <div className="px-1">
                    <input
                      type="text"
                      value={trackerSearch}
                      onChange={(e) => setTrackerSearch(e.target.value)}
                      placeholder="Filter trackers..."
                      className="w-full rounded-md border border-slate-800 bg-slate-950/80 px-2 py-1 text-[11px] text-slate-200 placeholder-slate-500 focus:border-brand-500 focus:outline-none"
                    />
                  </div>
                )}

                <div className="max-h-48 overflow-y-auto space-y-1 pr-1">
                  <button
                    onClick={() => onSelectTrackerFilter('all')}
                    className={`flex w-full items-center justify-between rounded-lg px-3 py-1.5 text-xs transition-colors ${
                      trackerFilter === 'all'
                        ? 'bg-slate-800 font-semibold text-white border border-slate-700'
                        : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
                    }`}
                  >
                    <span>All Trackers</span>
                    <span className="font-mono text-slate-400">{torrentCounts.all}</span>
                  </button>

                  {filteredTrackers.map((tr) => {
                    const active = trackerFilter.toLowerCase() === tr.host.toLowerCase();
                    return (
                      <button
                        key={tr.host}
                        onClick={() => onSelectTrackerFilter(tr.host)}
                        className={`flex w-full items-center justify-between rounded-lg px-3 py-1.5 text-xs transition-colors ${
                          active
                            ? 'bg-slate-800 font-semibold text-emerald-400 border border-slate-700'
                            : 'text-slate-400 hover:bg-slate-800/60 hover:text-slate-200'
                        }`}
                      >
                        <span className="truncate pr-2">{tr.host}</span>
                        <span className="rounded-full bg-slate-800/80 px-2 py-0.5 text-[10px] text-slate-300 font-mono shrink-0">
                          {tr.count}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer link to Swagger */}
      <div className="border-t border-slate-800/80 pt-4">
        <a
          href="/swagger-ui"
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center space-x-2 text-xs text-slate-400 hover:text-brand-400 transition-colors px-3 py-2 rounded-lg hover:bg-slate-800/60"
        >
          <FileCode2 className="h-4 w-4" />
          <span>Swagger API Docs &rarr;</span>
        </a>
      </div>
    </aside>
  );
};
