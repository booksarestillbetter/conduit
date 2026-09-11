// web/src/services/api.ts
import {
  AggregateStats,
  AppConfig,
  ApiTokenRecord,
  ArrGrabRecord,
  ArrStats,
  ArrTestConnectionResponse,
  DetailedTorrent,
  EventLogRecord,
  UnifiedTorrent,
  UserRecord,
} from '../types';

const API_BASE = '';

export function getStoredToken(): string | null {
  return localStorage.getItem('conduit_jwt');
}

function getAuthHeaders(): HeadersInit {
  const token = getStoredToken();
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
  };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }
  return headers;
}

/**
 * Central fetch wrapper. Every request in this module goes through here so a 401 is handled
 * once, consistently, instead of each of ~50 call sites silently leaving the UI in a stuck
 * state when a session expires. Only fires the session-expired flow when we actually had a
 * token to begin with — an anonymous 401 (e.g. a failed login attempt) is a normal, expected
 * response the caller already handles, not an expired session.
 */
async function apiFetch(input: string, init?: RequestInit): Promise<Response> {
  const hadToken = !!getStoredToken();
  const res = await fetch(input, init);
  if (res.status === 401 && hadToken) {
    localStorage.removeItem('conduit_jwt');
    window.dispatchEvent(new CustomEvent('conduit:session-expired'));
  }
  return res;
}

export async function fetchSetupStatus(): Promise<{ setup_needed: boolean }> {
  const res = await apiFetch(`${API_BASE}/api/auth/setup-status`);
  if (!res.ok) throw new Error('Failed to fetch setup status');
  return res.json();
}

export async function completeSetup(data: any): Promise<{ token: string; user: UserRecord }> {
  const res = await apiFetch(`${API_BASE}/api/auth/setup`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Setup failed');
  }
  const result = await res.json();
  localStorage.setItem('conduit_jwt', result.token);
  return result;
}

export async function login(
  username: string,
  password: string,
  totp_code?: string
): Promise<{ token?: string; user?: UserRecord; requires_2fa?: boolean; message?: string }> {
  const res = await apiFetch(`${API_BASE}/api/auth/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password, totp_code: totp_code || undefined }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Login failed');
  }
  const result = await res.json();
  if (result.token) {
    localStorage.setItem('conduit_jwt', result.token);
  }
  return result;
}

export async function logout(): Promise<void> {
  await apiFetch(`${API_BASE}/api/auth/logout`, { method: 'POST', headers: getAuthHeaders() }).catch(() => {});
  localStorage.removeItem('conduit_jwt');
}

export async function fetchMe(): Promise<UserRecord> {
  const res = await apiFetch(`${API_BASE}/api/auth/me`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Not authenticated');
  return res.json();
}

/** One-time, 30s-lived ticket for the /api/ws upgrade — see apiFetch's docs for why. */
export async function fetchWsTicket(): Promise<string> {
  const res = await apiFetch(`${API_BASE}/api/auth/ws-ticket`, { method: 'POST', headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to obtain WebSocket ticket');
  const data = await res.json();
  return data.ticket;
}

export async function updateProfile(username: string): Promise<{ success: boolean; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/auth/profile`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ username }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to update profile');
  }
  return res.json();
}

export async function changePassword(
  current_password: string,
  new_password: string
): Promise<{ success: boolean; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/auth/change-password`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ current_password, new_password }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to change password');
  }
  return res.json();
}

export async function setup2Fa(): Promise<{ secret: string; otpauth_url: string }> {
  const res = await apiFetch(`${API_BASE}/api/auth/2fa/setup`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to initialize 2FA setup');
  }
  return res.json();
}

export async function verify2Fa(code: string): Promise<{ success: boolean; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/auth/2fa/verify`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ code }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Invalid 2FA verification code');
  }
  return res.json();
}

export async function disable2Fa(password: string): Promise<{ success: boolean; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/auth/2fa/disable`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ password }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to disable 2FA');
  }
  return res.json();
}

export async function fetchTorrents(node?: string, search?: string, status?: string): Promise<UnifiedTorrent[]> {
  const params = new URLSearchParams();
  if (node && node !== 'all') params.append('node', node);
  if (search) params.append('search', search);
  if (status && status !== 'all') params.append('status', status);

  const res = await apiFetch(`${API_BASE}/api/torrents?${params.toString()}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch torrents');
  return res.json();
}

