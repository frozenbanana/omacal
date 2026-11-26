// FullCalendar listens to window.resize, but sidebar/toolbar layout changes can
// resize its container independently. Measure the actual available area.
export function observeCalendarSize(element: HTMLElement, updateSize: () => void): () => void {
  let frame: number | undefined;
  let width = -1;
  let height = -1;
  const schedule = () => {
    if (frame !== undefined) cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      frame = undefined;
      updateSize();
    });
  };
  if (typeof ResizeObserver === "undefined") {
    window.addEventListener("resize", schedule);
    return () => {
      window.removeEventListener("resize", schedule);
      if (frame !== undefined) cancelAnimationFrame(frame);
    };
  }
  const observer = new ResizeObserver(([entry]) => {
    if (!entry) return;
    const next = entry.contentRect;
    if (next.width <= 0 || next.height <= 0 || (width === next.width && height === next.height))
      return;
    width = next.width;
    height = next.height;
    schedule();
  });
  observer.observe(element);
  return () => {
    observer.disconnect();
    if (frame !== undefined) cancelAnimationFrame(frame);
  };
}
