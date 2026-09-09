// web/src/context/ToastContext.tsx
import React, { createContext, useContext, useState, useCallback } from 'react';
import { CheckCircle2, AlertTriangle, XCircle, Info, X } from 'lucide-react';

export type ToastType = 'success' | 'error' | 'info' | 'warning';

export interface ToastItem {
  id: string;
  type: ToastType;
  title?: string;
  message: string;
  duration?: number;
}

interface ToastContextType {
  toast: {
    success: (message: string, title?: string, duration?: number) => void;
    error: (message: string, title?: string, duration?: number) => void;
    info: (message: string, title?: string, duration?: number) => void;
    warning: (message: string, title?: string, duration?: number) => void;
    show: (item: Omit<ToastItem, 'id'>) => void;
    dismiss: (id: string) => void;
  };
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = useState<ToastItem[]>([]);

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const show = useCallback(
    ({ type, title, message, duration = 4000 }: Omit<ToastItem, 'id'>) => {
      const id = Math.random().toString(36).substring(2, 9);
      const newToast: ToastItem = { id, type, title, message, duration };

      setToasts((prev) => [...prev.slice(-4), newToast]); // keep max 5 toasts

      if (duration > 0) {
        setTimeout(() => {
          dismiss(id);
        }, duration);
      }
    },
    [dismiss]
  );

  const success = useCallback(
    (message: string, title?: string, duration?: number) => {
      show({ type: 'success', message, title, duration });
    },
    [show]
  );

  const error = useCallback(
    (message: string, title?: string, duration?: number) => {
      show({ type: 'error', message, title, duration: duration || 6000 });
    },
    [show]
  );

  const info = useCallback(
    (message: string, title?: string, duration?: number) => {
      show({ type: 'info', message, title, duration });
    },
    [show]
  );

  const warning = useCallback(
    (message: string, title?: string, duration?: number) => {
      show({ type: 'warning', message, title, duration: duration || 5000 });
    },
    [show]
  );

  const contextValue = {
    toast: { success, error, info, warning, show, dismiss },
  };

  return (
    <ToastContext.Provider value={contextValue}>
      {children}
      {/* Toast Overlay Container */}
      <div className="fixed bottom-5 right-5 z-50 flex flex-col space-y-2.5 max-w-md w-full pointer-events-none px-4 sm:px-0">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={`pointer-events-auto transform transition-all duration-300 ease-out flex items-start space-x-3 p-3.5 rounded-xl border shadow-2xl backdrop-blur-md animate-in slide-in-from-bottom-5 fade-in ${
              t.type === 'success'
                ? 'bg-slate-900/95 border-emerald-500/30 text-slate-100 shadow-emerald-950/20'
                : t.type === 'error'
                ? 'bg-slate-900/95 border-rose-500/40 text-slate-100 shadow-rose-950/30'
                : t.type === 'warning'
                ? 'bg-slate-900/95 border-amber-500/30 text-slate-100 shadow-amber-950/20'
                : 'bg-slate-900/95 border-sky-500/30 text-slate-100 shadow-sky-950/20'
            }`}
          >
            <div className="flex-shrink-0 mt-0.5">
              {t.type === 'success' && <CheckCircle2 className="h-5 w-5 text-emerald-400" />}
              {t.type === 'error' && <XCircle className="h-5 w-5 text-rose-400" />}
              {t.type === 'warning' && <AlertTriangle className="h-5 w-5 text-amber-400" />}
              {t.type === 'info' && <Info className="h-5 w-5 text-sky-400" />}
            </div>

            <div className="flex-1 min-w-0 pr-1">
              {t.title && <div className="text-xs font-bold text-slate-200">{t.title}</div>}
              <div className="text-xs text-slate-300 leading-relaxed break-words">{t.message}</div>
            </div>

            <button
              onClick={() => dismiss(t.id)}
              className="flex-shrink-0 rounded-lg p-1 text-slate-400 hover:text-slate-200 hover:bg-slate-800 transition-colors"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
};

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within a ToastProvider');
  }
  return context.toast;
};
