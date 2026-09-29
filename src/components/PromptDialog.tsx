import { useState } from "react";
import { Modal } from "./Modal";

export function PromptDialog({
  title,
  label,
  initialValue = "",
  confirmLabel = "Save",
  onConfirm,
  onCancel,
}: {
  title: string;
  label: string;
  initialValue?: string;
  confirmLabel?: string;
  onConfirm: (value: string) => Promise<void> | void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initialValue);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (!value.trim()) {
      setError("This field is required.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onConfirm(value.trim());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={title} onClose={onCancel} width={400}>
      <label className="field">
        <span>{label}</span>
        <input
          autoFocus
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
      </label>
      {error && <div className="field-error">{error}</div>}
      <div className="modal-actions">
        <button onClick={onCancel} disabled={busy}>
          Cancel
        </button>
        <button className="primary" onClick={submit} disabled={busy}>
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}
