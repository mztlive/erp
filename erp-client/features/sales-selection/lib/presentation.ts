/**
 * 选品册列表/详情展示口径：状态语气、时间与公开链接。
 */

import { formatDateTime } from "@/lib/datetime"
import type { StatusTone } from "@/components/ui/status-badge"
import type { BookletStatus } from "@/features/sales-selection/types"

/** 列表工作视图：状态即时生效，不进「查询」草稿。 */
export const BOOK_STATUS_VIEWS = [
    { value: "ALL", label: "全部" },
    { value: "DRAFT", label: "草稿" },
    { value: "PREPARING", label: "准备中" },
    { value: "PENDING_PUBLISH", label: "待发布" },
    { value: "PUBLISHED", label: "已发布" },
    { value: "SUBMITTED", label: "已提交" },
    { value: "CLOSED", label: "已关闭" },
    { value: "VOIDED", label: "已作废" },
] as const

export type BookStatusView = (typeof BOOK_STATUS_VIEWS)[number]["value"]

/** 状态徽标语气：终态、待办与进行中可一眼区分。 */
export function bookletStatusTone(status: BookletStatus): StatusTone {
    switch (status) {
        case "SUBMITTED":
            return "success"
        case "PUBLISHED":
            return "info"
        case "PREPARING":
        case "PENDING_PUBLISH":
            return "warning"
        case "CLOSED":
        case "VOIDED":
            return "void"
        default:
            return "neutral"
    }
}

const dateOnlyPattern = /^\d{4}-\d{2}-\d{2}$/
const digitsPattern = /^\d+$/

/**
 * 选品册时间展示。Unix 秒/毫秒、ISO 与资格日（YYYY-MM-DD）走同一出口。
 */
export function formatBookInstant(value?: number | string | null): string {
    if (value == null || value === "") return "—"
    if (typeof value === "string" && dateOnlyPattern.test(value)) return value
    if (typeof value === "number") {
        const ms = value < 1e12 ? value * 1000 : value
        return formatDateTime(new Date(ms).toISOString(), "full")
    }
    if (digitsPattern.test(value)) return formatBookInstant(Number(value))
    return formatDateTime(value, "full")
}

/** 把后端公开路径拼成可复制的完整地址。 */
export function publicSelectionHref(
    pathOrUrl?: string | null,
    origin?: string,
): string | null {
    const value = pathOrUrl?.trim()
    if (!value) return null
    if (/^https?:\/\//i.test(value)) return value
    const base =
        origin ?? (typeof window === "undefined" ? "" : window.location.origin)
    if (!base) return value
    return `${base}${value.startsWith("/") ? value : `/${value}`}`
}

/** 列表行稳定身份：优先 book_id，兼容只返回 id 的旧载荷。 */
export function bookIdentity(row: { book_id?: string; id: string }): string {
    return row.book_id ?? row.id
}
