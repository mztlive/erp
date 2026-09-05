"use client"

import { MetricItem, MetricStrip } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { metricReliabilityDetail } from "../lib/presentation"
import type { CustomerQualityView } from "../types"

export function CustomerQualityMetricStrip({
    metrics,
}: {
    metrics: CustomerQualityView["metrics"]
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
                    return (
                        <MetricItem
                            key={m.key}
                            id={`customers-quality-metric-${toAutomationIdSegment(m.key)}`}
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
