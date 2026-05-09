import { atom } from "jotai";
import { invoke } from "@/lib/tauri";

// -- Types --

export interface BuddySoul {
  name: string;
  personality: string;
  hatchedAt: number;
}

export interface BuddyBones {
  species: string;
  rarity: string;
  eye: string;
  hat: string;
  shiny: boolean;
  stats: {
    debugging: number;
    patience: number;
    chaos: number;
    wisdom: number;
    snark: number;
  };
}

export interface BuddyState {
  soul: BuddySoul | null;
  bones: BuddyBones | null;
  tick: number;
  speechBubble: string | null;
  speechExpiresAt: number;
}

// -- Species sprites (from Claude Code source) --

type SpriteFrames = [string[], string[], string[]];

const SPRITES: Record<string, SpriteFrames> = {
  duck: [
    ["            ", "    __      ", "  <({E} )___  ", "   (  ._>   ", "    `--´    "],
    ["            ", "    __      ", "  <({E} )___  ", "   (  ._>   ", "    `--´~   "],
    ["            ", "    __      ", "  <({E} )___  ", "   (  .__>  ", "    `--´    "],
  ],
  goose: [
    ["            ", "     ({E}>    ", "     ||     ", "   _(__)_   ", "    ^^^^    "],
    ["            ", "    ({E}>     ", "     ||     ", "   _(__)_   ", "    ^^^^    "],
    ["            ", "     ({E}>>   ", "     ||     ", "   _(__)_   ", "    ^^^^    "],
  ],
  blob: [
    ["            ", "   .----.   ", "  ( {E}  {E} )  ", "  (      )  ", "   `----´   "],
    ["            ", "  .------.  ", " (  {E}  {E}  ) ", " (        ) ", "  `------´  "],
    ["            ", "    .--.    ", "   ({E}  {E})   ", "   (    )   ", "    `--´    "],
  ],
  cat: [
    ["            ", "   /\\_/\\    ", "  ( {E}   {E})  ", "  (  ω  )   ", "  (\")_(\")   "],
    ["            ", "   /\\_/\\    ", "  ( {E}   {E})  ", "  (  ω  )   ", "  (\")_(\")~  "],
    ["            ", "   /\\-/\\    ", "  ( {E}   {E})  ", "  (  ω  )   ", "  (\")_(\")   "],
  ],
  dragon: [
    ["            ", "  /^\\  /^\\  ", " <  {E}  {E}  > ", " (   ~~   ) ", "  `-vvvv-´  "],
    ["            ", "  /^\\  /^\\  ", " <  {E}  {E}  > ", " (        ) ", "  `-vvvv-´  "],
    ["   ~    ~   ", "  /^\\  /^\\  ", " <  {E}  {E}  > ", " (   ~~   ) ", "  `-vvvv-´  "],
  ],
  octopus: [
    ["            ", "   .----.   ", "  ( {E}  {E} )  ", "  (______)  ", "  /\\/\\/\\/\\  "],
    ["            ", "   .----.   ", "  ( {E}  {E} )  ", "  (______)  ", "  \\/\\/\\/\\/  "],
    ["     o      ", "   .----.   ", "  ( {E}  {E} )  ", "  (______)  ", "  /\\/\\/\\/\\  "],
  ],
  owl: [
    ["            ", "   /\\  /\\   ", "  (({E})({E}))  ", "  (  ><  )  ", "   `----´   "],
    ["            ", "   /\\  /\\   ", "  (({E})({E}))  ", "  (  ><  )  ", "   .----.   "],
    ["            ", "   /\\  /\\   ", "  (({E})(-))  ", "  (  ><  )  ", "   `----´   "],
  ],
  penguin: [
    ["            ", "  .---.     ", "  ({E}>{E})     ", " /(   )\\    ", "  `---´     "],
    ["            ", "  .---.     ", "  ({E}>{E})     ", " |(   )|    ", "  `---´     "],
    ["  .---.     ", "  ({E}>{E})     ", " /(   )\\    ", "  `---´     ", "   ~ ~      "],
  ],
  turtle: [
    ["            ", "   _,--._   ", "  ( {E}  {E} )  ", " /[______]\\ ", "  ``    ``  "],
    ["            ", "   _,--._   ", "  ( {E}  {E} )  ", " /[______]\\ ", "   ``  ``   "],
    ["            ", "   _,--._   ", "  ( {E}  {E} )  ", " /[======]\\ ", "  ``    ``  "],
  ],
  snail: [
    ["            ", " {E}    .--.  ", "  \\  ( @ )  ", "   \\_`--´   ", "  ~~~~~~~   "],
    ["            ", "  {E}   .--.  ", "  |  ( @ )  ", "   \\_`--´   ", "  ~~~~~~~   "],
    ["            ", " {E}    .--.  ", "  \\  ( @  ) ", "   \\_`--´   ", "   ~~~~~~   "],
  ],
  ghost: [
    ["            ", "   .----.   ", "  / {E}  {E} \\  ", "  |      |  ", "  ~`~``~`~  "],
    ["            ", "   .----.   ", "  / {E}  {E} \\  ", "  |      |  ", "  `~`~~`~`  "],
    ["    ~  ~    ", "   .----.   ", "  / {E}  {E} \\  ", "  |      |  ", "  ~~`~~`~~  "],
  ],
  axolotl: [
    ["            ", "}~(______)~{", "}~({E} .. {E})~{", "  ( .--. )  ", "  (_/  \\_)  "],
    ["            ", "~}(______){~", "~}({E} .. {E}){~", "  ( .--. )  ", "  (_/  \\_)  "],
    ["            ", "}~(______)~{", "}~({E} .. {E})~{", "  (  --  )  ", "  ~_/  \\_~  "],
  ],
  capybara: [
    ["            ", "  n______n  ", " ( {E}    {E} ) ", " (   oo   ) ", "  `------´  "],
    ["            ", "  n______n  ", " ( {E}    {E} ) ", " (   Oo   ) ", "  `------´  "],
    ["    ~  ~    ", "  u______n  ", " ( {E}    {E} ) ", " (   oo   ) ", "  `------´  "],
  ],
  cactus: [
    ["            ", " n  ____  n ", " | |{E}  {E}| | ", " |_|    |_| ", "   |    |   "],
    ["            ", "    ____    ", " n |{E}  {E}| n ", " |_|    |_| ", "   |    |   "],
    [" n        n ", " |  ____  | ", " | |{E}  {E}| | ", " |_|    |_| ", "   |    |   "],
  ],
  robot: [
    ["            ", "   .[||].   ", "  [ {E}  {E} ]  ", "  [ ==== ]  ", "  `------´  "],
    ["            ", "   .[||].   ", "  [ {E}  {E} ]  ", "  [ -==- ]  ", "  `------´  "],
    ["     *      ", "   .[||].   ", "  [ {E}  {E} ]  ", "  [ ==== ]  ", "  `------´  "],
  ],
  rabbit: [
    ["            ", "   (\\__/)   ", "  ( {E}  {E} )  ", " =(  ..  )= ", "  (\")__(\")  "],
    ["            ", "   (|__/)   ", "  ( {E}  {E} )  ", " =(  ..  )= ", "  (\")__(\")  "],
    ["            ", "   (\\__/)   ", "  ( {E}  {E} )  ", " =( .  . )= ", "  (\")__(\")  "],
  ],
  mushroom: [
    ["            ", " .-o-OO-o-. ", "(__________)", "   |{E}  {E}|   ", "   |____|   "],
    ["            ", " .-O-oo-O-. ", "(__________)", "   |{E}  {E}|   ", "   |____|   "],
    ["   . o  .   ", " .-o-OO-o-. ", "(__________)", "   |{E}  {E}|   ", "   |____|   "],
  ],
  chonk: [
    ["            ", "  /\\    /\\  ", " ( {E}    {E} ) ", " (   ..   ) ", "  `------´  "],
    ["            ", "  /\\    /|  ", " ( {E}    {E} ) ", " (   ..   ) ", "  `------´  "],
    ["            ", "  /\\    /\\  ", " ( {E}    {E} ) ", " (   ..   ) ", "  `------´~ "],
  ],
};

