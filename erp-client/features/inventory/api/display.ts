/**
 * W10 库存台账 · 文案/枚举映射与纯显示辅助。
 * 只做后端 code → 前端 code/中文文案的映射，不发请求、不碰视图对象。
 */

import type { ApiError } from "@/lib/api/errors"
import type {
    AdjustmentReasonType,
    InventoryQuery,
    StockAdjustmentRow,
    StockReservationRow,
} from "@/features/inventory/types"
import {
    AVAILABILITY_LABEL,
    MOVEMENT_TYPE_LABEL,
    REASON_TYPE_OPTIONS,
    VIEW_LABEL,
} from "@/features/inventory/types"

export const OPENING_STOCK_NOTE =
    "期初库存只能通过导入与期初的基准日实盘导入形成流水；旧商城的库存数量不作为 ERP 库存记录，本台账不会写入或展示旧商城库存数据。"

export const EXCLUDED_NOTE =
    "供应商直发、电子交付、线下服务与实体卡不进入本台账的自有实物库存。"

export const SEGREGATION_NOTE =
    "经办提交后进入仓储复核与财务确认，经办本人不得复核或确认入账；余额仅在确认入账完成后由系统更新。"

export function secsToIso(secs: number | null | undefined): string {
    if (secs == null || secs === 0) return ""
    return new Date(secs * 1000).toISOString()
}

export function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

export function movementTypeLabel(code: string): string {
    return (
        MOVEMENT_TYPE_LABEL[code] ??
        MOVEMENT_TYPE_LABEL[frontendMovementType(code)] ??
        code
    )
}

/** Backend SCREAMING → frontend display code used by filters/labels. */
export function frontendMovementType(backend: string): string {
    switch (backend) {
        case "PURCHASE_RECEIPT_IN":
            return "PURCHASE_RECEIPT"
        case "WAREHOUSE_SHIP_OUT":
            return "WAREHOUSE_DISPATCH"
        case "INITIAL":
            return "OPENING_IMPORT"
        case "STOCK_GAIN":
        case "STOCK_LOSS":
        case "DAMAGE":
            return "STOCK_ADJUSTMENT"
        default:
            return backend
    }
}

export function backendMovementTypeFilter(
    types: string[] | undefined,
): string | undefined {
    if (!types?.length) return undefined
    // backend accepts single movement_type; pick first mapped value
    const t = types[0]
    switch (t) {
        case "PURCHASE_RECEIPT":
            return "PURCHASE_RECEIPT_IN"
        case "WAREHOUSE_DISPATCH":
            return "WAREHOUSE_SHIP_OUT"
        case "OPENING_IMPORT":
            return "INITIAL"
        case "STOCK_ADJUSTMENT":
            return "STOCK_GAIN"
        default:
            return t
    }
}

export function directionFrontend(d: string): "increase" | "decrease" {
    const u = d.toUpperCase()
    if (u === "DECREASE" || u === "decrease") return "decrease"
    return "increase"
}

export function reservationStatusLabel(status: string): {
    statusLabel: string
    statusTone: StockReservationRow["statusTone"]
} {
    switch (status) {
        case "ACTIVE":
            return { statusLabel: "有效", statusTone: "success" }
        case "PARTIALLY_CONSUMED":
            return { statusLabel: "部分消耗", statusTone: "warning" }
        case "CONSUMED":
        case "FULLY_CONSUMED":
            return { statusLabel: "已消耗", statusTone: "neutral" }
        case "RELEASED":
            return { statusLabel: "已释放", statusTone: "neutral" }
        default:
            return { statusLabel: status, statusTone: "neutral" }
    }
}

export function reasonTypeFrontend(backend: string): string {
    switch (backend) {
        case "STOCK_GAIN":
            return "COUNT_GAIN"
        case "STOCK_LOSS":
            return "COUNT_LOSS"
        case "DAMAGE":
            return "DAMAGE"
        default:
            return backend
    }
}

export function reasonTypeBackend(frontend: AdjustmentReasonType): string {
    switch (frontend) {
        case "COUNT_GAIN":
            return "STOCK_GAIN"
        case "COUNT_LOSS":
            return "STOCK_LOSS"
        case "DAMAGE":
            return "DAMAGE"
        case "OTHER":
            // backend has no OTHER; map to DAMAGE and rely on note — documented gap
            return "DAMAGE"
    }
}

export function reasonTypeLabel(frontendOrBackend: string): string {
    const fe = reasonTypeFrontend(frontendOrBackend)
    return (
        REASON_TYPE_OPTIONS.find((o) => o.value === fe)?.label ??
        frontendOrBackend
    )
}

export function reasonDirection(reason: string): "increase" | "decrease" {
    const fe = reasonTypeFrontend(reason) as AdjustmentReasonType
    return (
        REASON_TYPE_OPTIONS.find((o) => o.value === fe)?.direction ?? "decrease"
    )
}

export function adjustmentStatusMap(status: string): {
    status: string
    statusLabel: string
    statusTone: StockAdjustmentRow["statusTone"]
} {
    switch (status) {
        case "DRAFT":
            return {
                status: "DRAFT",
                statusLabel: "草稿",
                statusTone: "neutral",
            }
        case "PENDING_WAREHOUSE_REVIEW":
            return {
                status: "PENDING_WAREHOUSE_REVIEW",
                statusLabel: "待仓储复核",
                statusTone: "warning",
            }
        case "PENDING_FINANCE_REVIEW":
            return {
                status: "PENDING_FINANCE",
                statusLabel: "待财务确认",
                statusTone: "info",
            }
        case "POSTED":
            return {
                status: "POSTED",
                statusLabel: "已过账",
                statusTone: "success",
            }
        case "REJECTED":
            return {
                status: "REJECTED",
                statusLabel: "驳回",
                statusTone: "destructive",
            }
        case "REVERSED":
            return {
                status: "REVERSED",
                statusLabel: "已冲正",
                statusTone: "neutral",
            }
        default:
            return { status, statusLabel: status, statusTone: "neutral" }
    }
}

export function filterSummary(
    query: InventoryQuery,
    total: number,
    warehouses: { id: string; name: string }[],
): string {
    const parts = [
        VIEW_LABEL[query.view],
        query.warehouseId
            ? (warehouses.find((w) => w.id === query.warehouseId)?.name ??
              query.warehouseId)
            : "全部仓库",
        query.availability && query.availability !== "all"
            ? AVAILABILITY_LABEL[query.availability]
            : "全部状态",
    ]
    if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
    if (query.skuId) parts.push(`SKU ${query.skuId}`)
    if (query.movementType?.length) {
        parts.push(
            `流水类型 ${query.movementType
                .map((t) => MOVEMENT_TYPE_LABEL[t] ?? t)
                .join("、")}`,
        )
    }
    if (query.view === "movement" && query.occurredFrom && query.occurredTo) {
        parts.push(`${query.occurredFrom} 至 ${query.occurredTo}`)
    }
    parts.push(`${total} 条`)
    return parts.join(" · ")
}
