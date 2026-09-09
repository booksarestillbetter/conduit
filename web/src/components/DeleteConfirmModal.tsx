// web/src/components/DeleteConfirmModal.tsx
import React, { useState } from 'react';
import { AlertTriangle, Trash2, X } from 'lucide-react';

interface DeleteConfirmModalProps {
  isOpen: boolean;
  onClose: () => void;
  count: number;
  onConfirm: (deleteLocalData: boolean) => Promise<void>;
}

export const DeleteConfirmModal: React.FC<DeleteConfirmModalProps> = ({
  isOpen,
  onClose,
  count,
  onConfirm,
}) => {
  const [deleteLocalData, setDeleteLocalData] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleConfirm = async () => {
    setLoading(true);
    setError(null);
    try {
      await onConfirm(deleteLocalData);
      onClose();
    } catch (e: any) {
      // Stay open and show what went wrong instead of closing as if it worked.
      setError(e?.message || 'Delete failed. Please try again.');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
      <div className="w-full max-w-md rounded-2xl border border-rose-900/50 bg-slate-900 p-6 shadow-2xl animate-in zoom-in-95 duration-200">
        <div className="flex items-center space-x-3 text-rose-400 mb-4">
          <div className="flex h-10 w-10 items-center justify-center rounded-full bg-rose-950 border border-rose-800/50">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <h2 className="text-lg font-bold text-slate-100">
            Delete {count === 1 ? 'Torrent' : `${count} Torrents`}
          </h2>
        </div>

        <p className="text-sm text-slate-300">
          Are you sure you want to remove {count === 1 ? 'this torrent' : 'these torrents'} from the fetcher daemon?
        </p>

        <div className="mt-4 rounded-xl border border-rose-900/30 bg-rose-950/20 p-4">
          <label className="flex items-start space-x-3 cursor-pointer">
            <input
              type="checkbox"
              checked={deleteLocalData}
              onChange={(e) => setDeleteLocalData(e.target.checked)}
              className="mt-0.5 rounded border-rose-800 bg-slate-800 text-rose-600 focus:ring-rose-500 cursor-pointer"
            />
            <div className="text-sm">
              <span className="font-semibold text-rose-300">Also delete downloaded files from disk</span>
              <p className="text-xs text-rose-400/80 mt-0.5">
                This action is permanent and cannot be undone.
              </p>
            </div>
          </label>
        </div>

        {error && (
          <div className="mt-4 rounded-lg border border-rose-800/50 bg-rose-950/40 px-3 py-2 text-sm text-rose-300">
            {error}
          </div>
        )}

        <div className="flex items-center justify-end space-x-3 pt-6">
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg px-4 py-2 text-sm font-medium text-slate-400 hover:bg-slate-800 hover:text-white"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={loading}
            className="flex items-center space-x-2 rounded-lg bg-rose-600 px-5 py-2 text-sm font-semibold text-white shadow-lg shadow-rose-600/20 hover:bg-rose-500 disabled:opacity-50"
          >
            <Trash2 className="h-4 w-4" />
            <span>{loading ? 'Deleting...' : 'Delete'}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
