import * as React from "react"

import { MetricItem, MetricStrip, MoneyValue } from "@/components/business"
import type { CardBusinessAnalyticsView, TaxBasis } from "../types"

function taxBadge(basis: TaxBasis): string {
    return basis === "GROSS" ? "含税" : "不含税"
}

function metricValue(
    value: string | null,
    taxBasis: TaxBasis,
    valueState: string,
    reasonCode?: string,
): React.ReactNode {
    // 指标 label 已附加（含税）/（不含税），MoneyValue 不再重复渲染口径徽章
    if (valueState === "masked") {
        return <MoneyValue value={null} unavailableReason="无字段权限" />
    }
    if (valueState === "unavailable" || value == null) {
        return (
            <MoneyValue
                value={null}
                unavailableReason={reasonCode ?? "不可计算"}
            />
        )
    }
    // 比率类直接展示（label 已带口径说明）
    if (value.includes("%")) {
        return <span className="num">{value}</span>
    }
    return <MoneyValue value={value} />
}

export interface CardBusinessMetricStripProps {
    metrics: CardBusinessAnalyticsView["metrics"]
    profitReferenceOnly: boolean
}

/** 核心指标条 — 含税/不含税逐项标注。 */
export function CardBusinessMetricStrip({
    metrics,
    profitReferenceOnly,
}: CardBusinessMetricStripProps) {
    return (
        <MetricStrip
            columns={4}
            aria-label="卡券经营核心指标（销售消费余额含税 · 成本贡献不含税）"
        >
            {metrics.map((m) => (
                <MetricItem
                    key={m.key}
                    label={`${m.label}（${taxBadge(m.taxBasis)}）`}
                    value={metricValue(
                        m.value,
                        m.taxBasis,
                        m.valueState,
                        m.reasonCode,
                    )}
                    detail={m.detail}
                    status={
                        m.key === "currentContributionNet" &&
                        profitReferenceOnly
                            ? { label: "仅供参考", tone: "warning" }
                            : m.key === "consumptionMarginNet" &&
                                profitReferenceOnly
                              ? {
                                    label: "仅供参考",
                                    tone: "warning",
                                }
                              : undefined
                    }
                />
            ))}
        </MetricStrip>
    )
}