export async function fetchStats(): Promise<AggregateStats> {
  const res = await apiFetch(`${API_BASE}/api/torrents/stats`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch statistics');
  return res.json();
}

/** Long-range (up to 72h) one-minute-averaged bandwidth history, distinct from the live 5-minute
 * 1s-resolution `AggregateStats.bandwidth_history`. See `Database::insert_bandwidth_point`. */
export async function fetchBandwidthHistory(hours: number): Promise<import('../types').BandwidthDataPoint[]> {
  const res = await apiFetch(`${API_BASE}/api/torrents/bandwidth-history?hours=${hours}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch bandwidth history');
  return res.json();
}

export async function addTorrent(payload: {
  node: string;
  magnet_or_url?: string;
  metainfo_base64?: string;
  download_dir?: string;
  paused: boolean;
}): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/torrents`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to add torrent');
  }
  return res.json();
}

export async function migrateTorrent(payload: {
  source_node: string;
  target_node: string;
  hash: string;
  target_download_dir?: string;
  delete_source_torrent?: boolean;
  delete_source_data?: boolean;
}): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/torrents/migrate`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to migrate torrent');
  }
  return res.json();
}

export interface BulkItemResult {
  status: 'success' | 'error' | string;
  message: string;
}

export async function executeBulkAction(payload: {
  compound_ids: string[];
  action: string;
  delete_local_data?: boolean;
  target_directory?: string;
}): Promise<Record<string, BulkItemResult>> {
  const res = await apiFetch(`${API_BASE}/api/torrents/bulk`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error('Bulk action failed');
  return res.json();
}

export async function startTorrent(compoundId: string): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}/start`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to start torrent');
}

export async function stopTorrent(compoundId: string): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}/stop`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to pause torrent');
}

export async function deleteTorrent(compoundId: string, deleteData: boolean = false): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}?delete_data=${deleteData}`, {
    method: 'DELETE',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to delete torrent');
}

export async function enrichTorrent(compoundId: string): Promise<DetailedTorrent> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}/enrich`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || err.message || `Failed to enrich torrent (HTTP ${res.status})`);
  }
  return res.json();
}

export async function enrichAllTorrents(): Promise<{ status: string; total_scanned: number; processed_this_call: number; enriched_count: number; remaining_unenriched: number }> {
  const res = await apiFetch(`${API_BASE}/api/torrents/enrich-all`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || err.message || `Failed to batch enrich torrents (HTTP ${res.status})`);
  }
  return res.json();
}

export interface HealthStatus {
  status: string;
  service: string;
  dashboard_title?: string;
  version: string;
  greeting: string;
}

export async function fetchHealth(): Promise<HealthStatus> {
  const res = await apiFetch(`${API_BASE}/api/health`);
  if (!res.ok) throw new Error('Failed to fetch health status');
  return res.json();
}

export async function fetchSettings(): Promise<AppConfig> {
  const res = await apiFetch(`${API_BASE}/api/settings`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch settings');
  return res.json();
}

export async function saveSettings(config: AppConfig): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/settings`, {
    method: 'PUT',
    headers: getAuthHeaders(),
    body: JSON.stringify(config),
  });
  if (!res.ok) throw new Error('Failed to save settings');
}

export async function exportBackup(passphrase?: string): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/settings/backup`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ passphrase }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to export backup');
  }
  return res.json();
}

export async function restoreBackup(bundle: any, passphrase?: string): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/settings/restore`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ bundle, passphrase }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || 'Failed to restore backup');
  }
}

