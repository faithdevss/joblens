type IconProps = {
  name: string;
  className?: string;
};

/** Material Symbols Rounded glyph. Decorative; pair with visible text. */
export function Icon({ name, className }: IconProps) {
  return (
    <span className={className ? `icon ${className}` : "icon"} aria-hidden="true">
      {name}
    </span>
  );
}
