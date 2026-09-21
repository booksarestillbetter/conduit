import {
  ArrowDownCircle,
  ArrowUpCircle,
  PlusCircle,
  Search,
  LogOut,
  Server,
  Sparkles,
  ScrollText,
} from 'lucide-react';
import { AggregateStats, UserRecord } from '../types';
import { useServerVersion } from '../hooks/useServerVersion';
import { enrichAllTorrents, toggleTurtleModeAll } from '../services/api';

interface NavbarProps {
  stats: AggregateStats | null;
  activeNode: string;
  onNodeChange: (node: string) => void;
  search: string;
  onSearchChange: (search: string) => void;
  user: UserRecord | null;
  onLogout: () => void;
  onOpenAddModal: () => void;
  onOpenProfile: () => void;
  onNavigateHome: () => void;
  onNavigateToLogs?: () => void;
  isConnected: boolean;
  dashboardTitle?: string;
}

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec <= 0) return '0 B/s';
  const units = ['B/s', 'KB/s', 'MB/s', 'GB/s'];
  const i = Math.floor(Math.log(bytesPerSec) / Math.log(1024));
  return `${(bytesPerSec / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

export const Navbar: React.FC<NavbarProps> = ({
  stats,
  activeNode,
  onNodeChange,
  search,
  onSearchChange,
  user,
  onLogout,
  onOpenAddModal,
  onOpenProfile,
  onNavigateHome,
  onNavigateToLogs,
  isConnected,
  dashboardTitle,
}) => {
  const appVersion = useServerVersion();
  return (
    <header className="sticky top-0 z-30 flex h-16 w-full items-center justify-between border-b border-slate-800 bg-slate-900/80 px-6 backdrop-blur-md">
      {/* Brand & Node Selector */}
      <div className="flex items-center space-x-6">
        <button
          onClick={onNavigateHome}
          className="flex items-center space-x-3 cursor-pointer hover:opacity-90 transition-opacity text-left"
        >
          <span className="text-2xl">🐕</span>
          <span className="font-bold text-xl tracking-tight bg-gradient-to-r from-sky-400 to-blue-500 bg-clip-text text-transparent">
            {dashboardTitle || 'Conduit'}
          </span>
          <span className="rounded bg-slate-800/90 px-1.5 py-0.5 text-[10px] font-bold font-mono text-sky-400 border border-slate-700/80 shadow-xs">
            v{appVersion}
          </span>
          <div className="flex items-center space-x-1.5 rounded-full bg-slate-800/80 px-2.5 py-0.5 text-xs">
            <span className={`h-2 w-2 rounded-full ${isConnected ? 'bg-emerald-500 animate-pulse' : 'bg-rose-500'}`} />
            <span className="text-slate-400">{isConnected ? 'LIVE' : 'OFFLINE'}</span>
          </div>
        </button>

        {/* Node Dropdown */}
        <div className="relative flex items-center">
          <Server className="absolute left-3 h-4 w-4 text-slate-400 pointer-events-none" />
          <select
            value={activeNode}
            onChange={(e) => onNodeChange(e.target.value)}
            className="rounded-lg border border-slate-700 bg-slate-800/90 py-1.5 pl-9 pr-8 text-sm font-medium text-slate-200 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500 cursor-pointer"
          >
            <option value="all">All Nodes ({stats?.nodes.length || 0})</option>
            {stats?.nodes.map((n) => (
              <option key={n.node} value={n.node}>
                {n.node} {n.connected ? `(${n.total_torrents} fetchers)` : '(offline)'}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Speeds & Search */}
      <div className="flex items-center space-x-6">
        {/* Global Live Bandwidth Tickers */}
        <div className="hidden md:flex items-center space-x-4 bg-slate-950/60 rounded-xl px-4 py-1.5 border border-slate-800/80">
          <div className="flex items-center space-x-2 text-emerald-400 font-mono text-sm">
            <ArrowDownCircle className="h-4 w-4 animate-bounce" />
            <span className="font-semibold">{formatSpeed(stats?.total_download_speed || 0)}</span>
          </div>
          <div className="h-4 w-px bg-slate-800" />
          <div className="flex items-center space-x-2 text-sky-400 font-mono text-sm">
            <ArrowUpCircle className="h-4 w-4" />
            <span className="font-semibold">{formatSpeed(stats?.total_upload_speed || 0)}</span>
          </div>
        </div>

        {/* Search Bar */}
        <div className="relative w-64">
          <Search className="absolute left-3 top-2.5 h-4 w-4 text-slate-400" />
          <input
            type="text"
            value={search}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder="Search fetchers, hashes..."
            className="w-full rounded-lg border border-slate-700 bg-slate-800/90 py-1.5 pl-9 pr-4 text-sm text-slate-200 placeholder-slate-400 focus:border-brand-500 focus:outline-none focus:ring-1 focus:ring-brand-500"
          />
        </div>

        {/* Turtle Mode (Alt Speed Limits) Toggle */}
        <button
          onClick={async () => {
            try {
              const anyActive = stats?.nodes.some((n) => n.alt_speed_enabled);
              const target = !anyActive;
              await toggleTurtleModeAll(target);
            } catch (err: any) {
              alert(`Turtle mode toggle failed: ${err.message}`);
            }
          }}
          className={`flex items-center space-x-1.5 px-3 py-1.5 rounded-lg border text-xs font-semibold transition-all cursor-pointer ${
            stats?.nodes.some((n) => n.alt_speed_enabled)
              ? 'bg-amber-500/20 text-amber-300 border-amber-500/40 shadow-sm animate-pulse'
              : 'bg-slate-800/80 text-slate-400 hover:text-slate-200 border-slate-700/80'
          }`}
          title={
            stats?.nodes.some((n) => n.alt_speed_enabled)
              ? 'Turtle Mode is ON (Alternate speed limits applied). Click to disable.'
              : 'Click to enable Turtle Mode (Alternate speed limits on all nodes)'
          }
        >
          <span className="text-sm">🐢</span>
          <span className="hidden sm:inline">
            {stats?.nodes.some((n) => n.alt_speed_enabled) ? 'Turtle Active' : 'Turtle'}
          </span>
        </button>

        {/* Enrich All Button */}
        <button
          onClick={async () => {
            try {
              const res = await enrichAllTorrents();
              alert(`🐕 Enrichment Complete!\nScanned: ${res.total_scanned} fetchers\nEnriched: ${res.enriched_count} new media items.`);
            } catch (err: any) {
              alert(`Enrichment failed: ${err.message}`);
            }
          }}
          className="hidden sm:flex items-center space-x-1.5 px-3 py-1.5 rounded-lg bg-indigo-600/20 hover:bg-indigo-600/30 text-indigo-300 border border-indigo-500/30 text-xs font-semibold transition-colors cursor-pointer"
          title="Scan all active torrents and enrich metadata from Sonarr/Radarr"
        >
          <Sparkles className="h-3.5 w-3.5" />
          <span>Enrich All</span>
        </button>

        {/* Theme & User Profile */}
        <div className="flex items-center space-x-3 border-l border-slate-800 pl-4">
          {onNavigateToLogs && (
            <button
              onClick={onNavigateToLogs}
              className="rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200 transition-colors cursor-pointer"
              title="System Logs & Staging Activity"
            >
              <ScrollText className="h-4 w-4" />
            </button>
          )}

          <div className="flex items-center space-x-2">
            <button
              onClick={onOpenProfile}
              className="flex items-center space-x-1.5 px-2.5 py-1 rounded-lg bg-slate-800 text-slate-200 hover:bg-slate-700 hover:text-brand-400 transition-colors cursor-pointer border border-slate-700/60"
              title="Click to edit profile & security settings"
            >
              <span className="text-xs font-semibold">{user?.username || 'User'}</span>
              {user?.totp_enabled && (
                <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" title="2FA Active" />
              )}
            </button>
            <button
              onClick={onLogout}
              className="rounded-lg p-2 text-slate-400 hover:bg-rose-950/40 hover:text-rose-400 transition-colors"
              title="Logout"
            >
              <LogOut className="h-4 w-4" />
            </button>
          </div>
        </div>
      </div>
    </header>
  );
};
