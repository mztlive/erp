"use client"

import {
    MetricFilterItem,
    MetricItem,
    MetricStrip,
} from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { metricReliabilityDetail } from "../lib/presentation"
import type { CustomerQualityView } from "../types"

export function CustomerQualityMetricStrip({
    metrics,
    onFocusTable,
}: {
    metrics: CustomerQualityView["metrics"]
    onFocusTable: () => void
}) {
    return (
        <MetricStrip columns={4} aria-label="客户经营质量核心指标">
            {metrics
                .filter((m) => m.visible)
                .map((m) => {
                    const detail = metricReliabilityDetail(
                        m.reliability,
                        m.explanation,
                        m.fieldDenied,
                    )
                    const valueNode =
                        m.fieldDenied || m.reliability === "unavailable" ? (
                            <span className="text-muted-foreground">
                                {m.fieldDenied
                                    ? "当前角色不可查看"
                                    : "暂无可靠口径"}
                            </span>
                        ) : (
                            m.value
                        )
                    if (
                        m.key === "overdueGross" ||
                        m.key === "actualProfitLossNet" ||
                        m.key === "salesGrossAmount"
                    ) {
                        // D3：focusMetric 不再作伪筛选写 URL；保留「滚动定位到明细表」交互，
                        // 用普通按钮语义（不带 active 筛选态），消除「点了只滚动不筛数」的指标
                        return (
                            <MetricFilterItem
                                id={`customers-quality-metric-${toAutomationIdSegment(m.key)}-focus`}
                                key={m.key}
                                label={m.label}
                                value={valueNode}
                                detail={detail}
                                onClick={() => onFocusTable()}
                            />
                        )
                    }
                    return (
                        <MetricItem
                            key={m.key}
                            label={m.label}
                            value={valueNode}
                            detail={detail}
                            status={
                                m.reliability === "partial"
                                    ? {
                                          label: "部分可靠",
                                          tone: "warning",
                                      }
                                    : m.reliability === "unavailable"
                                      ? {
                                            label: "不可用",
                                            tone: "neutral",
                                        }
                                      : undefined
                            }
                        />
                    )
                })}
        </MetricStrip>
    )
}
