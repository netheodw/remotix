import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  profilesDelete,
  profilesDuplicate,
  profilesMoveToGroup,
  terminalGroupsCreate,
  terminalGroupsDelete,
  terminalGroupsUpdate,
} from "../api/commands";
import { errorMessage } from "../api/errors";
import { GroupDeleteDialog } from "../components/GroupDeleteDialog";
import { descendantIds, GroupTreeView, startItemDrag, useGroupTree, type GroupTreeNode } from "../components/GroupTree";
import { dragPayload, moveItems, MoveToGroupSelect, useRowSelection } from "../components/MoveToGroup";
import { PromptDialog } from "../components/PromptDialog";
import { ConfirmDialog } from "../components/Modal";
import { authKindLabel, type SshProfile } from "../api/types";
import { useData } from "../state/DataContext";
import { useTerminalTabs } from "../state/TerminalTabsContext";
import { ProfileEditorModal } from "./ProfileEditorModal";
import { TermiusImportModal } from "./TermiusImportModal";
import { TestConnectionModal } from "./TestConnectionModal";
import type { Section } from "../App";

export function TerminalsScreen({
  onNavigate,
  createSignal,
}: {
  onNavigate: (s: Section) => void;
  createSignal: number;
}) {
  const { terminalGroups, profiles, refreshTerminalGroups, refreshProfiles } = useData();
  const { openTab } = useTerminalTabs();
  const [selectedGroupId, setSelectedGroupId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [editingProfile, setEditingProfile] = useState<SshProfile | "new" | null>(null);
  const lastCreateSignal = useRef(createSignal);
  useEffect(() => {
    if (createSignal !== lastCreateSignal.current) {
      lastCreateSignal.current = createSignal;
      setEditingProfile("new");
    }
  }, [createSignal]);
  const [testingProfile, setTestingProfile] = useState<SshProfile | null>(null);
  const [renamingGroup, setRenamingGroup] = useState<GroupTreeNode | "new-root" | { parentId: string } | null>(null);
  const [deletingGroup, setDeletingGroup] = useState<GroupTreeNode | null>(null);
  const [deletingProfile, setDeletingProfile] = useState<SshProfile | null>(null);
  const [termiusExportPath, setTermiusExportPath] = useState<string | null>(null);

  const pickTermiusExport = async () => {
    const path = await openFileDialog({
      multiple: false,
      title: "Select Termius export",
      filters: [{ name: "Termius export", extensions: ["json"] }],
    });
    if (typeof path === "string") setTermiusExportPath(path);
  };

  const itemCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    profiles.forEach((p) => {
      if (p.terminal_group_id) counts[p.terminal_group_id] = (counts[p.terminal_group_id] ?? 0) + 1;
    });
    return counts;
  }, [profiles]);
  const tree = useGroupTree(terminalGroups, itemCounts);

  const visibleProfiles = useMemo(() => {
    let list = profiles;
    if (selectedGroupId) {
      const ids = descendantIds(terminalGroups, selectedGroupId);
      list = list.filter((p) => p.terminal_group_id && ids.has(p.terminal_group_id));
    }
    if (search.trim()) {
      const q = search.trim().toLowerCase();
      list = list.filter(
        (p) =>
          p.name.toLowerCase().includes(q) ||
          p.hostname.toLowerCase().includes(q) ||
          p.username.toLowerCase().includes(q),
      );
    }
    return list;
  }, [profiles, selectedGroupId, search, terminalGroups]);

  const visibleIds = useMemo(() => visibleProfiles.map((p) => p.id), [visibleProfiles]);
  const selection = useRowSelection(visibleIds);
  const [moveError, setMoveError] = useState<string | null>(null);

  const moveProfiles = async (ids: string[], groupId: string | null) => {
    setMoveError(null);
    const groupOf = new Map(profiles.map((p) => [p.id, p.terminal_group_id]));
    try {
      await moveItems(ids, groupId, (id) => groupOf.get(id), profilesMoveToGroup);
      selection.clear();
    } catch (e) {
      setMoveError(errorMessage(e));
    } finally {
      await refreshProfiles();
    }
  };

  const connect = (p: SshProfile) => {
    openTab(p.id, p.name, `${p.username}@${p.hostname}`);
    onNavigate("active");
  };

  return (
    <div className="split-screen">
      <div className="tree-pane">
        <div className="tree-pane-header">
          <h3>Terminal Groups</h3>
          <button className="ghost" title="New root group" onClick={() => setRenamingGroup("new-root")}>
            +
          </button>
        </div>
        <div className={`group-row ${selectedGroupId === null ? "selected" : ""}`} onClick={() => setSelectedGroupId(null)}>
          <span className="group-name">All Profiles</span>
          <span className="group-count">{profiles.length}</span>
        </div>
        <GroupTreeView
          nodes={tree}
          selectedId={selectedGroupId}
          onSelect={setSelectedGroupId}
          onRename={(node) => setRenamingGroup(node)}
          onDelete={(node) => setDeletingGroup(node)}
          onCreateChild={(parentId) => setRenamingGroup({ parentId })}
          onDropItems={moveProfiles}
        />
      </div>

      <div className="list-pane">
        <div className="list-pane-header">
          <input
            className="search-input"
            placeholder="Search profiles..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <MoveToGroupSelect
            groups={terminalGroups}
            count={selection.selected.size}
            onMove={(groupId) => moveProfiles([...selection.selected], groupId)}
          />
          <button onClick={pickTermiusExport}>Import from Termius</button>
          <button className="primary" onClick={() => setEditingProfile("new")}>
            New SSH Profile
          </button>
        </div>

        {moveError && <div className="field-error list-error">{moveError}</div>}

        {profiles.length === 0 ? (
          <div className="empty-state">
            <h3>No SSH profiles yet</h3>
            <p>Create your first profile to connect to a server, or import your hosts from a Termius export.</p>
            <div className="dashboard-empty-actions">
              <button className="primary" onClick={() => setEditingProfile("new")}>
                Create SSH Profile
              </button>
              <button onClick={pickTermiusExport}>Import from Termius</button>
            </div>
          </div>
        ) : visibleProfiles.length === 0 ? (
          <div className="empty-state">
            <h3>No profiles match this filter</h3>
            <button onClick={() => setSearch("")}>Reset Filter</button>
          </div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th className="select-col">
                  <input
                    type="checkbox"
                    aria-label="Select all profiles"
                    checked={selection.allSelected}
                    onChange={selection.toggleAll}
                  />
                </th>
                <th>Name</th>
                <th>Host</th>
                <th>User</th>
                <th>Auth</th>
                <th>Last status</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {visibleProfiles.map((p) => (
                <tr
                  key={p.id}
                  draggable
                  className={selection.selected.has(p.id) ? "selected" : undefined}
                  onDragStart={(e) => startItemDrag(e, dragPayload(p.id, selection.selected))}
                >
                  <td className="select-col">
                    <input
                      type="checkbox"
                      aria-label={`Select ${p.name}`}
                      checked={selection.selected.has(p.id)}
                      onChange={() => selection.toggle(p.id)}
                    />
                  </td>
                  <td>{p.name}</td>
                  <td>
                    {p.hostname}:{p.port}
                  </td>
                  <td>{p.username}</td>
                  <td>{authKindLabel(p.authentication_kind)}</td>
                  <td>
                    {p.last_status ? (
                      <span className={`status-dot ${p.last_status === "success" || p.last_status === "connected" ? "ok" : "err"}`} />
                    ) : (
                      <span className="status-dot idle" />
                    )}
                    {p.last_status ?? "never connected"}
                  </td>
                  <td className="row-actions">
                    <button className="primary" onClick={() => connect(p)}>
                      Connect
                    </button>
                    <button onClick={() => setTestingProfile(p)}>Test</button>
                    <button onClick={() => setEditingProfile(p)}>Edit</button>
                    <button
                      onClick={async () => {
                        const dup = await profilesDuplicate(p.id, `${p.name} copy`);
                        await refreshProfiles();
                        setEditingProfile(dup);
                      }}
                    >
                      Duplicate
                    </button>
                    <button className="danger" onClick={() => setDeletingProfile(p)}>
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {editingProfile && (
        <ProfileEditorModal
          profile={editingProfile === "new" ? null : editingProfile}
          defaultGroupId={selectedGroupId}
          onClose={() => setEditingProfile(null)}
          onSaved={() => setEditingProfile(null)}
        />
      )}

      {testingProfile && <TestConnectionModal profile={testingProfile} onClose={() => setTestingProfile(null)} />}

      {termiusExportPath && (
        <TermiusImportModal path={termiusExportPath} onClose={() => setTermiusExportPath(null)} />
      )}

      {renamingGroup && (
        <PromptDialog
          title={
            renamingGroup === "new-root" || (typeof renamingGroup === "object" && "parentId" in renamingGroup)
              ? "New Group"
              : "Rename Group"
          }
          label="Group name"
          initialValue={
            renamingGroup !== "new-root" && !("parentId" in renamingGroup) ? renamingGroup.name : ""
          }
          onCancel={() => setRenamingGroup(null)}
          onConfirm={async (value) => {
            if (renamingGroup === "new-root") {
              await terminalGroupsCreate({ parent_id: null, name: value });
            } else if ("parentId" in renamingGroup) {
              await terminalGroupsCreate({ parent_id: renamingGroup.parentId, name: value });
            } else {
              await terminalGroupsUpdate({
                id: renamingGroup.id,
                parent_id: renamingGroup.parent_id,
                name: value,
                sort_order: null,
              });
            }
            await refreshTerminalGroups();
            setRenamingGroup(null);
          }}
        />
      )}

      {deletingGroup && (
        <GroupDeleteDialog
          groupName={deletingGroup.name}
          itemNoun="profiles"
          onCancel={() => setDeletingGroup(null)}
          onConfirm={async (strategy) => {
            await terminalGroupsDelete(deletingGroup.id, strategy);
            await Promise.all([refreshTerminalGroups(), refreshProfiles()]);
            setDeletingGroup(null);
            if (selectedGroupId === deletingGroup.id) setSelectedGroupId(null);
          }}
        />
      )}

      {deletingProfile && (
        <ConfirmDialog
          title={`Delete "${deletingProfile.name}"?`}
          message="This permanently deletes the profile and its stored credential. Port forwards that use it will be marked Needs Profile instead of being deleted."
          confirmLabel="Delete"
          danger
          onCancel={() => setDeletingProfile(null)}
          onConfirm={async () => {
            await profilesDelete(deletingProfile.id);
            await refreshProfiles();
            setDeletingProfile(null);
          }}
        />
      )}
    </div>
  );
}
