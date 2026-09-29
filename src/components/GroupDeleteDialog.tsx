import { useState } from "react";
import type { GroupDeleteStrategy } from "../api/types";
import { Modal } from "./Modal";

export function GroupDeleteDialog({
  groupName,
  itemNoun,
  onConfirm,
  onCancel,
}: {
  groupName: string;
  itemNoun: string;
  onConfirm: (strategy: GroupDeleteStrategy) => Promise<void> | void;
  onCancel: () => void;
}) {
  const [strategy, setStrategy] = useState<GroupDeleteStrategy>("reassign_to_parent");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);

  const needsStrongConfirm = strategy === "delete_subtree";

  const submit = async () => {
    setBusy(true);
    try {
      await onConfirm(strategy);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={`Delete "${groupName}"`} onClose={onCancel} width={480}>
      <p>This group may contain subgroups and {itemNoun}. Choose what should happen to them.</p>
      <label className="radio-row">
        <input
          type="radio"
          checked={strategy === "reassign_to_parent"}
          onChange={() => {
            setStrategy("reassign_to_parent");
            setConfirmed(false);
          }}
        />
        <span>Move its subgroups and {itemNoun} up to the parent group (or root)</span>
      </label>
      <label className="radio-row">
        <input
          type="radio"
          checked={strategy === "delete_subtree"}
          onChange={() => {
            setStrategy("delete_subtree");
            setConfirmed(false);
          }}
        />
        <span>Delete this group and everything inside it, including nested groups</span>
      </label>
      {needsStrongConfirm && (
        <label className="checkbox-row">
          <input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
          I understand this permanently deletes all {itemNoun} in this group and its subgroups.
        </label>
      )}
      <div className="modal-actions">
        <button onClick={onCancel} disabled={busy}>
          Cancel
        </button>
        <button
          className="danger"
          disabled={busy || (needsStrongConfirm && !confirmed)}
          onClick={submit}
        >
          Delete
        </button>
      </div>
    </Modal>
  );
}