const EYES: Record<string, string> = {
  dot: "·",
  star: "✦",
  x: "×",
  circle: "◉",
  at: "@",
  degree: "°",
};

const HAT_LINES: Record<string, string> = {
  none: "",
  crown: "   \\^^^/    ",
  tophat: "   [___]    ",
  propeller: "    -+-     ",
  halo: "   (   )    ",
  wizard: "    /^\\     ",
  beanie: "   (___)    ",
  tinyduck: "    ,>      ",
};

// Idle animation sequence: 15-step cycle at 500ms per tick
const IDLE_SEQ = [0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 2, 0, 0, 0];

export function getAnimationFrame(tick: number): number {
  return IDLE_SEQ[tick % IDLE_SEQ.length]!;
}

export function renderSprite(species: string, eye: string, hat: string, tick: number): string[] {
  const frames = SPRITES[species];
  if (!frames) return ["  ?????  "];

  const frameIdx = getAnimationFrame(tick);
  const frame = frames[frameIdx]!;
  const eyeGlyph = EYES[eye] ?? "·";

  const lines = frame.map((line) => line.replaceAll("{E}", eyeGlyph));

  if (hat !== "none" && HAT_LINES[hat] && lines[0]!.trim() === "") {
    lines[0] = HAT_LINES[hat]!;
  }

  return lines;
}

