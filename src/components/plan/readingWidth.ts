export const MARKDOWN_CONTAINER_CLASS = "@container";

// The margin resolves to 1rem from the container edge for any centered column
// width (and to 0 with reading mode off), so the table spans the whole view.
export function markdownTableClass(fullWidth: boolean): string {
  const width = fullWidth ? "w-[calc(100cqw_-_2rem)] ml-[calc(50%_-_50cqw_+_1rem)]" : "w-full";
  return `my-2 ${width} border-collapse text-xs`;
}

export function markdownWidthClass(enabled: boolean): string {
  return enabled ? "mx-auto max-w-[100ch]" : "max-w-none";
}
