/**
 * ConfirmDialog — Drop-in replacement for browser `confirm()`.
 *
 * Usage:
 *   const [confirmState, requestConfirm] = useConfirm();
 *
 *   // In handler:
 *   const ok = await requestConfirm("Delete this item?", "This cannot be undone.");
 *   if (!ok) return;
 *
 *   // In JSX:
 *   <ConfirmDialog {...confirmState} />
 */
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { useState, useCallback, useRef } from "react";

export interface ConfirmState {
  open: boolean;
  title: string;
  description: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Hook that returns [state, requestConfirm]. Bind state to <ConfirmDialog />. */
export function useConfirm(): [ConfirmState, (title: string, description?: string) => Promise<boolean>] {
  const [state, setState] = useState<ConfirmState>({
    open: false,
    title: "",
    description: "",
    onConfirm: () => {},
    onCancel: () => {},
  });

  const resolveRef = useRef<((v: boolean) => void) | null>(null);

  const requestConfirm = useCallback(
    (title: string, description = "") =>
      new Promise<boolean>((resolve) => {
        resolveRef.current = resolve;
        setState({
          open: true,
          title,
          description,
          onConfirm: () => {
            resolve(true);
            setState((s) => ({ ...s, open: false }));
          },
          onCancel: () => {
            resolve(false);
            setState((s) => ({ ...s, open: false }));
          },
        });
      }),
    []
  );

  return [state, requestConfirm];
}

export default function ConfirmDialog({
  open,
  title,
  description,
  onConfirm,
  onCancel,
}: ConfirmState) {
  return (
    <AlertDialog open={open} onOpenChange={(v) => { if (!v) onCancel(); }}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          {description && (
            <AlertDialogDescription>{description}</AlertDialogDescription>
          )}
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel onClick={onCancel}>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            className="bg-destructive text-white hover:bg-destructive/90"
          >
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
