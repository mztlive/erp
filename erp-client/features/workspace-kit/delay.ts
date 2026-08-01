/** Simulated network latency for mock queryFns. */
export function mockDelay(ms = 80): Promise<void> {
  return new Promise((resolve) => {
    globalThis.setTimeout(resolve, ms)
  })
}
