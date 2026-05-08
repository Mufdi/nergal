"use client"

import * as ResizablePrimitive from "react-resizable-panels"

import { cn } from "@/lib/utils"

// react-resizable-panels v4 inlines `overflow: hidden` on the Group and
// `overflow: auto` on the inner div of each Panel. Both clip our panel
// glow's outer halo into rectangular shapes at the corners. Spreading
// `style: { overflow: "visible" }` overrides those inline values (the
// library spreads user style AFTER its own defaults, so `overflow`
// takes precedence). Children panels keep their own `overflow-hidden`
// to clip their content — only the wrapping containers go visible.
function ResizablePanelGroup({
  className,
  style,
  ...props
}: ResizablePrimitive.GroupProps) {
  return (
    <ResizablePrimitive.Group
      data-slot="resizable-panel-group"
      className={cn(
        "flex h-full w-full",
        className
      )}
      style={{ overflow: "visible", ...style }}
      {...props}
    />
  )
}

function ResizablePanel({ style, ...props }: ResizablePrimitive.PanelProps) {
  return (
    <ResizablePrimitive.Panel
      data-slot="resizable-panel"
      style={{ overflow: "visible", ...style }}
      {...props}
    />
  )
}

function ResizableHandle({
  className,
  ...props
}: ResizablePrimitive.SeparatorProps) {
  return (
    <ResizablePrimitive.Separator
      data-slot="resizable-handle"
      className={cn(
        "group relative flex shrink-0 items-center justify-center",
        "aria-[orientation=vertical]:w-2 aria-[orientation=vertical]:cursor-col-resize",
        "aria-[orientation=horizontal]:h-2 aria-[orientation=horizontal]:w-full aria-[orientation=horizontal]:cursor-row-resize",
        "select-none outline-none",
        className
      )}
      style={{ background: "transparent", outline: "none" }}
      {...props}
    />
  )
}

export { ResizableHandle, ResizablePanel, ResizablePanelGroup }
