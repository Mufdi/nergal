import { forwardRef, type SVGProps } from "react";

export interface SimpleIcon {
  title: string;
  slug: string;
  hex: string;
  path: string;
  source: string;
}

interface BrandIconProps extends Omit<SVGProps<SVGSVGElement>, "fill"> {
  icon: SimpleIcon;
  size?: number | string;
  useBrandColor?: boolean;
}

export const BrandIcon = forwardRef<SVGSVGElement, BrandIconProps>(
  ({ icon, size = 24, useBrandColor = false, ...props }, ref) => (
    <svg
      ref={ref}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={useBrandColor ? `#${icon.hex}` : "currentColor"}
      role="img"
      aria-label={icon.title}
      {...props}
    >
      <title>{icon.title}</title>
      <path d={icon.path} />
    </svg>
  ),
);

BrandIcon.displayName = "BrandIcon";
