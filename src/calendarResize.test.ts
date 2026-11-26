import { afterEach, describe, expect, it, vi } from "vitest";
import { observeCalendarSize } from "./calendarResize";

afterEach(() => vi.unstubAllGlobals());

describe("calendar container resizing", () => {
  function setup() {
    let notify: (entries: ResizeObserverEntry[]) => void = () => {};
    let frame: (() => void) | undefined;
    const disconnect = vi.fn();
    const observe = vi.fn();
    const cancel = vi.fn(() => {
      frame = undefined;
    });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: typeof notify) {
          notify = callback;
        }
        observe = observe;
        disconnect = disconnect;
      }
    );
    vi.stubGlobal("requestAnimationFrame", (callback: () => void) => {
      frame = callback;
      return 1;
    });
    vi.stubGlobal("cancelAnimationFrame", cancel);
    const update = vi.fn();
    const element = {} as HTMLElement;
    const stop = observeCalendarSize(element, update);
    const resize = (width: number, height: number) =>
      notify([{ contentRect: { width, height } } as ResizeObserverEntry]);
    const flush = () => {
      const callback = frame;
      frame = undefined;
      callback?.();
    };
    return { observe, disconnect, update, stop, resize, flush, element };
  }

  it("recalculates for half-width windows and toolbar height changes", () => {
    const s = setup();
    expect(s.observe).toHaveBeenCalledWith(s.element);
    for (const [width, height] of [
      [1256, 800],
      [519, 760],
      [519, 720],
      [1256, 800],
    ]) {
      s.resize(width, height);
      s.flush();
    }
    expect(s.update).toHaveBeenCalledTimes(4);
    s.stop();
  });

  it("coalesces resizing and ignores unchanged or hidden containers", () => {
    const s = setup();
    s.resize(700, 800);
    s.resize(519, 760);
    s.flush();
    expect(s.update).toHaveBeenCalledTimes(1);
    s.resize(519, 760);
    s.resize(0, 0);
    s.flush();
    expect(s.update).toHaveBeenCalledTimes(1);
    s.stop();
  });

  it("disconnects and cancels queued work on unmount", () => {
    const s = setup();
    s.resize(519, 760);
    s.stop();
    s.flush();
    expect(s.disconnect).toHaveBeenCalledOnce();
    expect(s.update).not.toHaveBeenCalled();
  });
});
