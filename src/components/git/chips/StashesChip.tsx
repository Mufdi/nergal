interface StashesChipProps {
  sessionId: string;
}

export function StashesChip({ sessionId: _sessionId }: StashesChipProps) {
  return (
    <div className="flex h-full items-center justify-center px-6">
      <div className="text-center">
        <p className="text-[11px] text-muted-foreground/80">Stashes coming in phase 5</p>
        <p className="mt-1 text-[10px] text-muted-foreground/50">Backend ready · UI in progress</p>
      </div>
    </div>
  );
}
