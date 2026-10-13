"use client"

import { Badge } from "@/components/ui/badge"
import type { PurchaseChangeCoverageEffect } from "@/features/purchase-orders/types"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"

function positive(value: string) {
    try {
        return compareDecimal(value, "0", 6) > 0
    } catch {
        return false
    }
}

/**
 * 在途采购改单对销售覆盖的预判（第一期 §6.3 采购改单执行规则）。
 * 与销售改单的「预计影响的采购」对应，供审批人判断缺口是否另有安排。
 */
export function PurchaseChangeCoverageEffects({
    id,
    effects,
}: {
    id: string
    effects: readonly PurchaseChangeCoverageEffect[]
}) {
    if (effects.length === 0) return null
    const gaps = effects.filter((effect) => positive(effect.gapQuantity)).length
    return (
        <section
            id={id}
            aria-labelledby={`${id}-title`}
            className="space-y-3 rounded-xl border border-border/70 px-4 py-4 md:px-5"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 id={`${id}-title`} className="text-sm font-semibold">
                    对销售的影响
                </h3>
                <Badge variant={gaps ? "warning" : "secondary"}>
                    {gaps ? `${gaps} 项产生缺口` : "不产生缺口"}
                </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
                按本次提交内容预判，生效时按当时事实复核；缺口生效后回到供给分配，由采购负责人重新安排供应。
            </p>
            <ul className="divide-y divide-border/60">
                {effects.map((effect) => (
                    <li
                        key={effect.salesOrderLineId}
                        className="space-y-0.5 py-2 text-sm first:pt-0 last:pb-0"
                    >
                        <p>
                            第 {effect.lineNo} 行 {effect.itemName}
                            <span className="num ml-2 text-muted-foreground">
                                {compactFixed(effect.baseQuantity)} →{" "}
                                {compactFixed(effect.targetQuantity)}
                            </span>
                        </p>
                        <p className="num text-xs text-muted-foreground">
                            销售需要 {compactFixed(effect.salesQuantity)}
                            ，其他采购与库存{" "}
                            {compactFixed(effect.otherCoveredQuantity)}
                            {positive(effect.gapQuantity)
                                ? `；改后还差 ${compactFixed(effect.gapQuantity)}`
                                : positive(effect.excessQuantity)
                                  ? `；改后仍超出 ${compactFixed(effect.excessQuantity)}`
                                  : "；改后正好覆盖"}
                        </p>
                    </li>
                ))}
            </ul>
        </section>
    )
}