export async function fetchNodes(): Promise<any[]> {
  const res = await apiFetch(`${API_BASE}/api/nodes`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch nodes');
  return res.json();
}

export async function fetchNodeSession(nodeName: string): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/nodes/${encodeURIComponent(nodeName)}/session`, {
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to fetch node daemon session');
  return res.json();
}

export async function updateNodeSession(nodeName: string, settings: any): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/nodes/${encodeURIComponent(nodeName)}/session`, {
    method: 'PUT',
    headers: getAuthHeaders(),
    body: JSON.stringify(settings),
  });
  if (!res.ok) throw new Error('Failed to update node daemon session');
}

export async function testNodePort(nodeName: string): Promise<boolean> {
  const res = await apiFetch(`${API_BASE}/api/nodes/${encodeURIComponent(nodeName)}/test-port`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Port test failed');
  const data = await res.json();
  return data.port_is_open;
}

export async function fetchTokens(): Promise<ApiTokenRecord[]> {
  const res = await apiFetch(`${API_BASE}/api/auth/tokens`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch API tokens');
  return res.json();
}

export async function createToken(name: string, scopes: string[], expiresDays?: number): Promise<{ token_record: ApiTokenRecord; raw_token: string }> {
  const res = await apiFetch(`${API_BASE}/api/auth/tokens`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ name, scopes, expires_days: expiresDays }),
  });
  if (!res.ok) throw new Error('Failed to generate token');
  return res.json();
}

export async function deleteToken(id: string): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/auth/tokens/${id}`, {
    method: 'DELETE',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to revoke token');
}

export async function fetchEvents(params?: {
  limit?: number;
  level?: string;
  event_type?: string;
  q?: string;
}): Promise<EventLogRecord[]> {
  const query = new URLSearchParams();
  if (params?.limit) query.set('limit', params.limit.toString());
  if (params?.level) query.set('level', params.level);
  if (params?.event_type) query.set('event_type', params.event_type);
  if (params?.q) query.set('q', params.q);

  const res = await apiFetch(`${API_BASE}/api/events?${query.toString()}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch event logs');
  return res.json();
}

export async function fetchArrGrabs(params?: {
  q?: string;
  status?: string;
  limit?: number;
  zone?: string;
}): Promise<import('../types').ArrGrabRecord[]> {
  const query = new URLSearchParams();
  if (params?.q) query.set('q', params.q);
  if (params?.status) query.set('status', params.status);
  if (params?.limit) query.set('limit', params.limit.toString());
  if (params?.zone) query.set('zone', params.zone);

  const res = await apiFetch(`${API_BASE}/api/arr/grabs?${query.toString()}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch arr grabs');
  return res.json();
}

export async function fetchPipelineItems(params?: {
  app?: string;
  status?: string;
  q?: string;
  limit?: number;
  zone?: string;
}): Promise<import('../types').ArrGrabRecord[]> {
  const query = new URLSearchParams();
  if (params?.app) query.set('app', params.app);
  if (params?.status) query.set('status', params.status);
  if (params?.q) query.set('q', params.q);
  if (params?.limit) query.set('limit', params.limit.toString());
  if (params?.zone) query.set('zone', params.zone);

  const res = await apiFetch(`${API_BASE}/api/arr/pipeline?${query.toString()}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch pipeline items');
  return res.json();
}

export async function deletePipelineItem(id: string): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/arr/pipeline/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to delete pipeline item');
}

