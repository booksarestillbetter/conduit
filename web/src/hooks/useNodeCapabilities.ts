// web/src/hooks/useNodeCapabilities.ts
import { useCallback, useEffect, useState } from 'react';
import { fetchNodeCapabilities, NodeCapabilities } from '../services/api';

const REFRESH_MS = 60_000;

/**
 * What each node's daemon can do, so a control it cannot perform is disabled (with a reason)
 * instead of failing when clicked. Until the answer arrives, or for a node it doesn't list,
 * everything counts as supported: the server still refuses with a 501 and a message.
 */
export function useNodeCapabilities() {
  const [caps, setCaps] = useState<Record<string, NodeCapabilities>>({});

  useEffect(() => {
    let cancelled = false;
    const load = () => {
      fetchNodeCapabilities()
        .then((c) => {
          if (!cancelled) setCaps(c);
        })
        .catch(() => {
          // Keep the last answer; the server enforces regardless.
        });
    };
    load();
    const timer = setInterval(load, REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, []);

  const can = useCallback(
    (node: string | null | undefined, op: keyof NodeCapabilities): boolean => {
      if (!node) return true;
      const c = caps[node];
      return c ? c[op] : true;
    },
    [caps],
  );

  return { caps, can };
}
