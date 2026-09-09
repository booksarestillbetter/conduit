// web/src/App.tsx
import React, { useState, useEffect, useMemo, useCallback } from 'react';
import { Navbar } from './components/Navbar';
import { Sidebar } from './components/Sidebar';
import { TorrentTable } from './components/TorrentTable';
import { BulkActionBar } from './components/BulkActionBar';
import { AddTorrentModal } from './components/AddTorrentModal';
import { DeleteConfirmModal } from './components/DeleteConfirmModal';
import { Dashboard } from './pages/Dashboard';
import { SetupWizard } from './pages/SetupWizard';
import { Login } from './pages/Login';

// Code-split the heavier, less-frequently-visited pages out of the main bundle — Settings
// alone is ~3900 lines (the single biggest file in the app) and was previously shipped in the
// same chunk as the Dashboard landing view every visitor loads first. None of these are needed
// for first paint.
const Settings = React.lazy(() => import('./pages/Settings').then((m) => ({ default: m.Settings })));
const EventsLog = React.lazy(() => import('./pages/EventsLog').then((m) => ({ default: m.EventsLog })));
const Docs = React.lazy(() => import('./pages/Docs').then((m) => ({ default: m.Docs })));
const PipelineBrowserLazy = React.lazy(() => import('./pages/PipelineBrowser').then((m) => ({ default: m.PipelineBrowser })));

const PageLoadingFallback: React.FC = () => (
  <div className="flex-1 flex items-center justify-center p-12 text-slate-500">
    <span className="text-2xl animate-bounce inline-block mr-3">🐕</span>
    <span className="text-sm font-medium">Loading...</span>
  </div>
);
import { useTelemetry } from './hooks/useTelemetry';
import {
  fetchSetupStatus,
  fetchHealth,
  fetchMe,
  fetchSettings,
  logout,
  addTorrent,
  executeBulkAction,
  moveBulkQueue,
  startTorrent,
  stopTorrent,
  deleteTorrent,
} from './services/api';
import { TorrentDetailsModal } from './components/TorrentDetailsModal';
import { ProfileModal } from './components/ProfileModal';
import { ArchivedGrabsView } from './components/ArchivedGrabsView';
import { useToast } from './context/ToastContext';
import { UserRecord, ZoneConfig } from './types';

