/**
 * W10 库存台账 · 服务端分页/排序适配（纯函数）。
 * 前端排序 token、日期区间与游标页码互转，不发起请求。
 */

import type { InventoryQuery } from "@/features/inventory/types"
import {
    decodeInventoryCursor,
    encodeInventoryCursor,
} from "@/features/inventory/lib/cursor"

export function dateToUnixStart(date: string | undefined): number | undefined {
    if (!date) return undefined
    const t = Date.parse(`${date}T00:00:00`)
    return Number.isFinite(t) ? Math.floor(t / 1000) : undefined
}

export function dateToUnixEnd(date: string | undefined): number | undefined {
    if (!date) return undefined
    const t = Date.parse(`${date}T23:59:59`)
    return Number.isFinite(t) ? Math.floor(t / 1000) : undefined
}

export function sortTokenToBackend(
    sort: string[],
    view: InventoryQuery["view"],
): { sort_by?: string; sort_dir?: "asc" | "desc" } {
    const token = sort[0]
    if (!token) {
        if (view === "balance") return { sort_by: "sku_id", sort_dir: "asc" }
        if (view === "movement")
            return { sort_by: "occurred_at", sort_dir: "desc" }
        if (view === "reservation")
            return { sort_by: "created_at", sort_dir: "desc" }
        return { sort_by: "created_at", sort_dir: "desc" }
    }
    const [field, dir] = token.split(":")
    const sort_dir = dir === "asc" ? "asc" : "desc"
    // map frontend field names to backend whitelist
    const map: Record<string, string> = {
        warehouseCode: "sku_id",
        skuCode: "sku_id",
        lastMovementAt: "created_at",
        occurredAt: "occurred_at",
        recordedAt: "recorded_at",
        movementId: "created_at",
        establishedAt: "created_at",
        reservationId: "created_at",
        salesOrderNo: "created_at",
        createdAt: "created_at",
        adjustmentId: "created_at",
        adjustmentNo: "adjustment_no",
    }
    return { sort_by: map[field] ?? field, sort_dir }
}

export function pageFromCursor(
    cursor: string | undefined,
    view: InventoryQuery["view"],
    pageSize: number,
): number {
    const offset = decodeInventoryCursor(cursor, view)
    return Math.floor(offset / pageSize) + 1
}

export function cursorsFromPage(
    view: InventoryQuery["view"],
    page: number,
    pageSize: number,
    total: number,
): { cursor: string; nextCursor?: string; previousCursor?: string } {
    const offset = (page - 1) * pageSize
    const cursor = offset === 0 ? "" : encodeInventoryCursor(view, offset)
    const nextOffset = offset + pageSize
    const nextCursor =
        nextOffset < total ? encodeInventoryCursor(view, nextOffset) : undefined
    const previousOffset = Math.max(0, offset - pageSize)
    const previousCursor =
        offset > 0
            ? previousOffset === 0
                ? ""
                : encodeInventoryCursor(view, previousOffset)
            : undefined
    return { cursor, nextCursor, previousCursor }
}
