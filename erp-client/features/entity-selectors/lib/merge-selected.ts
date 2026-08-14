/** 把已选项并入列表头部（若尚未出现），保证选中值始终可见。 */
export function mergeSelected<T>(
    rows: readonly T[] | undefined,
    selected: T | null | undefined,
    idOf: (item: T) => string,
): readonly T[] {
    if (!selected) return rows ?? []
    return (rows ?? []).some((item) => idOf(item) === idOf(selected))
        ? (rows ?? [])
        : [selected, ...(rows ?? [])]
}
