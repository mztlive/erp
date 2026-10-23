import type { NavSectionId } from "./sales-order-detail-model"

/** 主分区合并展示，保留原分区 URL 及工作台深链接。 */
export function salesOrderSectionGroup(section: NavSectionId): NavSectionId {
    switch (section) {
        case "acceptance":
        case "extra-cost":
        case "collaboration":
            return "fulfillment"
        case "receipt":
        case "invoice":
            return "receivable"
        case "returns":
            return "versions"
        case "approval":
            return "timeline"
        default:
            return section
    }
}

export const SALES_ORDER_SECTION_GROUPS: readonly {
    id: NavSectionId
    label: string
}[] = [
    { id: "overview", label: "概览" },
    { id: "fulfillment", label: "交付与验收" },
    { id: "receivable", label: "票款" },
    { id: "versions", label: "变更与售后" },
    { id: "documents", label: "合同凭证" },
    { id: "timeline", label: "全流程" },
]
