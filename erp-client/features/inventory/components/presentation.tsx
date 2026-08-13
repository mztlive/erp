"use client"

import { z } from "zod"

import { FilterChip } from "@/components/business/filter-chip"
import type {
    InventoryAvailability,
    InventoryView,
} from "@/features/inventory/types"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"

function parseView(raw: string | null): InventoryView {
    if (
        raw === "movement" ||
        raw === "reservation" ||
        raw === "adjustment" ||
        raw === "balance"
    ) {
        return raw
    }
    return "balance"
}

/** 本地时区 YYYY-MM-DDTHH:mm（datetime-local 控件值）。 */
function localNowInput(): string {
    const d = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function parseAvailability(raw: string | null): InventoryAvailability {
    if (
        raw === "positive" ||
        raw === "zero" ||
        raw === "reserved" ||
        raw === "all"
    ) {
        return raw
    }
    return "all"
}

const MOVEMENT_TYPE_OPTIONS = [
    { value: "PURCHASE_RECEIPT", label: "采购入库" },
    { value: "WAREHOUSE_DISPATCH", label: "仓库发出" },
    { value: "RESERVATION_ESTABLISH", label: "建立预占" },
    { value: "RESERVATION_CONSUME", label: "消耗预占" },
    { value: "STOCK_ADJUSTMENT", label: "库存调整" },
    { value: "OPENING_IMPORT", label: "期初导入" },
] as const

function defaultSortValue(view: InventoryView): string {
    if (view === "balance") return "warehouseCode:asc,skuCode:asc"
    if (view === "movement") return "occurredAt:desc,movementId:desc"
    if (view === "reservation") {
        return "establishedAt:desc,reservationId:desc"
    }
    return "createdAt:desc,adjustmentId:desc"
}

function sortOptions(view: InventoryView) {
    if (view === "balance") {
        return [
            { value: "warehouseCode:asc,skuCode:asc", label: "仓库 / SKU" },
            { value: "lastMovementAt:desc,skuCode:asc", label: "最近变动" },
        ]
    }
    if (view === "movement") {
        return [
            {
                value: "occurredAt:desc,movementId:desc",
                label: "发生时间（新到旧）",
            },
            {
                value: "occurredAt:asc,movementId:asc",
                label: "发生时间（旧到新）",
            },
            {
                value: "recordedAt:desc,movementId:desc",
                label: "记录时间（新到旧）",
            },
        ]
    }
    if (view === "reservation") {
        return [
            {
                value: "establishedAt:desc,reservationId:desc",
                label: "建立时间",
            },
            { value: "salesOrderNo:asc,reservationId:asc", label: "销售单号" },
        ]
    }
    return [
        { value: "createdAt:desc,adjustmentId:desc", label: "创建时间" },
        { value: "adjustmentNo:asc,adjustmentId:asc", label: "调整单号" },
    ]
}

/** 深链/隐形筛选参数的可移除标记：URL 参数与界面控件一一对应。
 *  复用共享 FilterChip（components/business/filter-chip.tsx），保持跨页形态一致。 */
function ChipFilter({
    label,
    onClear,
}: {
    label: string
    onClear: () => void
}) {
    return <FilterChip label={label} onClear={onClear} />
}

function formatQty(value: string, unit: string) {
    return (
        <span className="num text-sm">
            {value}
            <span className="ml-1 text-xs font-normal text-muted-foreground">
                {unit}
            </span>
        </span>
    )
}

const adjustSchema = z.object({
    reasonType: z.enum(["COUNT_GAIN", "COUNT_LOSS", "DAMAGE", "OTHER"]),
    quantity: z
        .string()
        .trim()
        .min(1, "请填写调整数量")
        .refine((v) => {
            try {
                parseDecimal(v, { maxScale: 6 })
                return compareDecimal(v, "0", 6) > 0
            } catch {
                return false
            }
        }, "数量必须为正数"),
    note: z.string().trim().min(2, "请填写至少 2 个字的原因说明"),
    occurredAt: z.string().min(1, "请填写业务发生时间"),
})

export {
    adjustSchema,
    ChipFilter,
    defaultSortValue,
    formatQty,
    localNowInput,
    MOVEMENT_TYPE_OPTIONS,
    parseAvailability,
    parseView,
    sortOptions,
}
