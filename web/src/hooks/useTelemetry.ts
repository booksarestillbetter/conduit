// web/src/hooks/useTelemetry.ts
import { useEffect, useState, useRef, useMemo } from 'react';
import { AggregateStats, UnifiedTorrent, ArrGrabRecord, EventLogRecord, ArrStats, EngineStatus } from '../types';
import { fetchStats, fetchTorrents, fetchWsTicket, fetchPipelineItems, fetchEvents, fetchArrStats, fetchEngineHealth } from '../services/api';
import { useToast } from '../context/ToastContext';

// Topics pushed over the same WebSocket connection as telemetry, opted into once on connect —
// see src/api/ws.rs and src/events.rs on the backend. `telemetry` itself (stats + torrents)
// stays unconditional/un-gated for backward compatibility, so it's not in this list.
const CHANNEL_TOPICS = ['pipeline', 'events', 'arr_stats', 'platform_health'];
const RECENT_EVENTS_CAP = 300;
const PIPELINE_ITEMS_CAP = 150;

const WS_RECONNECT_MIN_MS = 1000;
const WS_RECONNECT_MAX_MS = 30000;
// True fallback cadence while the WS is down; once connected this backs way off since the WS
// already pushes updates roughly every second — polling on top of that at the same rate was
// pure duplicate traffic for no freshness benefit.
const POLL_INTERVAL_DISCONNECTED_MS = 2000;
const POLL_INTERVAL_CONNECTED_MS = 30000;

// Global in-memory cache to eliminate loading flashes on route changes
let globalCachedStats: AggregateStats | null = null;
let globalCachedTorrents: UnifiedTorrent[] = [];

