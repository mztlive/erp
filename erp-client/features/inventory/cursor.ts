import type { InventoryView } from "@/features/inventory/types"

/** 后端不透明游标；页面只负责原样回传，不从中推导业务事实。 */
export function encodeInventoryCursor(
    view: InventoryView,
    offset: number,
): string {
    return `w10:${view}:${Math.max(0, Math.trunc(offset))}`
}

export function decodeInventoryCursor(
    cursor: string | undefined,
    view: InventoryView,
): number {
    if (!cursor) return 0
    const match = /^w10:(balance|movement|reservation|adjustment):(\d+)$/.exec(
        cursor,
    )
    if (!match || match[1] !== view) return 0
    const offset = Number(match[2])
    return Number.isSafeInteger(offset) ? offset : 0
}
