export function matchSearch(
  q: string | undefined,
  parts: readonly (string | undefined)[]
): boolean {
  if (!q?.trim()) return true
  const needle = q.trim().toLowerCase()
  return parts.some((p) => p?.toLowerCase().includes(needle))
}

export function filterRowsBySearch<T>(
  rows: readonly T[],
  q: string | undefined,
  pickParts: (row: T) => readonly (string | undefined)[]
): T[] {
  if (!q?.trim()) return [...rows]
  const needle = q.trim().toLowerCase()
  return rows.filter((row) =>
    pickParts(row).some((p) => p?.toLowerCase().includes(needle))
  )
}
