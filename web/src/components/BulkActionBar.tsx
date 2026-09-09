// web/src/components/BulkActionBar.tsx
import React from 'react';
import { Play, Pause, CheckCircle2, RotateCcw, Trash2, FolderInput, X } from 'lucide-react';

interface BulkActionBarProps {
  selectedCount: number;
  onClearSelection: () => void;
  onBulkAction: (action: string) => void;
  onOpenDeleteModal: () => void;
  onQueueMove?: (direction: 'top' | 'up' | 'down' | 'bottom') => void;
}

export const BulkActionBar: React.FC<BulkActionBarProps> = ({
  selectedCount,
  onClearSelection,
  onBulkAction,
  onOpenDeleteModal,
  onQueueMove,
}) => {
  if (selectedCount === 0) return null;

  return (
    <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-40 flex items-center space-x-3 rounded-2xl border border-slate-700/80 bg-slate-900/95 px-5 py-3 shadow-2xl backdrop-blur-md animate-in fade-in slide-in-from-bottom-4 duration-200">
      <div className="flex items-center space-x-2 border-r border-slate-700 pr-4">
        <span className="flex h-6 w-6 items-center justify-center rounded-full bg-brand-500 text-xs font-bold text-white">
          {selectedCount}
        </span>
        <span className="text-sm font-medium text-slate-200">selected</span>
        <button
          onClick={onClearSelection}
          className="rounded p-1 text-slate-400 hover:bg-slate-800 hover:text-white"
          title="Clear Selection"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="flex items-center space-x-2">
        <button
          onClick={() => onBulkAction('start')}
          className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-emerald-400 hover:bg-slate-700 transition-colors"
        >
          <Play className="h-3.5 w-3.5" />
          <span>Resume All</span>
        </button>

        <button
          onClick={() => onBulkAction('stop')}
          className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-amber-400 hover:bg-slate-700 transition-colors"
        >
          <Pause className="h-3.5 w-3.5" />
          <span>Pause All</span>
        </button>

        {onQueueMove && (
          <div className="flex items-center space-x-1 border-l border-r border-slate-800 px-2">
            <button
              onClick={() => onQueueMove('top')}
              className="px-2 py-1.5 rounded bg-slate-800 text-[11px] font-bold text-slate-300 hover:bg-slate-700 hover:text-white"
              title="Move selected to top of queue"
            >
              ⇈ Top
            </button>
            <button
              onClick={() => onQueueMove('up')}
              className="px-2 py-1.5 rounded bg-slate-800 text-[11px] font-bold text-slate-300 hover:bg-slate-700 hover:text-white"
              title="Move selected up in queue"
            >
              ↑ Up
            </button>
            <button
              onClick={() => onQueueMove('down')}
              className="px-2 py-1.5 rounded bg-slate-800 text-[11px] font-bold text-slate-300 hover:bg-slate-700 hover:text-white"
              title="Move selected down in queue"
            >
              ↓ Down
            </button>
            <button
              onClick={() => onQueueMove('bottom')}
              className="px-2 py-1.5 rounded bg-slate-800 text-[11px] font-bold text-slate-300 hover:bg-slate-700 hover:text-white"
              title="Move selected to bottom of queue"
            >
              ⇊ Bottom
            </button>
          </div>
        )}

        <button
          onClick={() => onBulkAction('verify')}
          className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-sky-400 hover:bg-slate-700 transition-colors"
        >
          <CheckCircle2 className="h-3.5 w-3.5" />
          <span>Verify</span>
        </button>

        <button
          onClick={() => onBulkAction('reannounce')}
          className="flex items-center space-x-1.5 rounded-lg bg-slate-800 px-3 py-1.5 text-xs font-semibold text-purple-400 hover:bg-slate-700 transition-colors"
        >
          <RotateCcw className="h-3.5 w-3.5" />
          <span>Reannounce</span>
        </button>

        <button
          onClick={onOpenDeleteModal}
          className="flex items-center space-x-1.5 rounded-lg bg-rose-950/60 px-3 py-1.5 text-xs font-semibold text-rose-400 border border-rose-800/40 hover:bg-rose-900/60 transition-colors"
        >
          <Trash2 className="h-3.5 w-3.5" />
          <span>Delete...</span>
        </button>
      </div>
    </div>
  );
};
