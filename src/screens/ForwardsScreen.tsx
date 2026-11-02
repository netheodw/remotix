import { useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import {
  forwardsDelete,
  forwardsDuplicate,
  forwardsMoveToGroup,
  forwardsStart,
  forwardsStop,
  portForwardGroupsCreate,
  portForwardGroupsDelete,
  portForwardGroupsUpdate,
} from "../api/commands";
import { errorMessage } from "../api/errors";
import { ConfirmDialog } from "../components/Modal";
import { GroupDeleteDialog } from "../components/GroupDeleteDialog";
import { descendantIds, GroupTreeView, startItemDrag, useGroupTree, type GroupTreeNode } from "../components/GroupTree";
import { dragPayload, moveItems, MoveToGroupSelect, useRowSelection } from "../components/MoveToGroup";
import { PromptDialog } from "../components/PromptDialog";
import { ForwardStatusBadge } from "../components/StatusBadge";
import type { PortForward } from "../api/types";
import { useData } from "../state/DataContext";
import { useForwardStatus, useForwardStatuses } from "../state/ForwardStatusContext";
import { ForwardEditorModal } from "./ForwardEditorModal";

function endpointLabel(f: PortForward): { source: string; destination: string | null } {
  if (f.forward_type === "remote") {
    return { source: `remote:${f.listen_port}`, destination: `${f.target_hostname}:${f.target_port}` };
  }
  if (f.forward_type === "dynamic") {
    return { source: `${f.bind_address}:${f.listen_port} (SOCKS5)`, destination: null };
  }
  if (f.target_port === null) {
    return { source: `${f.bind_address}:${f.listen_port}`, destination: `unix:${f.target_hostname}` };
  }
  return { source: `${f.bind_address}:${f.listen_port}`, destination: `${f.target_hostname}:${f.target_port}` };
}

function ForwardRow({
  f,
  profileName,
  selected,
  onToggleSelect,
  onDragStart,
  onEdit,
  onDelete,
  onDuplicate,
}: {
  f: PortForward;
  profileName: string;
  selected: boolean;
  onToggleSelect: () => void;
  onDragStart: (e: DragEvent) => void;
  onEdit: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
}) {
  const status = useForwardStatus(f.id);
  const endpoints = endpointLabel(f);
  const running = status.status !== "stopped" && status.status !== "failed";

  return (
    <tr draggable className={selected ? "selected" : undefined} onDragStart={onDragStart}>
      <td className="select-col">
        <input type="checkbox" aria-label={`Select ${f.name}`} checked={selected} onChange={onToggleSelect} />
      </td>
      <td>{f.name}</td>
      <td>{f.forward_type}</td>
      <td>{endpoints.source}</td>
      <td>{endpoints.destination ?? "per-request"}</td>
      <td>{f.ssh_profile_id ? profileName : <span className="field-error">Needs Profile</span>}</td>
      <td>
        <ForwardStatusBadge status={status} />
      </td>
      <td className="row-actions">
        {running ? (
          <button onClick={() => forwardsStop(f.id)}>Stop</button>
        ) : (
          <button className="primary" disabled={!f.ssh_profile_id} onClick={() => forwardsStart(f.id)}>
            Start
          </button>
        )}
        <button onClick={onEdit}>Edit</button>
        <button onClick={onDuplicate}>Duplicate</button>
        <button className="danger" onClick={onDelete}>
          Delete
        </button>
      </td>
    </tr>
  );
}

export function ForwardsScreen({ createSignal }: { createSignal: number }) {
  const { portForwardGroups, forwards, profiles, refreshPortForwardGroups, refreshForwards } = useData();
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [editingForward, setEditingForward] = useState<PortForward | "new" | null>(null);
  const lastCreateSignal = useRef(createSignal);
  useEffect(() => {
    if (createSignal !== lastCreateSignal.current) {
      lastCreateSignal.current = createSignal;
      if (profiles.length > 0) setEditingForward("new");
    }
  }, [createSignal, profiles.length]);
  const [renamingGroup, setRenamingGroup] = useState<GroupTreeNode | "new-root" | { parentId: string } | null>(null);
  const [deletingGroup, setDeletingGroup] = useState<GroupTreeNode | null>(null);
  const [deletingForward, setDeletingForward] = useState<PortForward | null>(null);

  const itemCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    forwards.forEach((f) => {
      if (f.port_forward_group_id) counts[f.port_forward_group_id] = (counts[f.port_forward_group_id] ?? 0) + 1;
    });
    return counts;
  }, [forwards]);
  const tree = useGroupTree(portForwardGroups, itemCounts);

  const profileName = (id: string | null) => profiles.find((p) => p.id === id)?.name ?? "Unknown";

  const visibleForwards = useMemo(() => {
    if (!selectedGroupId) return forwards;
    const ids = descendantIds(portForwardGroups, selectedGroupId);
    return forwards.filter((f) => f.port_forward_group_id && ids.has(f.port_forward_group_id));
  }, [forwards, selectedGroupId, portForwardGroups]);

  const visibleIds = useMemo(() => visibleForwards.map((f) => f.id), [visibleForwards]);
  const selection = useRowSelection(visibleIds);
  const [moveError, setMoveError] = useState<string | null>(null);

  const moveForwards = async (ids: string[], groupId: string | null) => {
    setMoveError(null);
    const groupOf = new Map(forwards.map((f) => [f.id, f.port_forward_group_id]));
    try {
      await moveItems(ids, groupId, (id) => groupOf.get(id), forwardsMoveToGroup);
      selection.clear();
    } catch (e) {
      setMoveError(errorMessage(e));
    } finally {
      await refreshForwards();
    }
  };

  // Bulk actions apply to everything in the current view: the selected group and all of its subgroups.
  const statuses = useForwardStatuses();
  const { startable, running, needsProfile } = useMemo(() => {
    const isRunning = (f: PortForward) => {
      const s = statuses[f.id]?.status ?? "stopped";
      return s !== "stopped" && s !== "failed";
    };
    return {
      startable: visibleForwards.filter((f) => !isRunning(f) && f.ssh_profile_id),
      running: visibleForwards.filter(isRunning),
      needsProfile: visibleForwards.filter((f) => !isRunning(f) && !f.ssh_profile_id).length,
    };
  }, [visibleForwards, statuses]);
  const scopeLabel = selectedGroupId
    ? `"${portForwardGroups.find((g) => g.id === selectedGroupId)?.name ?? "group"}" and its subgroups`
    : "all forwards";

  return (
    <div className="split-screen">
      <div className="tree-pane">
        <div className="tree-pane-header">
          <h3>Port Forward Groups</h3>
          <button className="ghost" title="New root group" onClick={() => setRenamingGroup("new-root")}>
            +
          </button>
        </div>
        <div className={`group-row ${selectedGroupId === null ? "selected" : ""}`} onClick={() => setSelectedGroupId(null)}>
          <span className="group-name">All Forwards</span>
          <span className="group-count">{forwards.length}</span>
        </div>
        <GroupTreeView
          nodes={tree}
          selectedId={selectedGroupId}
          onSelect={setSelectedGroupId}
          onRename={(node) => setRenamingGroup(node)}
          onDelete={(node) => setDeletingGroup(node)}
          onCreateChild={(parentId) => setRenamingGroup({ parentId })}
          onDropItems={moveForwards}
        />
      </div>

      <div className="list-pane">
        <div className="list-pane-header">
          <h3>Port Forwards</h3>
          <MoveToGroupSelect
            groups={portForwardGroups}
            count={selection.selected.size}
            onMove={(groupId) => moveForwards([...selection.selected], groupId)}
          />
          <span style={{ flex: 1 }} />
          <button
            disabled={startable.length === 0}
            title={
              `Start every stopped forward in ${scopeLabel}` +
              (needsProfile > 0 ? ` (${needsProfile} skipped: no SSH profile)` : "")
            }
            onClick={() => startable.forEach((f) => forwardsStart(f.id))}
          >
            Start All ({startable.length})
          </button>
          <button
            disabled={running.length === 0}
            title={`Stop every running forward in ${scopeLabel}`}
            onClick={() => running.forEach((f) => forwardsStop(f.id))}
          >
            Stop All ({running.length})
          </button>
          <button
            className="primary"
            disabled={profiles.length === 0}
            title={profiles.length === 0 ? "Create an SSH profile first" : undefined}
            onClick={() => setEditingForward("new")}
          >
            New Forward
          </button>
        </div>

        {moveError && <div className="field-error list-error">{moveError}</div>}

        {profiles.length === 0 ? (
          <div className="empty-state">
            <h3>Create an SSH profile first</h3>
            <p>Port forwards run through an SSH profile - add one under Terminals before creating a forward.</p>
          </div>
        ) : forwards.length === 0 ? (
          <div className="empty-state">
            <h3>No port forwards yet</h3>
            <button className="primary" onClick={() => setEditingForward("new")}>
              New Forward
            </button>
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th className="select-col">
                  <input
                    type="checkbox"
                    aria-label="Select all forwards"
                    checked={selection.allSelected}
                    onChange={selection.toggleAll}
                  />
                </th>
                <th>Name</th>
                <th>Type</th>
                <th>Source</th>
                <th>Destination</th>
                <th>Profile</th>
                <th>Status</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {visibleForwards.map((f) => (
                <ForwardRow
                  key={f.id}
                  f={f}
                  profileName={profileName(f.ssh_profile_id)}
                  selected={selection.selected.has(f.id)}
                  onToggleSelect={() => selection.toggle(f.id)}
                  onDragStart={(e) => startItemDrag(e, dragPayload(f.id, selection.selected))}
                  onEdit={() => setEditingForward(f)}
                  onDuplicate={async () => {
                    await forwardsDuplicate(f.id, `${f.name} copy`);
                    await refreshForwards();
                  }}
                  onDelete={() => setDeletingForward(f)}
                />
              ))}
            </tbody>
          </table>
        )}
      </div>

      {editingForward && (
        <ForwardEditorModal
          forward={editingForward === "new" ? null : editingForward}
          defaultGroupId={selectedGroupId}
          onClose={() => setEditingForward(null)}
          onSaved={() => setEditingForward(null)}
        />
      )}

      {renamingGroup && (
        <PromptDialog
          title={
            renamingGroup === "new-root" || (typeof renamingGroup === "object" && "parentId" in renamingGroup)
              ? "New Group"
              : "Rename Group"
          }
          label="Group name"
          initialValue={renamingGroup !== "new-root" && !("parentId" in renamingGroup) ? renamingGroup.name : ""}
          onCancel={() => setRenamingGroup(null)}
          onConfirm={async (value) => {
            if (renamingGroup === "new-root") {
              await portForwardGroupsCreate({ parent_id: null, name: value });
            } else if ("parentId" in renamingGroup) {
              await portForwardGroupsCreate({ parent_id: renamingGroup.parentId, name: value });
            } else {
              await portForwardGroupsUpdate({
                id: renamingGroup.id,
                parent_id: renamingGroup.parent_id,
                name: value,
                sort_order: null,
              });
            }
            await refreshPortForwardGroups();
            setRenamingGroup(null);
          }}
        />
      )}

      {deletingGroup && (
        <GroupDeleteDialog
          groupName={deletingGroup.name}
          itemNoun="forwards"
          onCancel={() => setDeletingGroup(null)}
          onConfirm={async (strategy) => {
            await portForwardGroupsDelete(deletingGroup.id, strategy);
            await Promise.all([refreshPortForwardGroups(), refreshForwards()]);
            setDeletingGroup(null);
            if (selectedGroupId === deletingGroup.id) setSelectedGroupId(null);
          }}
        />
      )}

      {deletingForward && (
        <ConfirmDialog
          title={`Delete "${deletingForward.name}"?`}
          message="This stops and permanently deletes the forward rule."
          confirmLabel="Delete"
          danger
          onCancel={() => setDeletingForward(null)}
          onConfirm={async () => {
            await forwardsDelete(deletingForward.id);
            await refreshForwards();
            setDeletingForward(null);
          }}
        />
      )}
    </div>
  );
}
