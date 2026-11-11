export interface AgentFormInput {
  name: string;
  description: string;
  tools: string;
}

/// Tool names typed as a comma or line separated list.
export function parseTools(text: string): string[] {
  const seen = new Set<string>();
  for (const part of text.split(/[,\n]/)) {
    const tool = part.trim();
    if (tool) seen.add(tool);
  }
  return [...seen];
}

/// The reason a form cannot be submitted yet, or null. Mirrors the server's
/// admission rule (a specialty needs a description and a tool) so the person
/// sees the problem before the round trip. The name is what people @mention,
/// so it cannot contain spaces.
export function agentFormProblem(input: AgentFormInput): string | null {
  const name = input.name.trim();
  if (!name) return "Give the agent a name.";
  if (/\s/.test(name)) return "A name cannot contain spaces, because people @mention it.";
  if (!input.description.trim()) return "Describe the agent's specialty.";
  if (parseTools(input.tools).length === 0) return "List at least one tool.";
  return null;
}

/// A mentionable name taken from a session title: words joined with dashes.
export function suggestAgentName(title: string): string {
  const words = title.match(/[A-Za-z0-9]+/g);
  return words && words.length ? words.join("-") : "Agent";
}
