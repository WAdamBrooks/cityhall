import { useCallback, useEffect, useState, type FormEvent } from "react";
import { api, ApiError, type TownhallAgent, type TownhallSession } from "../lib/api";
import { agentFormProblem, parseTools, suggestAgentName } from "../lib/agentForm";
import { PageBody, PageHeader } from "./AppShell";
import {
  Button,
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
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </PageBody>
      {creating && (
        <NewAgentDialog
          free={free}
          onClose={() => setCreating(false)}
          onCreated={() => {
            setCreating(false);
            void load();
          }}
        />
      )}
    </>
  );
}

function NewAgentDialog({
  free,
  onClose,
  onCreated,
}: {
  free: TownhallSession[];
  onClose: () => void;
  onCreated: () => void;
}) {
  const [sessionId, setSessionId] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [tools, setTools] = useState("");
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
      await api.registerTownhallAgent({
        session_id: sessionId || undefined,
        name: name.trim(),
        description: description.trim(),
        tools: parseTools(tools),
      });
      onCreated();
    } catch (err) {
      setError(errorText(err, "could not create the agent"));
      setBusy(false);
    }
  }

  return (
    <Modal
      title="New agent"
      onClose={onClose}
      footer={
        <>
          <Button type="button" variant="ghost" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button type="submit" form="new-agent" variant="primary" disabled={busy}>
            {busy ? "Creating..." : "Create agent"}
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
        {free.length > 0 && (
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
        {busy && !sessionId && <p className="text-[12.5px] text-text-hint">Starting a session in your workspace...</p>}
        {error && <ErrorText>{error}</ErrorText>}
      </form>
    </Modal>
  );
}
