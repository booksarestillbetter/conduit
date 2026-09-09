// web/src/components/GrabHistoryTimeline.tsx
import React, { useEffect, useState } from 'react';
import { Clock, RefreshCw } from 'lucide-react';
import { ArrGrabHistoryEntry } from '../types';
import { fetchGrabHistory } from '../services/api';

interface GrabHistoryTimelineProps {
  grabId: string;
}

function eventIcon(eventType: string): string {
  const t = eventType.toLowerCase();
  if (t.includes('delete')) return '🗑️';
  if (t.includes('download') || t.includes('import')) return '🏆';
  if (t.includes('research')) return '🔄';
  if (t.includes('grab')) return '🦴';
  if (t.includes('statuschange')) return '📋';
  return '📡';
}

function statusColor(status: string): string {
  const s = status.toLowerCase();
  if (s === 'imported' || s === 'downloaded') return 'text-emerald-400 border-emerald-500/20 bg-emerald-500/10';
  if (s === 'fetched') return 'text-sky-400 border-sky-500/20 bg-sky-500/10';
  if (s === 'deleted') return 'text-rose-400 border-rose-500/20 bg-rose-500/10';
  if (s === 'replaced' || s === 're-searched') return 'text-amber-400 border-amber-500/20 bg-amber-500/10';
  return 'text-slate-400 border-slate-700 bg-slate-800';
}

function formatSize(bytes?: number): string | null {
  if (!bytes || bytes <= 0) return null;
  const gb = bytes / (1024 * 1024 * 1024);
  return gb >= 1 ? `${gb.toFixed(2)} GB` : `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * The full grab/import/delete/re-search timeline for one canonical grab id — distinct from the
 * grab record itself, which collapses every event for the same media item onto one row (see
 * `arr_grab_history` on the backend). Used in the Ghost Archive and Media Detail views so a
 * trumped-and-replaced item's full lineage is visible, not just its current status.
 */
export const GrabHistoryTimeline: React.FC<GrabHistoryTimelineProps> = ({ grabId }) => {
  const [entries, setEntries] = useState<ArrGrabHistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetchGrabHistory(grabId)
      .then((data) => { if (!cancelled) setEntries(data); })
      .catch((e) => { if (!cancelled) setError(e.message || 'Failed to load history'); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [grabId]);

  if (loading) {
    return (
      <div className="flex items-center space-x-2 text-xs text-slate-500 py-3">
        <RefreshCw className="h-3.5 w-3.5 animate-spin" />
        <span>Loading timeline...</span>
      </div>
    );
  }

  if (error) {
    return <div className="text-xs text-rose-400 py-2">{error}</div>;
  }

  if (entries.length === 0) {
    return <div className="text-xs text-slate-500 py-2">No recorded history for this item yet.</div>;
  }

  return (
    <div className="space-y-2">
      {entries.length > 1 && (
        <div className="text-[10px] text-slate-500 font-sans">
          {entries.length} event{entries.length === 1 ? '' : 's'} recorded for this media item
        </div>
      )}
      <div className="space-y-1.5 max-h-64 overflow-y-auto pr-1">
        {entries.map((e) => (
          <div key={e.id} className="flex items-start space-x-2.5 rounded-lg border border-slate-800 bg-slate-950/60 p-2.5">
            <span className="text-base leading-none mt-0.5">{eventIcon(e.event_type)}</span>
            <div className="flex-1 min-w-0 space-y-1">
              <div className="flex items-center flex-wrap gap-1.5">
                <span className={`px-1.5 py-0.5 rounded text-[10px] font-bold uppercase border ${statusColor(e.status)}`}>
                  {e.status}
                </span>
                <span className="text-[10px] text-slate-500 font-mono">{e.event_type}</span>
                {e.quality && (
                  <span className="px-1.5 py-0.5 rounded-full text-[10px] font-semibold bg-slate-800 text-slate-300 border border-slate-700">
                    {e.quality}
                  </span>
                )}
                <span className="flex items-center space-x-1 text-[10px] text-slate-500 font-sans ml-auto shrink-0">
                  <Clock className="h-2.5 w-2.5" />
                  <span>{new Date(e.created_at).toLocaleString()}</span>
                </span>
              </div>
              {(e.release_title || e.scene_name) && (
                <div className="text-[11px] text-slate-300 font-mono truncate" title={e.release_title || e.scene_name}>
                  {e.release_title || e.scene_name}
                </div>
              )}
              <div className="flex items-center space-x-3 text-[10px] text-slate-500 font-sans flex-wrap gap-y-0.5">
                {e.indexer && <span>📡 {e.indexer}</span>}
                {formatSize(e.size_bytes) && <span>💾 {formatSize(e.size_bytes)}</span>}
                {e.download_client && <span>🐾 {e.download_client}</span>}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
};
