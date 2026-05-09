import { useEffect, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  buddyStateAtom,
  buddyTickAtom,
  buddySpeechAtom,
  loadBuddyAtom,
  renderSprite,
} from "@/stores/buddy";
import { activeModeAtom } from "@/stores/workspace";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
} from "@/components/ui/tooltip";

const TICK_MS = 500;

// Speech reactions to mode transitions
const MODE_REACTIONS: Record<string, string[]> = {
  idle: [
    "zzz...",
    "*yawns*",
    "...",
    "*floats quietly*",
  ],
  active: [
    "*perks up*",
    "here we go",
    "watching...",
  ],
  tool: [
    "*observes intently*",
    "interesting...",
    "hmm",
  ],
};

export function BuddyWidget() {
  const state = useAtomValue(buddyStateAtom);
  const tick = useSetAtom(buddyTickAtom);
  const speak = useSetAtom(buddySpeechAtom);
  const loadBuddy = useSetAtom(loadBuddyAtom);
  const mode = useAtomValue(activeModeAtom);
  const prevModeRef = useRef(mode);

  useEffect(() => {
    loadBuddy();
  }, [loadBuddy]);

  useEffect(() => {
    const interval = setInterval(() => tick(), TICK_MS);
    return () => clearInterval(interval);
  }, [tick]);

  // React to mode changes with occasional speech
  useEffect(() => {
    const prev = prevModeRef.current;
    prevModeRef.current = mode;
    if (prev === mode) return;

    if (Math.random() > 0.3) return;

    const reactions = MODE_REACTIONS[mode] ?? MODE_REACTIONS.idle!;
    const msg = reactions[Math.floor(Math.random() * reactions.length)]!;
    speak(msg);
  }, [mode, speak]);

  if (!state.soul) return null;

  // Default to ghost with degree eyes and no hat if bones not yet loaded
  const species = state.bones?.species ?? "ghost";
  const eye = state.bones?.eye ?? "degree";
  const hat = state.bones?.hat ?? "none";
  const lines = renderSprite(species, eye, hat, state.tick);
  const showBubble = state.speechBubble && Date.now() < state.speechExpiresAt;
  const bubbleFading = showBubble && (state.speechExpiresAt - Date.now()) < 3000;

  return (
    <div className="flex flex-col items-center gap-1 px-2 py-1.5">
      {/* Speech bubble */}
      {showBubble && (
        <div
          className={`max-w-full rounded-md bg-secondary px-2 py-0.5 text-center text-[10px] text-muted-foreground transition-opacity duration-1000 ${
            bubbleFading ? "opacity-40" : "opacity-100"
          }`}
        >
          {state.speechBubble}
        </div>
      )}

      {/* Sprite */}
      <Tooltip>
        <TooltipTrigger
          className={`cursor-default select-none text-center font-mono text-[9px] leading-[10px] text-foreground/80 whitespace-pre ${
            state.bones?.shiny ? "text-amber-400" : ""
          }`}
        >
          {lines.join("\n")}
        </TooltipTrigger>
        <TooltipContent side="right" className="max-w-56">
          <div className="flex flex-col gap-1">
            <span className="font-semibold">{state.soul.name}</span>
            <span className="text-xs capitalize text-muted-foreground">
              {state.bones?.shiny && "✨ "}
              {state.bones?.rarity ?? "?"} {species}
            </span>
            <p className="text-xs text-muted-foreground">
              {state.soul.personality}
            </p>
            {state.bones?.stats && (
              <div className="mt-1 grid grid-cols-2 gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
                <span>DEBUG {state.bones.stats.debugging}</span>
                <span>PATIENCE {state.bones.stats.patience}</span>
                <span>CHAOS {state.bones.stats.chaos}</span>
                <span>WISDOM {state.bones.stats.wisdom}</span>
                <span>SNARK {state.bones.stats.snark}</span>
              </div>
            )}
          </div>
        </TooltipContent>
      </Tooltip>

      {/* Name label */}
      <span className="text-[10px] font-medium text-muted-foreground">
        {state.soul.name}
      </span>
    </div>
  );
}
