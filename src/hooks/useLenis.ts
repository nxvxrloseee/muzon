import Lenis from "lenis";
import { useEffect, type RefObject } from "react";

/**
 * Smooth scrolling for plain, non-windowed containers.
 *
 * Deliberately not used on anything backed by `@tanstack/react-virtual`: Lenis
 * drives `scrollTop` from its own rAF loop, and every write makes the
 * virtualizer recompute its window and re-render the list, so smooth scrolling
 * a windowed list costs a full list render per frame. Those containers get
 * `data-lenis-prevent` instead, so the wrapper's Lenis leaves them alone.
 */
export function useLenis(
  wrapperRef: RefObject<HTMLElement | null>,
  contentRef?: RefObject<HTMLElement | null>,
) {
  useEffect(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    const content = contentRef?.current ?? (wrapper.firstElementChild as HTMLElement | null);
    if (!content) return;

    const lenis = new Lenis({ wrapper, content, autoRaf: true });
    return () => lenis.destroy();
    // Refs are stable for the lifetime of the component; without this the
    // effect re-ran on every render, tearing down and rebuilding Lenis (and its
    // rAF loop) each time.
  }, [wrapperRef, contentRef]);
}