export async function purgePipelineArchive(app?: string): Promise<{ status: string; purged: number }> {
  const query = new URLSearchParams();
  if (app && app !== 'all') query.set('app', app);

  const res = await apiFetch(`${API_BASE}/api/arr/pipeline?${query.toString()}`, {
    method: 'DELETE',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to purge pipeline archive');
  return res.json();
}

export async function reSearchPipelineItem(id: string): Promise<{ status: string; executed: boolean; details: string }> {
  const res = await apiFetch(`${API_BASE}/api/arr/pipeline/${encodeURIComponent(id)}/re-search`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to trigger re-search');
  return res.json();
}

export async function fetchGrabHistory(id: string): Promise<import('../types').ArrGrabHistoryEntry[]> {
  const res = await apiFetch(`${API_BASE}/api/arr/pipeline/${encodeURIComponent(id)}/history`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch grab history');
  return res.json();
}

export async function fetchHookScript(format = 'sh', url?: string): Promise<string> {
  const query = new URLSearchParams();
  if (format) query.set('format', format);
  if (url) query.set('url', url);

  const res = await apiFetch(`${API_BASE}/api/sync/hook-script?${query.toString()}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch hook script');
  return res.text();
}

export async function testArrConnection(appType: string, baseUrl: string, apiKey: string): Promise<ArrTestConnectionResponse> {
  const res = await apiFetch(`${API_BASE}/api/arr/test-connection`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ app_type: appType, base_url: baseUrl, api_key: apiKey }),
  });
  if (!res.ok) throw new Error('Failed to perform connection test');
  return res.json();
}

export async function triggerArrSyncNow(appType: string): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/arr/sync-now`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ app_type: appType }),
  });
  if (!res.ok) throw new Error('Failed to trigger synchronization');
  return res.json();
}

export async function fetchArrStats(zone?: string): Promise<ArrStats> {
  const query = zone ? `?zone=${encodeURIComponent(zone)}` : '';
  const res = await apiFetch(`${API_BASE}/api/arr/stats${query}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch Arr stats');
  return res.json();
}

export async function testPlexConnection(url: string, token: string): Promise<{ success: boolean; message: string; server_name?: string; version?: string }> {
  const res = await apiFetch(`${API_BASE}/api/plex/test-connection`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ url, token }),
  });
  if (!res.ok) throw new Error('Failed to test Plex connection');
  return res.json();
}

export async function refreshPlexSections(url: string, token: string): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/plex/refresh`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ url, token }),
  });
  if (!res.ok) throw new Error('Failed to trigger Plex refresh');
  return res.json();
}

export interface PlexOAuthPin {
  pin_id: number;
  code: string;
  signin_url: string;
}

export async function createPlexOAuthPin(): Promise<PlexOAuthPin> {
  const res = await apiFetch(`${API_BASE}/api/plex/oauth/pin`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to start Plex account linking');
  return res.json();
}

export interface PlexDiscoveredServer {
  name: string;
  connections: { uri: string; local: boolean }[];
}

export interface PlexOAuthPollResult {
  authenticated: boolean;
  account_token?: string;
  servers?: PlexDiscoveredServer[];
}

export async function pollPlexOAuthPin(pinId: number): Promise<PlexOAuthPollResult> {
  const res = await apiFetch(`${API_BASE}/api/plex/oauth/poll/${pinId}`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to poll Plex account linking status');
  return res.json();
}

export async function addOAuthPlexServer(name: string, url: string, token: string): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/plex/oauth/add-server`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ name, url, token }),
  });
  if (!res.ok) throw new Error('Failed to add Plex server');
  return res.json();
}

export async function testTraktConnection(clientId: string, accessToken?: string): Promise<{ success: boolean; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/trakt/test-connection`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ client_id: clientId, access_token: accessToken }),
  });
  if (!res.ok) throw new Error('Failed to test Trakt credentials');
  return res.json();
}

