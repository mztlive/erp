"use client"

import { DecisionKpiBar, DecisionKpiCell } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { metricReliabilityDetail } from "../lib/presentation"
import type { CustomerQualityView } from "../types"

export function CustomerQualityMetricStrip({
    metrics,
}: {
    metrics: CustomerQualityView["metrics"]
}) {
    return (
        <DecisionKpiBar columns={4} ariaLabel="客户经营质量核心指标">
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
                                {m.fieldDenied ? "当前角色不可查看" : "待核算"}
                            </span>
                        ) : (
                            m.value
                        )
                    const reliabilityBadge =
                        m.reliability === "partial" ? (
                            <StatusBadge tone="warning" label="部分可靠" />
                        ) : m.reliability === "unavailable" ? (
                            <StatusBadge tone="neutral" label="不可用" />
                        ) : undefined
                    return (
                        <DecisionKpiCell
                            key={m.key}
                            id={`customers-quality-metric-${toAutomationIdSegment(m.key)}`}
                            label={m.label}
                            value={valueNode}
                            valueMeta={reliabilityBadge}
                            footnote={detail}
                        />
                    )
                })}
        </DecisionKpiBar>
    )
}
