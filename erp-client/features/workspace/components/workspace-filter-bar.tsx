"use client"

import { CopyNavLink } from "@/features/approval-copies/page"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    WorkspaceViewNav,
    WorkspaceViewNavItem,
} from "@/components/business/workspace-view-nav"
import type { WorkspaceMetric, WorkspaceMetricKey } from "../types"

/**
 * 队列口径切换。数量直接使用服务端指标，禁止对已加载条目求和。
 */
export function WorkspaceQueueScopeNav({
    metrics,
    activeMetric,
    onMetricClick,
}: {
    metrics: readonly WorkspaceMetric[]
    activeMetric: WorkspaceMetricKey
    onMetricClick: (key: WorkspaceMetricKey) => void
}) {
    const visibleMetrics = metrics.filter(
        (metric) =>
            metric.visible &&
            (metric.key === "inbox" ||
                metric.key === "managed" ||
                metric.key === "history" ||
                metric.key === "started"),
    )
    const scopeKey =
        activeMetric === "started" ||
        activeMetric === "managed" ||
        activeMetric === "history"
            ? activeMetric
            : "inbox"

    return (
        <WorkspaceViewNav>
            {visibleMetrics.map((metric) => (
                <WorkspaceViewNavItem
                    key={metric.key}
                    id={`workspace-queue-scope-${toAutomationIdSegment(metric.key)}`}
                    active={metric.key === scopeKey}
                    aria-label={
                        metric.key === "history"
                            ? metric.label
                            : `${metric.label} ${metric.count} 项`
                    }
                    onClick={() => onMetricClick(metric.key)}
                >
                    <span>{metric.label}</span>
                    {metric.key !== "history" && (
                        <span className="num ml-1 text-xs text-muted-foreground">
                            {metric.count}
                        </span>
                    )}
                </WorkspaceViewNavItem>
            ))}
            <CopyNavLink />
        </WorkspaceViewNav>
    )
}
