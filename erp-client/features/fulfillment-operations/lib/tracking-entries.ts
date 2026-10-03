import type { DeliveryTrackingEntry, DeliveryTrackingEntryDto } from "../types"
import {
    MAX_TRACKING_NUMBERS,
    MAX_TRACKING_NUMBER_LENGTH,
} from "./tracking-numbers"

/** 保留每个包裹的明细归属和承运方，不跨销售行合并。 */
export function trackingEntriesFromDto(
    entries?: readonly DeliveryTrackingEntryDto[] | null,
): DeliveryTrackingEntry[] {
    return (entries ?? []).map((entry) => ({
        salesOrderLineId: entry.sales_order_line_id,
        trackingNo: entry.tracking_no,
        carrier: entry.carrier ?? undefined,
    }))
}

export function trackingEntriesToDto(
    entries?: readonly DeliveryTrackingEntry[],
): DeliveryTrackingEntryDto[] {
    return (entries ?? []).map((entry) => ({
        sales_order_line_id: entry.salesOrderLineId,
        tracking_no: entry.trackingNo.trim(),
        carrier: entry.carrier?.trim() || null,
    }))
}

/** 仅同一明细、同一承运方、同一单号属于重复录入。 */
export function trackingEntryKey(entry: DeliveryTrackingEntry): string {
    return JSON.stringify([
        entry.salesOrderLineId,
        entry.carrier ?? "",
        entry.trackingNo,
    ])
}

export function trackingEntriesValidationMessage(
    entries: readonly DeliveryTrackingEntry[] | undefined,
    allowedLineIds: readonly string[],
): string | null {
    if (!entries?.length) return "请按发货明细添加物流号"
    if (entries.length > MAX_TRACKING_NUMBERS)
        return `本次发货最多添加 ${MAX_TRACKING_NUMBERS} 条包裹明细`
    const allowed = new Set(allowedLineIds)
    if (entries.some((entry) => !allowed.has(entry.salesOrderLineId)))
        return "有物流号不属于本次发货明细，请移除对应物流号后重试"
    if (
        entries.some(
            (entry) =>
                !entry.trackingNo.trim() ||
                entry.trackingNo.trim().length > MAX_TRACKING_NUMBER_LENGTH,
        )
    )
        return `请填写有效物流号，每个最多 ${MAX_TRACKING_NUMBER_LENGTH} 个字符`
    if (entries.some((entry) => (entry.carrier?.trim().length ?? 0) > 64))
        return "承运方最多64个字符"
    return null
}
