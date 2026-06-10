import { forwardRef, type SVGProps } from "react";
import { siClickup } from "simple-icons";
import { BrandIcon } from "./BrandIcon";

export const ClickUpIcon = forwardRef<SVGSVGElement, SVGProps<SVGSVGElement> & { size?: number | string }>(
  ({ size = 24, ...props }, ref) => (
    <BrandIcon ref={ref} icon={siClickup} size={size} {...props} />
  ),
);

ClickUpIcon.displayName = "ClickUpIcon";
