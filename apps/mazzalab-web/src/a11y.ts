import { useEffect } from "react";

/** Whether a control with only an icon in it still needs an accessible name taken from its title. */
export function needsLabel(control: { title: string; ariaLabel: string | null; labelledBy: string | null; text: string }): boolean {
  return control.title.trim() !== "" && control.text.trim() === "" && !control.ariaLabel && !control.labelledBy;
}

/** The ids that are in `now` and were not in `before`, in order. */
export function newlyAdded(before: readonly string[], now: readonly string[]): string[] {
  const known = new Set(before);
  return now.filter((id) => !known.has(id));
}

function label(root: ParentNode): void {
  for (const element of root.querySelectorAll<HTMLElement>("button[title], [role=button][title]")) {
    if (needsLabel({ title: element.title, ariaLabel: element.getAttribute("aria-label"), labelledBy: element.getAttribute("aria-labelledby"), text: element.textContent ?? "" })) {
      // Not for buttons whose icon says it all and whose title is a shortcut hint only: the title is the name.
      element.setAttribute("aria-label", element.title.replace(/\s*\([^)]*\)\s*$/, ""));
    }
  }
}

/**
 * Icon-only buttons already carry their name as a title for the pointer;
 * this gives the same words to screen readers, for the buttons that have
 * none of their own, as they appear.
 */
export function useTitleLabels(): void {
  useEffect(() => {
    label(document);
    let queued = false;
    const observer = new MutationObserver(() => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        label(document);
      });
    });
    observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["title"] });
    return () => observer.disconnect();
  }, []);
}
