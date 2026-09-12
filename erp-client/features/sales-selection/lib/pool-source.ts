/**
 * 商品池来源：由勾选自动决定，不让销售在弹窗里再选路径。
 */

import type { PoolSourceKind } from "@/features/sales-selection/types"

/**
 * 已勾选则按勾选冻结，否则按当前筛选。
 * @param selectedCount 当前勾选件数
 */
export function resolvePoolSourceKind(selectedCount: number): PoolSourceKind {
    return selectedCount > 0 ? "SELECTION" : "FILTER"
}

/**
 * 发起选品时展示的来源摘要，只读、不暴露身份字段。
 * @param input.kind 筛选或勾选
 * @param input.itemCount 将进入选品册的件数
 * @param input.filterLabel 已生效筛选的业务描述；空则视为全部可售
 */
export function describePoolSource(input: {
    kind: PoolSourceKind
    itemCount: number
    filterLabel: string
}): string {
    if (input.kind === "SELECTION") {
        return `已勾选 ${input.itemCount} 件`
    }
    const filter = input.filterLabel.trim() || "全部可售"
    return `当前筛选 · ${filter} · ${input.itemCount} 件`
}
