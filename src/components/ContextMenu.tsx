import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export type ContextMenuItem = {
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  separatorBefore?: boolean;
};

type Props = {
  x: number;
  y: number;
  title: string;
  subtitle?: string;
  items: ContextMenuItem[];
  onAction: (id: string) => void;
  onClose: (restoreFocus: boolean) => void;
};

export function ContextMenu({ x, y, title, subtitle, items, onAction, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });

  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    const bounds = menu.getBoundingClientRect();
    setPosition({
      left: Math.max(8, Math.min(x, window.innerWidth - bounds.width - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - bounds.height - 8)),
    });
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y]);

  useEffect(() => {
    const outside = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) onClose(false);
    };
    const scroll = (event: Event) => {
      if (!ref.current?.contains(event.target as Node)) onClose(false);
    };
    const resize = () => onClose(false);
    const key = (event: KeyboardEvent) => {
      // Prevent calendar shortcuts, even when focus is on a menu item.
      event.stopImmediatePropagation();
      const buttons = [
        ...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []),
      ];
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const shortcut =
        (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "c"
          ? "copy"
          : !event.ctrlKey && !event.metaKey && event.key.toLowerCase() === "e"
            ? "edit"
            : event.key === "Delete"
              ? "delete"
              : undefined;
      if (shortcut && items.some((item) => item.id === shortcut && !item.disabled)) {
        event.preventDefault();
        onAction(shortcut);
        return;
      }
      if (event.key === "Escape" || event.key === "Tab") {
        event.preventDefault();
        onClose(true);
      } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        event.preventDefault();
        const next =
          event.key === "Home"
            ? 0
            : event.key === "End"
              ? buttons.length - 1
              : (index + (event.key === "ArrowUp" ? -1 : 1) + buttons.length) % buttons.length;
        buttons[next]?.focus();
      } else if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        buttons[index]?.click();
      } else {
        event.preventDefault();
      }
    };
    document.addEventListener("pointerdown", outside, true);
    window.addEventListener("scroll", scroll, true);
    window.addEventListener("resize", resize);
    window.addEventListener("keydown", key, true);
    return () => {
      document.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("scroll", scroll, true);
      window.removeEventListener("resize", resize);
      window.removeEventListener("keydown", key, true);
    };
  }, [onClose, onAction, items]);

  return createPortal(
    <div
      ref={ref}
      className="event-context-menu"
      role="menu"
      aria-label={`Actions for ${title}`}
      style={position}
      onContextMenu={(event) => event.preventDefault()}
    >
      <div className="context-menu-heading" role="presentation">
        <strong>{title}</strong>
        {subtitle && <span>{subtitle}</span>}
      </div>
      {items.map((item) => (
        <div key={item.id} role="presentation">
          {item.separatorBefore && <div className="context-menu-separator" role="separator" />}
          <button
            type="button"
            role="menuitem"
            tabIndex={-1}
            disabled={item.disabled}
            className={item.danger ? "context-menu-danger" : undefined}
            onClick={() => onAction(item.id)}
          >
            <span>{item.label}</span>
            {item.shortcut && <kbd>{item.shortcut}</kbd>}
          </button>
        </div>
      ))}
    </div>,
    document.body
  );
}
