export function moveListItem<T>(
    items: readonly T[],
    fromIndex: number,
    toIndex: number,
): T[] {
    if (toIndex < 0 || toIndex >= items.length || fromIndex === toIndex) {
        return [...items]
    }
    const next = [...items]
    const [item] = next.splice(fromIndex, 1)
    if (item !== undefined) next.splice(toIndex, 0, item)
    return next
}
