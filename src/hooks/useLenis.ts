import Lenis from "lenis";
import { useEffect, type RefObject } from "react";

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
  });
}
