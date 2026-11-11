import clsx from "clsx";
import { Ellipsis } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { IconButton } from "./ui";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  /// Marks an item that removes something.
  danger?: boolean;
  /// For a choice among several: marks the one now in effect.
  checked?: boolean;
}

/// An ellipsis button that opens a short menu. It closes on a choice, on a press
/// outside it, and on Escape. With no items it shows nothing, so a row can always
/// render it and let its items decide.
export function RowMenu({ label, items }: { label: string; items: MenuItem[] }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function away(e: MouseEvent) {
      if (root.current && !root.current.contains(e.target as Node)) setOpen(false);
    }
    function escape(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  if (items.length === 0) return null;
  return (
    <div ref={root} className="relative shrink-0">
      <IconButton
        type="button"
        aria-label={`${label} menu`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <Ellipsis size={16} />
      </IconButton>
      {open && (
        <div
          role="menu"
          className="absolute right-0 z-20 mt-1 min-w-[180px] rounded-md border border-border bg-surface-elevated py-1 shadow-lg"
        >
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              role={item.checked === undefined ? "menuitem" : "menuitemradio"}
              aria-checked={item.checked}
              onClick={() => {
                setOpen(false);
                item.onSelect();
              }}
              className={clsx(
                "flex w-full items-center gap-2 px-3 py-1.5 text-left text-[13px] hover:bg-surface-hover focus-visible:bg-surface-hover focus-visible:outline-none",
                item.danger ? "text-error" : "text-text",
              )}
            >
              {item.checked !== undefined && (
                <span aria-hidden className="w-3 text-accent-bright">
                  {item.checked ? "✓" : ""}
                </span>
              )}
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
