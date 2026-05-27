import { forwardRef, type SVGProps } from "react";
import { siObsidian } from "simple-icons";
import { BrandIcon } from "./BrandIcon";

export const ObsidianIcon = forwardRef<SVGSVGElement, SVGProps<SVGSVGElement> & { size?: number | string }>(
  ({ size = 24, ...props }, ref) => (
    <BrandIcon ref={ref} icon={siObsidian} size={size} {...props} />
  ),
);

ObsidianIcon.displayName = "ObsidianIcon";
