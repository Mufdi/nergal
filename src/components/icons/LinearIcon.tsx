import { forwardRef, type SVGProps } from "react";
import { siLinear } from "simple-icons";
import { BrandIcon } from "./BrandIcon";

export const LinearIcon = forwardRef<SVGSVGElement, SVGProps<SVGSVGElement> & { size?: number | string }>(
  ({ size = 24, ...props }, ref) => (
    <BrandIcon ref={ref} icon={siLinear} size={size} {...props} />
  ),
);

LinearIcon.displayName = "LinearIcon";
