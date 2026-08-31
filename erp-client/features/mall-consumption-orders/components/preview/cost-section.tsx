"use client"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { derivePrimaryCostBasis } from "@/features/mall-consumption-orders/lib/preview-cost-basis"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import {
    COST_BASIS_LABEL,
    COST_BASIS_TONE,
} from "@/features/mall-consumption-orders/types"
import { SectionTitle } from "./section-title"

type Props = {
    view: MallConsumptionOrderView
}

export function CostSection({ view }: Props) {
    const costBasisPrimary = derivePrimaryCostBasis(view)

    return (
        <section className="space-y-2" aria-label="成本口径">
            <SectionTitle>成本口径</SectionTitle>
            <div className="flex flex-wrap items-center gap-2">
                <BusinessStatusBadge
                    context="list"
                    label={COST_BASIS_LABEL[costBasisPrimary]}
                    tone={COST_BASIS_TONE[costBasisPrimary]}
                />
                {costBasisPrimary === "NONE" ? (
                    <Badge variant="outline">
                        金额为空，不按零成本计入利润
                    </Badge>
                ) : null}
            </div>
            {view.consumptionEntries.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    尚无消费条目成本评估（待归集时常见）。支付记录与订单仍保留。
                </p>
            ) : (
                <ul className="space-y-2">
                    {view.consumptionEntries.map((entry) => {
                        const ca = entry.currentCostAssessment
                        return (
                            <li
                                key={entry.consumptionEntryId}
                                className="rounded-lg border border-border bg-card px-3 py-2 text-xs"
                            >
                                <div className="flex flex-wrap items-center gap-2">
                                    <BusinessStatusBadge
                                        context="list"
                                        label={COST_BASIS_LABEL[ca.costBasis]}
                                        tone={COST_BASIS_TONE[ca.costBasis]}
                                    />
                                    <span className="text-muted-foreground">
                                        {ca.basisSourceLabel}
                                    </span>
                                </div>
                                <div className="mt-0.5 flex flex-wrap gap-x-3 text-muted-foreground">
                                    <span>
                                        消费金额{" "}
                                        <MoneyValue value={entry.amount} />
                                    </span>
                                    <span>
                                        成本金额（含税）{" "}
                                        {ca.costBasis === "NONE" ||
                                        view.fieldPermissions.cost ===
                                            "masked" ? (
                                            <MoneyValue
                                                value={null}
                                                unavailableReason={
                                                    ca.noneReason ??
                                                    (view.fieldPermissions
                                                        .cost === "masked"
                                                        ? "字段打码"
                                                        : "无可用成本")
                                                }
                                            />
                                        ) : (
                                            <MoneyValue
                                                value={ca.grossAmount}
                                            />
                                        )}
                                    </span>
                                </div>
                            </li>
                        )
                    })}
                </ul>
            )}
        </section>
    )
}
