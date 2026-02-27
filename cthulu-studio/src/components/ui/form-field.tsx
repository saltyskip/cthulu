import * as React from "react";
import { Label } from "@/components/ui/label";

interface FormFieldProps {
  label: string;
  error?: string;
  children: React.ReactNode;
}

function FormField({ label, error, children }: FormFieldProps) {
  return (
    <div className="form-group">
      <Label className="text-[11px] font-semibold uppercase tracking-wider text-[var(--text-secondary)]">
        {label}
      </Label>
      {children}
      {error && <span className="field-error">{error}</span>}
    </div>
  );
}

export { FormField };
