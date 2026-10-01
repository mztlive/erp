export type PatchDual = (
    patch: Record<string, string | null | undefined>,
) => void

export function toggleId(ids: readonly string[], id: string): string[] {
    return ids.includes(id) ? ids.filter((v) => v !== id) : [...ids, id]
}