export async function sendTestNotification(targetId: string): Promise<{ status: string; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/notifications/test`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ target_id: targetId }),
  });
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new Error(body?.message || 'Failed to dispatch test notification');
  }
  return res.json();
}

export async function testPipelineRegex(pattern: string, testString: string): Promise<{ matched: boolean; pattern_valid: boolean; error?: string }> {
  const res = await apiFetch(`${API_BASE}/api/pipeline/test-regex`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ pattern, test_string: testString }),
  });
  if (!res.ok) throw new Error('Failed to test regex pattern');
  return res.json();
}

export async function fetchTorrentDetails(compoundId: string): Promise<import('../types').DetailedTorrent> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}`, {
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to fetch torrent details');
  return res.json();
}

export async function fetchQueueRoutes(): Promise<import('../types').QueueRoutingConfig> {
  const res = await apiFetch(`${API_BASE}/api/sync/routes`, {
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to fetch queue routes');
  return res.json();
}

export async function classifyFile(payload: {
  name: string;
  hash?: string;
  node?: string;
  tracker?: string;
  trackers?: string[];
  dir?: string;
}): Promise<import('../types').ClassifyFileResponse> {
  const res = await apiFetch(`${API_BASE}/api/sync/classify`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error('Failed to classify file');
  return res.json();
}

export async function fetchTrackerMappings(): Promise<import('../types').TrackerMappingRule[]> {
  const res = await apiFetch(`${API_BASE}/api/sync/trackers`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch tracker mappings');
  return res.json();
}

export async function saveTrackerMappings(rules: import('../types').TrackerMappingRule[]): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/sync/trackers`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(rules),
  });
  if (!res.ok) throw new Error('Failed to save tracker mappings');
}

export async function fetchMediaTypes(): Promise<import('../types').MediaTypeDefinition[]> {
  const res = await apiFetch(`${API_BASE}/api/sync/media-types`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch media types');
  return res.json();
}

export async function saveMediaTypes(types: import('../types').MediaTypeDefinition[]): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/sync/media-types`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(types),
  });
  if (!res.ok) throw new Error('Failed to save media types');
}

export async function updateNodeMediaOverrides(nodeName: string, overrides: Record<string, string>): Promise<void> {
  const res = await apiFetch(`${API_BASE}/api/nodes/${encodeURIComponent(nodeName)}/media-overrides`, {
    method: 'PUT',
    headers: getAuthHeaders(),
    body: JSON.stringify(overrides),
  });
  if (!res.ok) throw new Error('Failed to update node media overrides');
}

export async function notifyDownload(payload: {
  hash: string;
  name: string;
  node?: string;
  path?: string;
  queue?: string;
  target_dir?: string;
}): Promise<{ status: string; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/sync/notify-download`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error('Failed to notify download');
  return res.json();
}

export async function fetchSystemHealth(): Promise<import('../types').SystemHealthOverview> {
  const res = await apiFetch(`${API_BASE}/api/system/health`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch system health');
  return res.json();
}

export async function fetchEngineHealth(): Promise<import('../types').EngineStatus[]> {
  const res = await apiFetch(`${API_BASE}/api/system/engines`, { headers: getAuthHeaders() });
  if (!res.ok) throw new Error('Failed to fetch engine health');
  return res.json();
}

export async function forceTripCircuitBreaker(host: string): Promise<{ message: string }> {
  const res = await apiFetch(`${API_BASE}/api/system/circuit-breakers/${encodeURIComponent(host)}/trip`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to force-trip circuit breaker');
  return res.json();
}

export async function forceResetCircuitBreaker(host: string): Promise<{ message: string }> {
  const res = await apiFetch(`${API_BASE}/api/system/circuit-breakers/${encodeURIComponent(host)}/reset`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to force-reset circuit breaker');
  return res.json();
}

export async function fetchPlexScrobbles(params?: {
  limit?: number;
  offset?: number;
  event?: string;
  user?: string;
}): Promise<{ items: import('../types').PlexScrobbleRecord[]; total: number; limit: number; offset: number }> {
  const query = new URLSearchParams();
  if (params?.limit) query.set('limit', params.limit.toString());
  if (params?.offset) query.set('offset', params.offset.toString());
  if (params?.event) query.set('event', params.event);
  if (params?.user) query.set('user', params.user);

  const res = await apiFetch(`${API_BASE}/api/plex/scrobbles?${query.toString()}`, {
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to fetch Plex scrobbles');
  return res.json();
}

export async function fetchOmbiRequests(params?: {
  limit?: number;
  offset?: number;
  media_type?: string;
  status?: string;
}): Promise<{ items: import('../types').OmbiRequestRecord[]; total: number; limit: number; offset: number }> {
  const query = new URLSearchParams();
  if (params?.limit) query.set('limit', params.limit.toString());
  if (params?.offset) query.set('offset', params.offset.toString());
  if (params?.media_type) query.set('media_type', params.media_type);
  if (params?.status) query.set('status', params.status);

  const res = await apiFetch(`${API_BASE}/api/ombi/requests?${query.toString()}`, {
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to fetch Ombi requests');
  return res.json();
}

export async function sendPlexWebhookTest(): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/plex/inbound`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      event: 'media.scrobble',
      user: { title: 'TestUser' },
      Account: { title: 'TestUser' },
      Metadata: {
        type: 'episode',
        title: 'Pilot Test',
        grandparentTitle: 'Conduit Test Series',
        parentIndex: 1,
        index: 1,
        year: 2026,
        duration: 3600000,
        viewOffset: 3500000,
        Guid: [{ id: 'imdb://tt9999999' }],
      },
    }),
  });
  if (!res.ok) throw new Error('Plex test webhook failed');
  return res.json();
}

