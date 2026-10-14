import { compactFixed } from "@/lib/fixed-decimal"

/** 销售变更对采购某一行的影响原因（服务端扁平字段，按 kind 取值）。 */
export type SourceImpactReasonWire = {
    sales_order_line_id: string
    line_no: number
    item_name: string
    kind: "ZEROED" | "QUANTITY_REDUCED" | "DUE_EARLIER" | "DELIVERY_CHANGED"
    baseline_quantity?: string
    current_quantity?: string
    excess_quantity?: string
    current_due?: string
    expected_delivery_date?: string
}

/** 界面使用的影响原因。 */
export type SourceImpactReasonView = {
    lineNo: number
    itemName: string
    /** 用户可读的影响描述，不含行号与品名。 */
    description: string
}

/** 把服务端影响原因转成一句用户可读的描述。 */
export function describeSourceImpact(reason: SourceImpactReasonWire): string {
    switch (reason.kind) {
        case "ZEROED":
            return "销售数量已归零"
        case "QUANTITY_REDUCED":
            return `销售数量 ${compactFixed(reason.baseline_quantity ?? "0")} → ${compactFixed(reason.current_quantity ?? "0")}，已采购超出 ${compactFixed(reason.excess_quantity ?? "0")}`
        case "DUE_EARLIER":
            return `履约期限提前到 ${reason.current_due ?? "—"}，早于预计交付 ${reason.expected_delivery_date ?? "—"}`
        case "DELIVERY_CHANGED":
            return "收件地址或数量已变化"
    }
}

/** 映射服务端影响原因列表。 */
export function mapSourceImpactReasons(
    reasons: readonly SourceImpactReasonWire[] | undefined,
): SourceImpactReasonView[] {
    return (reasons ?? []).map((reason) => ({
        lineNo: reason.line_no,
        itemName: reason.item_name,
        description: describeSourceImpact(reason),
    }))
}
