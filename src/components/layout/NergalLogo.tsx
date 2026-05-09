import { useCallback, useRef } from "react";
import growlUrl from "@/assets/nergal-growl.mp3";

const FAVICON_WEDGES = [
  "1,1 11,1 6,10",
  "1,13 11,13 6,22",
  "1,25 11,25 6,34",
  "1,37 11,37 6,46",
  "1,49 11,49 6,58",
  "49,1 59,1 54,10",
  "49,13 59,13 54,22",
  "49,25 59,25 54,34",
  "49,37 59,37 54,46",
  "49,49 59,49 54,58",
  "13,13 23,13 18,22",
  "25,25 35,25 30,34",
  "37,37 47,37 42,46",
];

export function NergalN({ size = 48 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 60 60"
      fill="currentColor"
      role="img"
      aria-label="Nergal"
      style={{ color: "var(--primary)" }}
    >
      {FAVICON_WEDGES.map((points, i) => (
        <polygon key={i} points={points} />
      ))}
    </svg>
  );
}

interface NergalMarkProps {
  width?: number;
  height?: number;
  className?: string;
}

export function NergalMark({ width, height, className }: NergalMarkProps) {
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 -10 100 170"
      aria-hidden="true"
      className={className}
    >
      <path fill="#c53030" d="M 50,-8 L 50,12 L 44,12 Z M 21,21 L 17,3 L 32,14 Z M 14,32 L 14,46 L 0,38 Z M 14,52 L 22,62 L 4,68 Z M 50,12 L 50,74 L 50,138 L 50,160 L 45,138 L 45,74 L 24,72 L 14,54 L 14,32 L 24,14 L 45,12 Z" />
      <path fill="#761c19" d="M 50,-8 L 56,12 L 50,12 Z M 79,21 L 68,14 L 83,3 Z M 100,38 L 86,46 L 86,32 Z M 96,68 L 78,62 L 86,52 Z M 50,12 L 55,12 L 76,14 L 86,32 L 86,54 L 76,72 L 55,74 L 55,138 L 50,160 L 50,138 L 50,74 Z" />
      <path fill="#f13f2e" d="M 24,14 L 45,12 L 50,12 L 50,46 L 46,40 L 46,36 L 24,28 L 14,32 Z" />
      <path fill="#761c19" d="M 14,46 L 24,38 L 36,46 L 28,54 L 14,54 Z" />
      <path fill="#cd0000" d="M 86,46 L 76,38 L 64,46 L 72,54 L 86,54 Z" />
      <path fill="#3d0a08" d="M 76,14 L 55,12 L 50,12 L 50,46 L 54,40 L 54,36 L 76,28 L 86,32 Z" />
      <path fill="#1a0a08" d="M 24,28 L 46,36 L 46,40 L 34,42 L 24,36 Z M 76,28 L 76,36 L 66,42 L 54,40 L 54,36 Z M 46,46 L 54,46 L 56,48 L 50,54 L 44,48 Z M 30,54 L 70,54 L 64,66 L 36,66 Z" />
      <path fill="#f5e8c4" d="M 32,54 L 38,54 L 35,62 Z M 62,54 L 68,54 L 65,62 Z" />
    </svg>
  );
}

export function NergalLogo() {
  const audioRef = useRef<HTMLAudioElement | null>(null);

  const playGrowl = useCallback(() => {
    if (!audioRef.current) {
      const audio = new Audio(growlUrl);
      // WebKitGTK can crash the WebProcess if the underlying GStreamer
      // pipeline cannot construct an audio sink (missing system plugins).
      // Listening to "error" silences the failure path so the click never
      // propagates a NULL-pointer assertion into the renderer.
      audio.addEventListener("error", () => {});
      audioRef.current = audio;
    }
    audioRef.current.currentTime = 0;
    audioRef.current.play().catch(() => {});
  }, []);

  return (
    <div className="flex w-full items-center justify-evenly select-none">
      <NergalN size={48} />

      <button
        type="button"
        onClick={playGrowl}
        className="rounded transition-transform hover:scale-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring active:scale-95"
        aria-label="Play Nergal growl"
      >
        <NergalMark width={36} height={60} />
      </button>
    </div>
  );
}
