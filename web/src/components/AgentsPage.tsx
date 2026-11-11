import { useCallback, useEffect, useState, type FormEvent } from "react";
import { api, ApiError, type TownhallAgent, type TownhallSession } from "../lib/api";
import { agentFormProblem, parseTools, suggestAgentName } from "../lib/agentForm";
import { PageBody, PageHeader } from "./AppShell";
import { RowMenu } from "./RowMenu";
import {
  Button,
  Checkbox,
  ErrorText,
  Field,
  Input,
  Modal,
  Select,
  Tag,
  Textarea,
  tableCardClass,
  tableHeadClass,
  tdClass,
  thClass,
  trClass,
} from "./ui";

function errorText(e: unknown, fallback: string): string {
  return e instanceof ApiError ? e.message : fallback;
}

/// The agents you have registered, and a way to register another. An agent is one
/// of your workspace's sessions given a name and a specialty so that meetings can
/// bring it in.
export function AgentsPage() {
  const [agents, setAgents] = useState<TownhallAgent[] | null>(null);
  const [sessions, setSessions] = useState<TownhallSession[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<TownhallAgent | null>(null);
  const [editing, setEditing] = useState<TownhallAgent | null>(null);

  const load = useCallback(async () => {
    try {
      const [mine, open] = await Promise.all([api.listMyTownhallAgents(), api.listMyTownhallSessions()]);
      setAgents(mine);
      setSessions(open);
      setError(null);
    } catch (e) {
      setError(errorText(e, "could not load your agents"));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const titleOf = (id: string) => sessions?.find((s) => s.id === id)?.title ?? id;
  const taken = new Set((agents ?? []).map((a) => a.session_id));
  const free = (sessions ?? []).filter((s) => !taken.has(s.id));

  return (
    <>
      <PageHeader
        title="Agents"
        meta={agents ? `${agents.length} agent${agents.length === 1 ? "" : "s"}` : undefined}
        actions={
          <Button variant="primary" onClick={() => setCreating(true)}>
            + New agent
          </Button>
        }
      />
      <PageBody className="space-y-3.5">
        <p className="max-w-[600px] text-[13px] text-text-dim">
          An agent is one of your workspace sessions with a name and a specialty, so a meeting can bring it in. You can
          bring your agents into a meeting from that meeting&apos;s page.
        </p>
        {error && <ErrorText>{error}</ErrorText>}
        {agents && agents.length === 0 && !error && (
          <p className="text-sm text-text-dim">You have not created an agent yet.</p>
        )}
        {agents && agents.length > 0 && (
          <div className={tableCardClass}>
            <table className="w-full text-sm">
              <thead>
                <tr className={tableHeadClass}>
                  <th className={thClass}>Name</th>
                  <th className={thClass}>Specialty</th>
                  <th className={thClass}>Tools</th>
                  <th className={thClass}>Session</th>
                  <th className={thClass}></th>
                </tr>
              </thead>
              <tbody>
                {agents.map((a) => (
                  <tr key={a.id} className={trClass}>
                    <td className={`${tdClass} text-text-bright`}>{a.name}</td>
                    <td className={`${tdClass} text-text-dim`}>{a.description || "none"}</td>
                    <td className={tdClass}>
                      <div className="flex flex-wrap gap-1.5">
                        {a.tools.length ? (
                          a.tools.map((t) => <Tag key={t}>{t}</Tag>)
                        ) : (
                          <span className="text-text-dim">none</span>
                        )}
                      </div>
                    </td>
                    <td className={`${tdClass} text-text-dim`}>{titleOf(a.session_id)}</td>
                    <td className={`${tdClass} text-right`}>
                      <RowMenu
                        label={a.name}
                        items={[
                          { label: "Edit", onSelect: () => setEditing(a) },
                          { label: "Delete", danger: true, onSelect: () => setDeleting(a) },
                        ]}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </PageBody>
      {deleting && (
        <DeleteAgentDialog
          agent={deleting}
          sessionTitle={titleOf(deleting.session_id)}
          onClose={() => setDeleting(null)}
          onDeleted={() => {
            setDeleting(null);
            void load();
          }}
        />
      )}
      {(creating || editing) && (
        <AgentDialog
          agent={editing ?? undefined}
          free={free}
          onClose={() => {
            setCreating(false);
            setEditing(null);
          }}
          onSaved={() => {
            setCreating(false);
            setEditing(null);
            void load();
          }}
        />
      )}
    </>
  );
}

function DeleteAgentDialog({
  agent,
  sessionTitle,
  onClose,
  onDeleted,
}: {
  agent: TownhallAgent;
  sessionTitle: string;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const [deleteSession, setDeleteSession] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function confirm() {
    setBusy(true);
    setError(null);
    try {
      await api.deleteTownhallAgent(agent.id, deleteSession);
      onDeleted();
    } catch (err) {
      setError(errorText(err, "could not delete the agent"));
      setBusy(false);
    }
  }

  return (
    <Modal
      title={`Delete ${agent.name}?`}
      onClose={onClose}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button type="button" variant="danger" onClick={confirm} disabled={busy}>
            {busy ? "Deleting..." : "Delete agent"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3 text-[13px] text-text">
        <p>{agent.name} leaves every meeting it is in. What it already said stays in those meetings&apos; history.</p>
        <Checkbox
          checked={deleteSession}
          onChange={setDeleteSession}
          disabled={busy}
          label={`Also delete its session "${sessionTitle}" from my workspace`}
        />
        {error && <ErrorText>{error}</ErrorText>}
      </div>
    </Modal>
  );
}

/// The card for a new agent, or, given `agent`, for changing an existing one.
function AgentDialog({
  agent,
  free,
  onClose,
  onSaved,
}: {
  agent?: TownhallAgent;
  free: TownhallSession[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const editing = agent !== undefined;
  const [sessionId, setSessionId] = useState("");
  const [name, setName] = useState(agent?.name ?? "");
  const [description, setDescription] = useState(agent?.description ?? "");
  const [tools, setTools] = useState(agent?.tools.join(", ") ?? "");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  function pick(id: string) {
    setSessionId(id);
    const session = free.find((s) => s.id === id);
    if (session && !name.trim()) setName(suggestAgentName(session.title));
  }

  async function submit(e: FormEvent) {
    e.preventDefault();
    const problem = agentFormProblem({ name, description, tools });
    if (problem) {
      setError(problem);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const fields = { name: name.trim(), description: description.trim(), tools: parseTools(tools) };
      if (agent) {
        await api.updateTownhallAgent(agent.id, fields);
      } else {
        await api.registerTownhallAgent({ session_id: sessionId || undefined, ...fields });
      }
      onSaved();
    } catch (err) {
      setError(errorText(err, editing ? "could not save the agent" : "could not create the agent"));
      setBusy(false);
    }
  }

  return (
    <Modal
      title={editing ? `Edit ${agent.name}` : "New agent"}
      onClose={onClose}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" form="new-agent" variant="primary" disabled={busy}>
            {editing ? (busy ? "Saving..." : "Save") : busy ? "Creating..." : "Create agent"}
          </Button>
        </>
      }
    >
      <form id="new-agent" onSubmit={submit} className="flex flex-col gap-3">
        <Field label="Name" help="What people type after @. No spaces.">
          <Input value={name} onChange={(e) => setName(e.target.value)} aria-label="Agent name" autoFocus />
        </Field>
        <Field label="Specialty" help="What it is good for. It decides when it speaks up.">
          <Textarea
            rows={3}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            aria-label="Agent specialty"
          />
        </Field>
        <Field label="Tools" help="Comma separated, for example read, search.">
          <Input value={tools} onChange={(e) => setTools(e.target.value)} aria-label="Agent tools" />
        </Field>
        {!editing && free.length > 0 && (
          <Field
            label="Session"
            help="CityHall starts a new session in your workspace for the agent, unless you pick one."
          >
            <Select value={sessionId} onChange={(e) => pick(e.target.value)} aria-label="Session">
              <option value="">Start a new session</option>
              {free.map((s) => (
                <option key={s.id} value={s.id}>
                  Use {s.title} ({s.tool})
                </option>
              ))}
            </Select>
          </Field>
        )}
        {busy && !editing && !sessionId && (
          <p className="text-[12.5px] text-text-hint">Starting a session in your workspace...</p>
        )}
        {error && <ErrorText>{error}</ErrorText>}
      </form>
    </Modal>
  );
}