export function useTelemetry(
  activeNode: string,
  search: string,
  statusFilter: string,
  trackerFilter: string = 'all',
  // Node names belonging to the selected zone (see ZoneConfig), or null for "no zone selected" /
  // combined view — an additional filter layered on top of activeNode, same as trackerFilter.
  activeZoneNodeNames: string[] | null = null
) {
  const toast = useToast();
  const [stats, setStats] = useState<AggregateStats | null>(() => globalCachedStats);
  const [allTorrents, setAllTorrents] = useState<UnifiedTorrent[]>(() => globalCachedTorrents);
  const [pipelineItems, setPipelineItems] = useState<ArrGrabRecord[]>([]);
  const [recentEvents, setRecentEvents] = useState<EventLogRecord[]>([]);
  const [arrStats, setArrStats] = useState<ArrStats | null>(null);
  const [engines, setEngines] = useState<EngineStatus[] | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const isConnectedRef = useRef(false);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimeoutRef = useRef<any>(null);
  const reconnectDelayRef = useRef(WS_RECONNECT_MIN_MS);
  const initialLoadRef = useRef(false);
  const knownTorrentsRef = useRef<Map<string, { percent_done: number; status: string; title: string }>>(new Map());

  // 1. Establish and maintain persistent WebSocket connection
  useEffect(() => {
    let unmounted = false;
    let stopReconnecting = false;

    const scheduleReconnect = (connectFn: () => void) => {
      if (unmounted || stopReconnecting) return;
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = setTimeout(connectFn, reconnectDelayRef.current);
      reconnectDelayRef.current = Math.min(reconnectDelayRef.current * 2, WS_RECONNECT_MAX_MS);
    };

    const onSessionExpired = () => {
      // No point hammering ws-ticket with an expired session — stop until the user logs back
      // in (a fresh mount of this hook, e.g. after re-login, starts the whole cycle over).
      stopReconnecting = true;
      clearTimeout(reconnectTimeoutRef.current);
      if (wsRef.current) wsRef.current.close();
    };
    window.addEventListener('conduit:session-expired', onSessionExpired);

    const connectWs = async () => {
      if (unmounted || stopReconnecting) return;

      let ticket: string;
      try {
        ticket = await fetchWsTicket();
      } catch {
        scheduleReconnect(connectWs);
        return;
      }
      if (unmounted || stopReconnecting) return;

      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = `${protocol}//${window.location.host}/api/ws?ticket=${encodeURIComponent(ticket)}`;

      try {
        const ws = new WebSocket(wsUrl);
        wsRef.current = ws;

        ws.onopen = () => {
          if (!unmounted) {
            setIsConnected(true);
            isConnectedRef.current = true;
            reconnectDelayRef.current = WS_RECONNECT_MIN_MS;
            ws.send(JSON.stringify({ type: 'subscribe', topics: CHANNEL_TOPICS }));
          }
        };

        ws.onmessage = (event) => {
          if (unmounted) return;
          try {
            const data = JSON.parse(event.data);
            if (data.type === 'channel') {
              if (data.topic === 'pipeline' && data.data) {
                const updated = data.data as ArrGrabRecord;
                setPipelineItems((prev) => {
                  const next = prev.filter((p) => p.id !== updated.id);
                  next.unshift(updated);
                  return next.slice(0, PIPELINE_ITEMS_CAP);
                });
              } else if (data.topic === 'events' && data.data) {
                const evt = data.data as EventLogRecord;
                setRecentEvents((prev) => [evt, ...prev].slice(0, RECENT_EVENTS_CAP));
              } else if (data.topic === 'arr_stats' && data.data) {
                setArrStats(data.data as ArrStats);
              } else if (data.topic === 'platform_health' && Array.isArray(data.data)) {
                setEngines(data.data as EngineStatus[]);
              }
            } else if (data.type === 'telemetry') {
              if (data.stats) {
                globalCachedStats = data.stats;
                setStats(data.stats);
              }
              if (Array.isArray(data.torrents)) {
                if (!initialLoadRef.current) {
                  initialLoadRef.current = true;
                  for (const t of data.torrents) {
                    knownTorrentsRef.current.set(t.compound_id, {
                      percent_done: t.percent_done,
                      status: t.status,
                      title: t.arr_grab?.title || t.name,
                    });
                  }
                } else {
                  for (const t of data.torrents) {
                    const prev = knownTorrentsRef.current.get(t.compound_id);
                    const title = t.arr_grab?.title || t.name;
                    const indexer = t.arr_grab?.indexer ? ` • ${t.arr_grab.indexer}` : '';

                    if (!prev) {
                      toast.info(`🐕 Conduit Sniffed: ${title}${indexer}`, 'New Content Intake');
                    } else if (prev.percent_done < 1.0 && t.percent_done >= 1.0) {
                      toast.success(`🐕 Conduit Retrieved: ${title}`, 'Download Complete');
                    }

                    knownTorrentsRef.current.set(t.compound_id, {
                      percent_done: t.percent_done,
                      status: t.status,
                      title,
                    });
                  }
                }

                setAllTorrents((prev) => {
                  const grabMap = new Map<string, any>();
                  for (const t of prev) {
                    if (t.arr_grab) grabMap.set(t.compound_id, t.arr_grab);
                  }
                  const merged = data.torrents.map((t: UnifiedTorrent) => ({
                    ...t,
                    arr_grab: t.arr_grab || grabMap.get(t.compound_id),
                  }));
                  globalCachedTorrents = merged;
                  return merged;
                });
              }
            }
          } catch (e) {
            console.error('Failed to parse telemetry message:', e);
          }
        };

        ws.onclose = () => {
          if (!unmounted) {
            setIsConnected(false);
            isConnectedRef.current = false;
            scheduleReconnect(connectWs);
          }
        };

        ws.onerror = () => {
          if (ws.readyState === WebSocket.OPEN) {
            ws.close();
          }
        };
      } catch (err) {
        if (!unmounted) {
          setIsConnected(false);
          isConnectedRef.current = false;
          scheduleReconnect(connectWs);
        }
      }
    };

    connectWs();

    // 2. Fallback polling loop. True fallback cadence (2s) while the WS is down; once connected
    // this backs off to a slow safety net (30s) instead of duplicating what the WS already pushes
    // roughly every second. Also doubles as the instant-first-paint fetch (runs immediately on
    // mount, before the WS ticket round trip even completes) for the pipeline/events/arr-stats/
    // platform-health channels below, each of which otherwise only updates via WS push.
    let pollTimeout: any;
    const poll = async () => {
      try {
        const [st, tr] = await Promise.all([fetchStats(), fetchTorrents()]);
        if (!unmounted) {
          if (st) {
            globalCachedStats = st;
            setStats(st);
          }
          if (Array.isArray(tr)) {
            setAllTorrents((prev) => {
              const grabMap = new Map<string, any>();
              for (const t of prev) {
                if (t.arr_grab) grabMap.set(t.compound_id, t.arr_grab);
              }
              const merged = tr.map((t: UnifiedTorrent) => ({
                ...t,
                arr_grab: t.arr_grab || grabMap.get(t.compound_id),
              }));
              globalCachedTorrents = merged;
              return merged;
            });
          }
        }
      } catch (e) {
        // Silently handle poll errors if unauthenticated or network hiccup
      } finally {
        if (!unmounted) {
          const delay = isConnectedRef.current ? POLL_INTERVAL_CONNECTED_MS : POLL_INTERVAL_DISCONNECTED_MS;
          pollTimeout = setTimeout(poll, delay);
        }
      }
    };

    poll(); // Initial poll, then self-schedules based on connection state

    // These four are cheap local-DB/cache reads on the backend (not the live external-API
    // calls torrents/stats used to require) — same disconnected-only cadence isn't needed, so
    // fetch once for instant paint and otherwise just rely on the WS channel push above.
    Promise.allSettled([
      fetchPipelineItems({ limit: PIPELINE_ITEMS_CAP }).then((items) => { if (!unmounted && Array.isArray(items)) setPipelineItems(items); }),
      fetchEvents({ limit: RECENT_EVENTS_CAP }).then((items) => { if (!unmounted && Array.isArray(items)) setRecentEvents(items); }),
      fetchArrStats().then((s) => { if (!unmounted) setArrStats(s); }),
      fetchEngineHealth().then((e) => { if (!unmounted && Array.isArray(e)) setEngines(e); }),
    ]).catch(() => {});

    return () => {
      unmounted = true;
      window.removeEventListener('conduit:session-expired', onSessionExpired);
      clearTimeout(pollTimeout);
      clearTimeout(reconnectTimeoutRef.current);
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, []);

  // Compute status counts across all node-filtered torrents (independent of statusFilter or search)
  const statusCounts = useMemo(() => {
    let nodeTorrents = allTorrents;
    if (activeNode && activeNode !== 'all') {
      nodeTorrents = nodeTorrents.filter((t) => t.node === activeNode);
    }
    if (activeZoneNodeNames) {
      nodeTorrents = nodeTorrents.filter((t) => activeZoneNodeNames.includes(t.node));
    }
    if (trackerFilter && trackerFilter !== 'all') {
      const tf = trackerFilter.toLowerCase();
      nodeTorrents = nodeTorrents.filter((t) =>
        (t.tracker_stats || []).some((tr) => (tr.host && tr.host.toLowerCase() === tf) || (tr.announce && tr.announce.toLowerCase().includes(tf)))
      );
    }

    return {
      all: nodeTorrents.length,
      downloading: nodeTorrents.filter((t) => t.status === 'downloading').length,
      seeding: nodeTorrents.filter((t) => t.status === 'seeding').length,
      active: nodeTorrents.filter((t) => t.rate_download > 0 || t.rate_upload > 0).length,
      queued: nodeTorrents.filter((t) => t.status === 'queued' || t.status === 'queuedseed' || t.status === 'checkwait').length,
      paused: nodeTorrents.filter((t) => t.status === 'stopped').length,
      checking: nodeTorrents.filter((t) => t.status === 'checking' || t.status === 'checkwait').length,
      error: nodeTorrents.filter((t) => t.status === 'error' || t.error > 0).length,
    };
  }, [allTorrents, activeNode, trackerFilter, activeZoneNodeNames]);

  // Compute tracker counts across all node-filtered torrents
  const trackerCounts = useMemo(() => {
    let nodeTorrents = allTorrents;
    if (activeNode && activeNode !== 'all') {
      nodeTorrents = nodeTorrents.filter((t) => t.node === activeNode);
    }
    if (activeZoneNodeNames) {
      nodeTorrents = nodeTorrents.filter((t) => activeZoneNodeNames.includes(t.node));
    }

    const counts: Record<string, number> = {};
    for (const t of nodeTorrents) {
      const seenForThisTorrent = new Set<string>();
      for (const tr of t.tracker_stats || []) {
        const host = tr.host ? tr.host.toLowerCase().trim() : '';
        if (host && !seenForThisTorrent.has(host)) {
          seenForThisTorrent.add(host);
          counts[host] = (counts[host] || 0) + 1;
        }
      }
    }

    return Object.entries(counts)
      .map(([host, count]) => ({ host, count }))
      .sort((a, b) => b.count - a.count);
  }, [allTorrents, activeNode, activeZoneNodeNames]);

  // 3. Fast, non-blocking client-side memoized filtering
  const torrents = useMemo(() => {
    let list = allTorrents;

    if (activeNode && activeNode !== 'all') {
      list = list.filter((t) => t.node === activeNode);
    }

    if (activeZoneNodeNames) {
      list = list.filter((t) => activeZoneNodeNames.includes(t.node));
    }

    if (statusFilter && statusFilter !== 'all') {
      list = list.filter((t) => {
        if (statusFilter === 'downloading') return t.status === 'downloading';
        if (statusFilter === 'seeding') return t.status === 'seeding';
        if (statusFilter === 'queued') return t.status === 'queued' || t.status === 'queuedseed' || t.status === 'checkwait';
        if (statusFilter === 'paused') return t.status === 'stopped';
        if (statusFilter === 'error') return t.status === 'error' || t.error > 0;
        if (statusFilter === 'checking') return t.status === 'checking' || t.status === 'checkwait';
        if (statusFilter === 'active') return t.rate_download > 0 || t.rate_upload > 0;
        return true;
      });
    }

    if (trackerFilter && trackerFilter !== 'all') {
      const tf = trackerFilter.toLowerCase();
      list = list.filter((t) =>
        (t.tracker_stats || []).some((tr) => (tr.host && tr.host.toLowerCase() === tf) || (tr.announce && tr.announce.toLowerCase().includes(tf)))
      );
    }

    if (search.trim()) {
      const s = search.toLowerCase();
      list = list.filter((t) =>
        t.name.toLowerCase().includes(s) ||
        t.hash_string.toLowerCase().includes(s) ||
        t.download_dir.toLowerCase().includes(s) ||
        t.tracker_stats.some((tr) => tr.host.toLowerCase().includes(s) || tr.announce.toLowerCase().includes(s))
      );
    }

    return list;
  }, [allTorrents, activeNode, search, statusFilter, trackerFilter, activeZoneNodeNames]);

  return { stats, torrents, allTorrents, statusCounts, trackerCounts, isConnected, pipelineItems, recentEvents, arrStats, engines };
}
