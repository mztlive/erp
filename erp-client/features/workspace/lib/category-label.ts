import { TYPE_META, workspaceTypeLabel } from "../api/work-item-meta"

const CATEGORY_LABELS: Record<string, string> = {
    PROCUREMENT_ORDER_CREATION: "供给分配",
    PURCHASE_PAYMENT_ARRANGEMENT: "采购付款安排",
    PURCHASE_SOURCE_UPDATE: "销售变更影响",
}

/** 分类名称沿用任务元数据；未注册类型不直接显示内部枚举。 */
export function workspaceCategoryLabel(type: string, started = false): string {
    if (started) return workspaceTypeLabel("APPROVAL_INSTANCE", type)
    return CATEGORY_LABELS[type] ?? TYPE_META[type]?.label ?? "其他待办"
}
