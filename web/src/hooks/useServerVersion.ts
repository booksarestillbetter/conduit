// web/src/hooks/useServerVersion.ts
import { useEffect, useState } from 'react';
import { fetchHealth } from '../services/api';
import { APP_VERSION } from '../types';

// The version shown in the UI is the backend's own (from /api/health), so it can never drift
// from what is actually running. APP_VERSION, baked in at build time from web/package.json, is
// only the fallback until the server answers (or if it can't).
let current: string = APP_VERSION;
let requested = false;
const listeners = new Set<(v: string) => void>();

function load() {
  if (requested) return;
  requested = true;
  fetchHealth()
    .then((h) => {
      if (h.version && h.version !== current) {
        current = h.version;
        listeners.forEach((l) => l(current));
      }
    })
    .catch(() => {
      requested = false; // try again on the next mount
    });
}

export function useServerVersion(): string {
  const [version, setVersion] = useState(current);
  useEffect(() => {
    listeners.add(setVersion);
    setVersion(current);
    load();
    return () => {
      listeners.delete(setVersion);
    };
  }, []);
  return version;
}
