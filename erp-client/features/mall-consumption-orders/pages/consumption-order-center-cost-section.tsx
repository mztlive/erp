"use client"

import {
    BusinessStatusBadge,
    CostCoverageNotice,
    DocumentSection,
    DocumentSummary,
    MoneyValue,
} from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type {
    CostBasis,
    MallConsumptionOrderView,
} from "@/features/mall-consumption-orders/types"
import {
    COST_BASIS_LABEL,
    COST_BASIS_TONE,
} from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"
import type { CostCoverage } from "./consumption-order-center-derivations"

export function CostSection({
    view,
    costBasisPrimary,
    costCoverage,
}: {
    view: MallConsumptionOrderView
    costBasisPrimary: CostBasis
    costCoverage: CostCoverage
}) {
    const costEntries = view.consumptionEntries
    return (
        <DocumentSection
            title="成本口径"
            description="无成本时仅显示原因，不按零成本计入利润。"
        >
            <CostCoverageNotice
                basis={costBasisPrimary}
                coveragePercent={costCoverage.percent}
                coverageLabel={
                    costCoverage.total === 0
                        ? "无消费条目"
                        : `${costCoverage.coveredCount}/${costCoverage.total} 条已覆盖`
                }
                coverageState={costCoverage.state}
                breakdown={{
                    ACTUAL: `${costEntries.filter((e) => e.currentCostAssessment.costBasis === "ACTUAL").length} 条`,
                    STANDARD: `${costEntries.filter((e) => e.currentCostAssessment.costBasis === "STANDARD").length} 条`,
                    NONE: `${costEntries.filter((e) => e.currentCostAssessment.costBasis === "NONE").length} 条`,
                }}
                profitBasis={
                    costCoverage.coveredCount === 0
                        ? "无成本数据时不计入利润；经营分析见「卡券经营分析」"
                        : "利润解读须同时阅读成本覆盖"
                }
                notice={
                    costCoverage.state === "none"
                        ? "无可用成本条目金额为空并显示无成本，不按零成本计入利润。"
                        : undefined
                }
            />

            <div className="mt-4 space-y-3">
                {view.consumptionEntries.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        尚无消费条目成本评估（待归集时常见）。支付记录与订单仍保留。
                    </p>
                ) : (
                    view.consumptionEntries.map((entry) => {
                        const ca = entry.currentCostAssessment
                        return (
                            <Card
                                key={entry.consumptionEntryId}
                                className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                            >
                                <CardHeader className="border-b border-grid pb-2">
                                    <CardTitle className="text-base">
                                        {entry.direction === "REVERSAL"
                                            ? "冲减"
                                            : "消费"}{" "}
                                        <span className="num text-sm font-normal">
                                            {entry.consumptionEntryId}
                                        </span>
                                    </CardTitle>
                                    <CardDescription>
                                        <BusinessStatusBadge
                                            context="list"
                                            label={COST_BASIS_LABEL[ca.costBasis]}
                                            tone={COST_BASIS_TONE[ca.costBasis]}
                                        />
                                        <span className="ml-2">
                                            {ca.basisSourceLabel}
                                        </span>
                                    </CardDescription>
                                </CardHeader>
                                <CardContent>
                                    <DocumentSummary
                                        columns="three"
                                        items={[
                                            {
                                                id: "f-89180",
                                                label: "消费金额",
                                                value: (
                                                    <MoneyValue
                                                        value={entry.amount}
                                                    />
                                                ),
                                            },
                                            {
                                                id: "f-25665",
                                                label: "成本金额（含税）",
                                                value:
                                                    ca.costBasis === "NONE" ||
                                                    view.fieldPermissions
                                                        .cost === "masked" ? (
                                                        <MoneyValue
                                                            value={null}
                                                            unavailableReason={
                                                                ca.noneReason ??
                                                                (view
                                                                    .fieldPermissions
                                                                    .cost ===
                                                                "masked"
                                                                    ? "字段打码"
                                                                    : "无可用成本")
                                                            }
                                                        />
                                                    ) : (
                                                        <MoneyValue
                                                            value={
                                                                ca.grossAmount
                                                            }
                                                        />
                                                    ),
                                            },
                                            {
                                                id: "f-38012",
                                                label: "评估时间",
                                                value: (
                                                    <span className="num">
                                                        {formatDateTime(
                                                            ca.assessedAt,
                                                            "default",
                                                        )}
                                                    </span>
                                                ),
                                            },
                                        ]}
                                    />
                                    {ca.costBasis === "NONE" ? (
                                        <p className="mt-2 text-sm text-warning-foreground">
                                            {ca.noneReason ??
                                                "无可用成本来源，金额为空，不计入利润"}
                                        </p>
                                    ) : null}
                                </CardContent>
                            </Card>
                        )
                    })
                )}
            </div>
        </DocumentSection>
    )
}
