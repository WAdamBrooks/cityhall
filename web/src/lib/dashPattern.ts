/// A small seeded generator, so a pattern is random-looking but the same every
/// time it is drawn for the same seed (mulberry32).
function seeded(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const PAIRS = 4;

/// An irregular `stroke-dasharray` for an agent's border: a few dash and gap
/// lengths, in pixels, that repeat around the box. Each agent gets its own
/// pattern, and the same agent always gets the same one, so its messages and its
/// entry in the room list visibly match. The dashes are long and the gaps short
/// so a thin border stays calm rather than busy.
export function dashPattern(seed: number): string {
  const next = seeded(seed * 2654435761 + 1);
  const lengths: number[] = [];
  for (let i = 0; i < PAIRS; i++) {
    lengths.push(Math.round(5 + next() * 9)); // dash: 5 to 14
    lengths.push(Math.round(2 + next() * 3)); // gap: 2 to 5
  }
  return lengths.join(" ");
}
