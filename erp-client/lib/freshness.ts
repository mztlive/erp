export type FreshnessDemoState = "stale" | "rebuilding" | "failed"

export function resolveFreshness<T>(
  demo: FreshnessDemoState | undefined,
  states: {
    fresh: T
    stale: T
    rebuilding: T
    failed: T
  }
): T {
  if (demo === "stale") return states.stale
  if (demo === "rebuilding") return states.rebuilding
  if (demo === "failed") return states.failed
  return states.fresh
}
