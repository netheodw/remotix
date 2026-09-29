import { useEffect, useRef, useState } from "react";
import { profilesTestConnection } from "../api/commands";
import { parseHostKeyError, errorMessage } from "../api/errors";
import type { HostKeyDecision } from "../components/HostKeyDialog";
import { HostKeyDialog } from "../components/HostKeyDialog";
import { Modal } from "../components/Modal";
import type { SshProfile, TestConnectionStage } from "../api/types";
import { useData } from "../state/DataContext";

const STEPS: { key: TestConnectionStage["stage"] | "done"; label: string }[] = [
  { key: "connecting", label: "Connecting" },
  { key: "verifying_host", label: "Host-key verification" },
  { key: "authenticating", label: "Authenticating" },
  { key: "done", label: "Success" },
];

export function TestConnectionModal({ profile, onClose }: { profile: SshProfile; onClose: () => void }) {
  const { refreshProfiles } = useData();
  const [stage, setStage] = useState<TestConnectionStage["stage"] | "done" | "failed">("connecting");
  const [reachedIndex, setReachedIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [hostKeyDecision, setHostKeyDecision] = useState<HostKeyDecision | null>(null);
  const running = useRef(false);

  const run = () => {
    if (running.current) return;
    running.current = true;
    setStage("connecting");
    setReachedIndex(0);
    setError(null);
    profilesTestConnection(profile.id, (s) => {
      setStage(s.stage);
      setReachedIndex(STEPS.findIndex((x) => x.key === s.stage));
    })
      .then(() => {
        setStage("done");
        setReachedIndex(STEPS.length - 1);
        refreshProfiles();
      })
      .catch((e) => {
        const decision = parseHostKeyError(e);
        if (decision) {
          setHostKeyDecision(decision);
        } else {
          setStage("failed");
          setError(errorMessage(e));
          refreshProfiles();
        }
      })
      .finally(() => {
        running.current = false;
      });
  };

  useEffect(() => {
    run();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (hostKeyDecision) {
    return (
      <HostKeyDialog
        decision={hostKeyDecision}
        onCancel={() => {
          setHostKeyDecision(null);
          setStage("failed");
          setError("Host key rejected.");
        }}
        onResolved={() => {
          setHostKeyDecision(null);
          run();
        }}
      />
    );
  }

  return (
    <Modal title={`Test Connection - ${profile.name}`} onClose={onClose} width={440}>
      <ul className="timeline">
        {STEPS.map((s, stepIndex) => {
          const isFailed = stage === "failed";
          const state = isFailed
            ? stepIndex < reachedIndex
              ? "done"
              : stepIndex === reachedIndex
                ? "failed"
                : "pending"
            : stepIndex < reachedIndex
              ? "done"
              : stepIndex === reachedIndex
                ? "current"
                : "pending";
          return (
            <li key={s.key} className={`timeline-step ${state}`}>
              {s.label}
            </li>
          );
        })}
      </ul>
      {stage === "failed" && <div className="field-error">{error}</div>}
      {stage === "done" && <div className="field-success">Connection succeeded.</div>}
      <div className="modal-actions">
        {stage === "failed" && <button onClick={run}>Retry</button>}
        <button className="primary" onClick={onClose}>
          Close
        </button>
      </div>
    </Modal>
  );
}