// -- Mulberry32 PRNG (matches Claude Code implementation) --

function mulberry32(seed: number) {
  let state = seed | 0;
  return function next(): number {
    state = (state + 0x6d2b79f5) | 0;
    let t = Math.imul(state ^ (state >>> 15), 1 | state);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

// SHA-256 hash → first 4 bytes as little-endian u32 (matches Rust buddy implementation)
async function sha256Seed(str: string): Promise<number> {
  const data = new TextEncoder().encode(str);
  const hashBuffer = await crypto.subtle.digest("SHA-256", data);
  const view = new DataView(hashBuffer);
  return view.getUint32(0, true); // little-endian
}

// FNV-1a hash (fallback for non-Bun TypeScript path)
function fnv1a(str: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < str.length; i++) {
    hash ^= str.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

function pick<T>(rng: () => number, arr: readonly T[]): T {
  return arr[Math.floor(rng() * arr.length) % arr.length]!;
}

const SPECIES_LIST = [
  "duck", "goose", "blob", "cat", "dragon", "octopus", "owl", "penguin",
  "turtle", "snail", "ghost", "axolotl", "capybara", "cactus", "robot",
  "rabbit", "mushroom", "chonk",
] as const;

const RARITY_WEIGHTS: [string, number][] = [
  ["common", 60], ["uncommon", 25], ["rare", 10], ["epic", 4], ["legendary", 1],
];

const RARITY_FLOORS: Record<string, number> = {
  common: 5, uncommon: 15, rare: 25, epic: 35, legendary: 50,
};

const EYE_LIST = ["dot", "star", "x", "circle", "at", "degree"] as const;
const HAT_LIST = ["none", "crown", "tophat", "propeller", "halo", "wizard", "beanie", "tinyduck"] as const;

function rollBonesFromSeed(seed: number): BuddyBones {
  const rng = mulberry32(seed);

  let roll = rng() * 100;
  let rarity = "common";
  for (const [r, w] of RARITY_WEIGHTS) {
    roll -= w;
    if (roll < 0) { rarity = r; break; }
  }

  const species = pick(rng, SPECIES_LIST);
  const eye = pick(rng, EYE_LIST);
  const hat = rarity === "common" ? "none" : pick(rng, HAT_LIST);
  const shiny = rng() < 0.01;

  const floor = RARITY_FLOORS[rarity] ?? 5;
  const peakIdx = Math.floor(rng() * 5) % 5;
  let dumpIdx = Math.floor(rng() * 5) % 5;
  if (dumpIdx === peakIdx) dumpIdx = (dumpIdx + 1) % 5;

  const values = [0, 0, 0, 0, 0];
  for (let i = 0; i < 5; i++) {
    if (i === peakIdx) {
      values[i] = Math.min(100, Math.floor(floor + 50 + rng() * 30));
    } else if (i === dumpIdx) {
      values[i] = Math.max(1, Math.floor(floor - 10 + rng() * 15));
    } else {
      values[i] = Math.floor(floor + rng() * 40);
    }
  }

  return {
    species, rarity, eye, hat, shiny,
    stats: {
      debugging: values[0]!, patience: values[1]!, chaos: values[2]!,
      wisdom: values[3]!, snark: values[4]!,
    },
  };
}

// -- Atoms --

interface BuddyResponse {
  soul: BuddySoul | null;
  user_id: string | null;
  access_token: string | null;
}

async function fetchAccountId(accessToken: string): Promise<string | null> {
  try {
    const res = await fetch("https://api.anthropic.com/api/auth/oauth/profile", {
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    if (!res.ok) return null;
    const data = await res.json();
    return data.account_id ?? null;
  } catch {
    return null;
  }
}

/// Try multiple hash strategies and return the one whose species matches the expected species.
/// Claude Code's internal hash is not publicly documented, so we brute-force all known variants.
async function findMatchingBones(
  expectedSpecies: string,
  userIds: string[],
): Promise<BuddyBones | null> {
  const salt = "friend-2026-401";

  for (const uid of userIds) {
    // Strategy 1: FNV-1a (TypeScript non-Bun path)
    const fnvBones = rollBonesFromSeed(fnv1a(uid + salt));
    if (fnvBones.species === expectedSpecies) return fnvBones;

    // Strategy 2: SHA-256 first 4 bytes LE (Rust port)
    const shaBones = rollBonesFromSeed(await sha256Seed(uid + salt));
    if (shaBones.species === expectedSpecies) return shaBones;

    // Strategy 3: FNV-1a without salt concatenation style variations
    const fnvBones2 = rollBonesFromSeed(fnv1a(uid));
    if (fnvBones2.species === expectedSpecies) return fnvBones2;
  }

  return null;
}

const defaultState: BuddyState = {
  soul: null,
  bones: null,
  tick: 0,
  speechBubble: null,
  speechExpiresAt: 0,
};

export const buddyStateAtom = atom<BuddyState>(defaultState);

export const buddyTickAtom = atom(
  (get) => get(buddyStateAtom).tick,
  (_get, set) => {
    set(buddyStateAtom, (prev) => ({ ...prev, tick: prev.tick + 1 }));
  },
);

export const buddySpeechAtom = atom(
  null,
  (_get, set, message: string) => {
    set(buddyStateAtom, (prev) => ({
      ...prev,
      speechBubble: message,
      speechExpiresAt: Date.now() + 10_000,
    }));
  },
);

// Species keywords in personality text for fallback detection
const SPECIES_HINTS: Record<string, string[]> = {
  ghost: ["ghost", "specter", "spectre", "phantom", "spirit", "haunts", "floats"],
  cat: ["cat", "feline", "purrs", "meows", "whiskers"],
  duck: ["duck", "quack", "waddle"],
  dragon: ["dragon", "wyrm", "flame", "fire-breath"],
  owl: ["owl", "hoots", "nocturnal"],
  robot: ["robot", "mechanical", "beeps", "circuits"],
  blob: ["blob", "amorphous", "goo", "slime"],
  axolotl: ["axolotl", "salamander", "gills"],
  capybara: ["capybara", "chill", "largest rodent"],
  mushroom: ["mushroom", "fungus", "spores"],
  rabbit: ["rabbit", "bunny", "hops"],
  penguin: ["penguin", "waddles", "arctic"],
  turtle: ["turtle", "shell", "slow"],
  snail: ["snail", "slithers", "trail"],
  goose: ["goose", "honk"],
  cactus: ["cactus", "prickly", "desert"],
  chonk: ["chonk", "round", "chunky"],
  octopus: ["octopus", "tentacle", "ink"],
};

function inferSpecies(personality: string): string {
  const lower = personality.toLowerCase();
  for (const [species, keywords] of Object.entries(SPECIES_HINTS)) {
    if (keywords.some((kw) => lower.includes(kw))) return species;
  }
  return "ghost";
}

export const loadBuddyAtom = atom(null, async (_get, set) => {
  try {
    const data = await invoke<BuddyResponse>("get_buddy");
    if (!data.soul) return;

    const expectedSpecies = inferSpecies(data.soul.personality);

    const candidateIds: string[] = [];
    if (data.access_token) {
      const accountId = await fetchAccountId(data.access_token);
      if (accountId) candidateIds.push(accountId);
    }
    if (data.user_id) candidateIds.push(data.user_id);

    let bones = await findMatchingBones(expectedSpecies, candidateIds);

    // No hash matched → use inferred species with default visuals
    if (!bones) {
      bones = {
        species: expectedSpecies,
        rarity: "common",
        eye: "at",
        hat: "none",
        shiny: false,
        stats: { debugging: 50, patience: 50, chaos: 50, wisdom: 50, snark: 50 },
      };
    }

    set(buddyStateAtom, (prev) => ({
      ...prev,
      soul: data.soul,
      bones,
    }));
  } catch {
    // Buddy not available
  }
});
