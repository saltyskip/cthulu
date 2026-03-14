import type { ReactNode, MouseEvent } from "react";

interface EntityRowProps {
  leading?: ReactNode;
  title: string;
  subtitle?: string;
  trailing?: ReactNode;
  selected?: boolean;
  onClick?: (e: MouseEvent) => void;
  className?: string;
}

export function EntityRow({
  leading, title, subtitle, trailing, selected, onClick, className,
}: EntityRowProps) {
  return (
    <div
      className={`entity-row${selected ? " entity-row-selected" : ""}${className ? ` ${className}` : ""}`}
      onClick={onClick}
      style={{ cursor: onClick ? "pointer" : undefined }}
    >
      {leading && <div className="entity-row-leading">{leading}</div>}
      <div className="entity-row-body">
        <span className="entity-row-title">{title}</span>
        {subtitle && <span className="entity-row-subtitle">{subtitle}</span>}
      </div>
      {trailing && <div className="entity-row-trailing">{trailing}</div>}
    </div>
  );
}
