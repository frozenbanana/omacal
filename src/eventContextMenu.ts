import type { ContextMenuItem } from "./components/ContextMenu";
import type { CalEvent } from "./store";

export function eventContextItems(event: CalEvent, canDuplicate: boolean): ContextMenuItem[] {
  const recurring = !!event.rrule;
  const writable = !event.readonly;
  const items: ContextMenuItem[] = [{ id: "details", label: "Open details" }];
  if (recurring) {
    items.push(
      {
        id: "edit-single",
        label: "Edit this occurrence…",
        disabled: !writable || !event.recurrence_id,
      },
      { id: "edit-series", label: "Edit entire series…", disabled: !writable }
    );
  } else {
    items.push({ id: "edit", label: "Edit…", shortcut: "E", disabled: !writable });
  }
  items.push(
    {
      id: "copy",
      label: recurring ? "Copy this occurrence" : "Copy",
      shortcut: "Ctrl+C",
      separatorBefore: true,
    },
    {
      id: "duplicate",
      label: recurring ? "Duplicate this occurrence…" : "Duplicate…",
      disabled: !canDuplicate,
    },
    {
      id: "delete",
      label: "Delete…",
      shortcut: "Del",
      danger: true,
      separatorBefore: true,
      disabled: !writable,
    }
  );
  return items;
}