export async function sendOmbiWebhookTest(): Promise<any> {
  const res = await apiFetch(`${API_BASE}/api/ombi/inbound`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      notificationType: 'NewRequest',
      requestedUser: 'TestUser',
      title: 'Conduit Test Movie',
      type: 'Movie',
      year: 2026,
      overview: 'A thrilling test request ingested into the Conduit pipeline.',
      theMovieDbId: 999999,
      status: 'pending',
    }),
  });
  if (!res.ok) throw new Error('Ombi test webhook failed');
  return res.json();
}

export async function moveTorrentQueue(compoundId: string, direction: 'top' | 'up' | 'down' | 'bottom'): Promise<{ status: string; direction: string }> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}/queue-move`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ direction }),
  });
  if (!res.ok) throw new Error(`Failed to move torrent queue (${direction})`);
  return res.json();
}

export async function moveBulkQueue(compoundIds: string[], direction: 'top' | 'up' | 'down' | 'bottom'): Promise<{ moved_torrents: number }> {
  const res = await apiFetch(`${API_BASE}/api/torrents/queue-move`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ compound_ids: compoundIds, direction }),
  });
  if (!res.ok) throw new Error(`Failed to execute bulk queue move (${direction})`);
  return res.json();
}

export async function setSequentialDownload(compoundId: string, enabled: boolean): Promise<{ sequential_download: boolean }> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}/sequential-download`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) throw new Error('Failed to update sequential download');
  return res.json();
}

export async function renameTorrentPath(compoundId: string, path: string, newName: string): Promise<{ path: string; new_name: string }> {
  const res = await apiFetch(`${API_BASE}/api/torrents/${encodeURIComponent(compoundId)}/rename-path`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ path, new_name: newName }),
  });
  if (!res.ok) throw new Error('Failed to rename torrent path');
  return res.json();
}

export async function batchReplaceTrackers(payload: { node?: string; compound_ids?: string[]; old_url: string; new_url: string }): Promise<{ replaced_torrents: number }> {
  const res = await apiFetch(`${API_BASE}/api/torrents/batch-replace-trackers`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify(payload),
  });
  if (!res.ok) throw new Error('Failed to replace trackers');
  return res.json();
}

export async function toggleTurtleMode(nodeName: string, enabled: boolean): Promise<{ alt_speed_enabled: boolean }> {
  const res = await apiFetch(`${API_BASE}/api/nodes/${encodeURIComponent(nodeName)}/turtle-mode`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) throw new Error('Failed to toggle turtle mode');
  return res.json();
}

export async function toggleTurtleModeAll(enabled: boolean): Promise<{ alt_speed_enabled: boolean; updated_nodes: string[] }> {
  const res = await apiFetch(`${API_BASE}/api/nodes/turtle-mode`, {
    method: 'POST',
    headers: getAuthHeaders(),
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) throw new Error('Failed to toggle global turtle mode');
  return res.json();
}

export async function updateNodeBlocklist(nodeName: string): Promise<{ blocklist_size: number; message: string }> {
  const res = await apiFetch(`${API_BASE}/api/nodes/${encodeURIComponent(nodeName)}/blocklist-update`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to update node blocklist');
  return res.json();
}

export async function createMobilePairToken(): Promise<{
  pair_code: string;
  qr_payload: string;
  server_url?: string;
  expires_in_secs: number;
}> {
  const res = await apiFetch(`${API_BASE}/api/auth/mobile/pair-token`, {
    method: 'POST',
    headers: getAuthHeaders(),
  });
  if (!res.ok) throw new Error('Failed to generate mobile pairing token');
  return res.json();
}