export const App: React.FC = () => {
  const toast = useToast();
  const [setupNeeded, setSetupNeeded] = useState<boolean | null>(null);
  const [dashboardTitle, setDashboardTitle] = useState<string>('Conduit');
  const [user, setUser] = useState<UserRecord | null>(null);
  const [loadingAuth, setLoadingAuth] = useState(true);
  const [isProfileModalOpen, setIsProfileModalOpen] = useState(false);

  // Client-side URL Routing State
  const [currentRoute, setCurrentRoute] = useState(() => window.location.pathname || '/dashboard');
  const [activeNode, setActiveNode] = useState(() => {
    const params = new URLSearchParams(window.location.search);
    return params.get('node') || 'all';
  });
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState('all');
  const [trackerFilter, setTrackerFilter] = useState('all');

  // Multi-tenant "zones" — an opt-in grouping of one Sonarr/Radarr/Lidarr + a subset of
  // Plex/fetcher nodes (e.g. "General Library" vs "4K Library"). Empty when the user
  // hasn't configured any, in which case the switcher is hidden and nothing changes.
  const [zones, setZones] = useState<ZoneConfig[]>([]);
  const [activeZone, setActiveZone] = useState('all');

  // Selection & Modals
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [isAddModalOpen, setIsAddModalOpen] = useState(false);
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null);
  const [isBulkDeleteModalOpen, setIsBulkDeleteModalOpen] = useState(false);
  const [inspectCompoundId, setInspectCompoundId] = useState<string | null>(null);

  // URL Navigation helper
  const navigate = useCallback((pathWithQuery: string) => {
    window.history.pushState(null, '', pathWithQuery);
    const [path, queryString] = pathWithQuery.split('?');
    setCurrentRoute(path || '/dashboard');
    if (queryString) {
      const params = new URLSearchParams(queryString);
      if (params.has('node')) {
        setActiveNode(params.get('node') || 'all');
      }
    }
  }, []);

  // Listen to browser Back/Forward navigation
  useEffect(() => {
    const handlePopState = () => {
      const path = window.location.pathname || '/dashboard';
      const params = new URLSearchParams(window.location.search);
      setCurrentRoute(path);
      setActiveNode(params.get('node') || 'all');
    };

    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, []);

  // Selected zone's fetcher nodes, or null (no filtering) when "All Zones" is active.
  const activeZoneNodeNames = useMemo(() => {
    if (activeZone === 'all') return null;
    const z = zones.find((z) => z.id === activeZone);
    if (!z) return null;
    return z.fetcher_node_names || z.transmission_node_names || null;
  }, [activeZone, zones]);

  // Telemetry & WebSocket Hook
  const { stats, torrents, statusCounts, trackerCounts, isConnected, pipelineItems, recentEvents, arrStats, engines } = useTelemetry(activeNode, search, statusFilter, trackerFilter, activeZoneNodeNames);

  useEffect(() => {
    checkInitialState();
    fetchSettings().then((cfg) => setZones(cfg.zones || [])).catch((e) => console.error('Failed to load zones:', e));
  }, []);

  // Custom dashboard title — read from the unauthenticated /api/health endpoint so it's
  // available on the login/setup screens too, not just once a session exists. Re-fetched on
  // 'settings-saved' so an admin editing it in Conduit Settings sees the navbar/tab title update
  // immediately rather than needing a full page reload.
  useEffect(() => {
    const loadTitle = () => {
      fetchHealth()
        .then((health) => {
          const title = health.dashboard_title?.trim() || 'Conduit';
          setDashboardTitle(title);
          document.title = `${title} - Media Fetching Commander`;
        })
        .catch((e) => console.error('Failed to load health/title:', e));
    };
    loadTitle();
    window.addEventListener('settings-saved', loadTitle);
    return () => window.removeEventListener('settings-saved', loadTitle);
  }, []);

  // Session-expiry handling: previously an expired/invalid token just made every subsequent API
  // call throw a generic error (often silently swallowed), leaving the UI stuck. api.ts dispatches
  // this event the moment any authenticated request comes back 401.
  useEffect(() => {
    const onSessionExpired = () => {
      setUser(null);
      toast.info('Your session expired — please sign in again.', 'Session Expired');
    };
    window.addEventListener('conduit:session-expired', onSessionExpired);
    return () => window.removeEventListener('conduit:session-expired', onSessionExpired);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const checkInitialState = async () => {
    try {
      const { setup_needed } = await fetchSetupStatus();
      setSetupNeeded(setup_needed);
      if (!setup_needed) {
        const me = await fetchMe().catch(() => null);
        setUser(me);
      }
    } catch (e) {
      console.error(e);
    } finally {
      setLoadingAuth(false);
    }
  };

  const torrentCounts = statusCounts;

  // useCallback + functional setState so this has a stable identity across renders — it's
  // passed down into TorrentTable's memoized row component, where a fresh function reference
  // every render would defeat the memoization (React.memo bails out only when every prop,
  // including callbacks, is reference-equal to the previous render).
  const handleToggleSelect = useCallback((compoundId: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(compoundId)) {
        next.delete(compoundId);
      } else {
        next.add(compoundId);
      }
      return next;
    });
  }, []);

  const handleSelectAll = (select: boolean) => {
    if (select) {
      setSelectedIds(new Set(torrents.map((t) => t.compound_id)));
    } else {
      setSelectedIds(new Set());
    }
  };

  const handleBulkAction = async (action: string) => {
    if (selectedIds.size === 0) return;
    let results;
    try {
      results = await executeBulkAction({
        compound_ids: Array.from(selectedIds),
        action,
      });
    } catch (e: any) {
      // The request itself failed (network/500) — no per-item results to report.
      toast.error(`Bulk action '${action}' failed: ${e.message}`, 'Action Failed');
      return;
    }
    setSelectedIds(new Set());
    try {
      reportBulkResults(action, results);
    } catch {
      // reportBulkResults already toasted the all-failed case; nothing further to do here.
    }
  };

  /**
   * Surfaces the real per-torrent outcome instead of assuming success just because the HTTP call
   * returned 200. Throws when every item failed so callers relying on this to gate a "did it
   * actually work" decision (e.g. DeleteConfirmModal staying open) behave correctly; a partial
   * failure is reported via toast but doesn't throw, since some real progress was made.
   */
  const reportBulkResults = (action: string, results: Record<string, { status: string; message: string }>) => {
    const entries = Object.entries(results);
    const failed = entries.filter(([, r]) => r.status !== 'success');
    const succeeded = entries.length - failed.length;

    if (failed.length === 0) {
      toast.success(`Executed '${action}' on ${succeeded} torrent${succeeded === 1 ? '' : 's'}`, '🐕 Bulk Action Completed');
    } else if (succeeded === 0) {
      const detail = failed[0]?.[1]?.message || 'Unknown error';
      toast.error(`'${action}' failed on all ${failed.length} torrent${failed.length === 1 ? '' : 's'}: ${detail}`, 'Bulk Action Failed');
      throw new Error(detail);
    } else {
      toast.error(
        `'${action}': ${succeeded} succeeded, ${failed.length} failed (e.g. ${failed[0][1].message})`,
        'Bulk Action Partially Failed'
      );
    }
  };

  const handleConfirmDelete = async (deleteLocalData: boolean) => {
    // Deliberately does NOT catch its own errors — DeleteConfirmModal relies on this throwing
    // to know a delete failed, so it can stay open and show the error instead of closing as if
    // the delete succeeded.
    if (deleteTargetId) {
      await deleteTorrent(deleteTargetId, deleteLocalData);
      setDeleteTargetId(null);
      toast.success('Removed torrent from node', '🐕 Conduit Removed Torrent');
    } else if (isBulkDeleteModalOpen && selectedIds.size > 0) {
      const results = await executeBulkAction({
        compound_ids: Array.from(selectedIds),
        action: 'delete',
        delete_local_data: deleteLocalData,
      });
      setSelectedIds(new Set());
      setIsBulkDeleteModalOpen(false);
      reportBulkResults('delete', results);
    }
  };

  const handleStartTorrent = useCallback(async (id: string) => {
    try {
      await startTorrent(id);
      toast.info('Torrent started', '🐕 Conduit Started');
    } catch (err: any) {
      toast.error(`Start failed: ${err.message}`, 'Error');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleStopTorrent = useCallback(async (id: string) => {
    try {
      await stopTorrent(id);
      toast.info('Torrent paused', '🐕 Conduit Paused');
    } catch (err: any) {
      toast.error(`Pause failed: ${err.message}`, 'Error');
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const handleRequestDelete = useCallback((id: string) => setDeleteTargetId(id), []);
  const handleViewTorrentDetails = useCallback((id: string) => setInspectCompoundId(id), []);

  const handleBulkQueueMove = async (direction: 'top' | 'up' | 'down' | 'bottom') => {
    const ids = Array.from(selectedIds);
    if (ids.length === 0) return;
    try {
      const res = await moveBulkQueue(ids, direction);
      toast.success(`Moved ${res.moved_torrents} fetchers (${direction})`, 'Queue Reordered');
    } catch (err: any) {
      toast.error(`Queue move failed: ${err.message}`, 'Action Failed');
    }
  };

  const handleLogout = async () => {
    await logout();
    setUser(null);
    toast.info('Logged out of Conduit', 'Session Ended');
  };

  const handleNodeSelect = (node: string) => {
    setActiveNode(node);
    if (node === 'all') {
      navigate('/fetchers');
    } else {
      navigate(`/fetchers?node=${encodeURIComponent(node)}`);
    }
  };

  if (loadingAuth) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-slate-950 text-slate-400">
        <div className="text-center space-y-3">
          <span className="text-4xl animate-bounce inline-block">🐕</span>
          <p className="text-sm font-medium">Connecting to Conduit...</p>
        </div>
      </div>
    );
  }

  if (setupNeeded) {
    return (
      <SetupWizard
        onComplete={(adminUser) => {
          setUser(adminUser);
          setSetupNeeded(false);
        }}
      />
    );
  }

  if (!user) {
    return <Login onLoginSuccess={setUser} />;
  }

  const isFetchersView = currentRoute.startsWith('/fetchers');
  const isPipelineView = currentRoute.startsWith('/pipeline');
  const isArchiveView = currentRoute.startsWith('/archive');
  const isDashboardView = currentRoute === '/' || currentRoute.startsWith('/dashboard');
  const isSettingsView = currentRoute.startsWith('/settings');
  const isLogsView = currentRoute.startsWith('/logs');
  const isDocsView = currentRoute.startsWith('/docs');

  return (
    <div className="dark flex flex-col h-screen overflow-hidden bg-slate-950 text-slate-100">
      <Navbar
        stats={stats}
        activeNode={activeNode}
        onNodeChange={handleNodeSelect}
        search={search}
        onSearchChange={setSearch}
        user={user}
        onLogout={handleLogout}
        onOpenAddModal={() => setIsAddModalOpen(true)}
        onOpenProfile={() => setIsProfileModalOpen(true)}
        onNavigateHome={() => navigate('/dashboard')}
        onNavigateToLogs={() => navigate('/logs')}
        dashboardTitle={dashboardTitle}
        isConnected={isConnected}
      />

      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          currentRoute={currentRoute}
          onNavigate={navigate}
          activeNode={activeNode}
          onSelectNode={handleNodeSelect}
          statusFilter={statusFilter}
          onSelectStatusFilter={setStatusFilter}
          trackerFilter={trackerFilter}
          onSelectTrackerFilter={setTrackerFilter}
          trackerCounts={trackerCounts}
          stats={stats}
          torrentCounts={torrentCounts}
          zones={zones}
          activeZone={activeZone}
          onSelectZone={setActiveZone}
        />

        <main className={`flex-1 flex flex-col overflow-hidden relative ${isFetchersView ? '' : 'p-4 md:p-6 overflow-y-auto'}`}>
          {isDashboardView && (
            <Dashboard
              stats={stats}
              pipelineItems={pipelineItems}
              recentEvents={recentEvents}
              arrStats={arrStats}
              engines={engines}
              zone={activeZone === 'all' ? undefined : activeZone}
              onNavigate={navigate}
              onViewDetails={(id) => setInspectCompoundId(id)}
            />
          )}

          {isFetchersView && (
            <TorrentTable
              torrents={torrents}
              selectedIds={selectedIds}
              onToggleSelect={handleToggleSelect}
              onSelectAll={handleSelectAll}
              onStart={handleStartTorrent}
              onStop={handleStopTorrent}
              onDelete={handleRequestDelete}
              onViewDetails={handleViewTorrentDetails}
            />
          )}

          {isArchiveView && <ArchivedGrabsView zone={activeZone === 'all' ? undefined : activeZone} />}

          {(isPipelineView || isSettingsView || isLogsView || isDocsView) && (
            <React.Suspense fallback={<PageLoadingFallback />}>
              {isPipelineView && (
                <PipelineBrowserLazy
                  onViewTorrent={handleViewTorrentDetails}
                  pipelineItems={pipelineItems}
                  zone={activeZone === 'all' ? undefined : activeZone}
                />
              )}
              {isSettingsView && <Settings />}
              {isLogsView && <EventsLog recentEvents={recentEvents} />}
              {isDocsView && <Docs />}
            </React.Suspense>
          )}

          {isFetchersView && (
            <BulkActionBar
              selectedCount={selectedIds.size}
              onClearSelection={() => setSelectedIds(new Set())}
              onBulkAction={handleBulkAction}
              onOpenDeleteModal={() => setIsBulkDeleteModalOpen(true)}
              onQueueMove={handleBulkQueueMove}
            />
          )}
        </main>
      </div>

      {isProfileModalOpen && user && (
        <ProfileModal
          user={user}
          onClose={() => setIsProfileModalOpen(false)}
          onUserUpdated={(updated) => setUser(updated)}
        />
      )}

      <TorrentDetailsModal
        compoundId={inspectCompoundId}
        onClose={() => setInspectCompoundId(null)}
      />

      <AddTorrentModal
        isOpen={isAddModalOpen}
        onClose={() => setIsAddModalOpen(false)}
        nodes={stats?.nodes || []}
        activeNode={activeNode}
        onAdd={async (payload) => {
          try {
            await addTorrent(payload);
            toast.success(`Torrent sent to node '${payload.node}'`, '🐕 Conduit Added Torrent');
          } catch (err: any) {
            toast.error(`Failed to add torrent: ${err.message}`, 'Add Failed');
            throw err;
          }
        }}
      />

      <DeleteConfirmModal
        isOpen={!!deleteTargetId || isBulkDeleteModalOpen}
        onClose={() => {
          setDeleteTargetId(null);
          setIsBulkDeleteModalOpen(false);
        }}
        count={deleteTargetId ? 1 : selectedIds.size}
        onConfirm={handleConfirmDelete}
      />
    </div>
  );
};
