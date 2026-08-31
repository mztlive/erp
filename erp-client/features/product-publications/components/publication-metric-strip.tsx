"use client"

import { MetricFilterItem, MetricStrip } from "@/components/business"

import type { ProductPublicationListResult } from "@/features/product-publications/types"

/**
 * 指标与「发布状态/发送状态」双向互斥：指标点击清除状态维度、状态变更清除指标。
 * 这是有意设计（避免指标×状态矛盾空结果），与通用「指标点击不清其它筛选」不同；保留并注明。
 */
export function PublicationMetricStrip({
    metrics,
    metric,
    onToggle,
}: {
    metrics: ProductPublicationListResult["metrics"] | undefined
    metric: string
    /** 传入目标指标键；父级负责互斥切换（再次点击清除） */
    onToggle: (metricKey: string) => void
}) {
    return (
        <MetricStrip>
            <MetricFilterItem
                id="publication-metrics-pending-publish"
                label="待发布"
                value={metrics?.pendingPublish ?? "—"}
                active={metric === "pending_publish"}
                onClick={() => onToggle("pending_publish")}
            />
            <MetricFilterItem
                id="publication-metrics-pending-confirm"
                label="待商城确认"
                value={metrics?.pendingConfirm ?? "—"}
                active={metric === "pending_confirm"}
                onClick={() => onToggle("pending_confirm")}
            />
            <MetricFilterItem
                id="publication-metrics-failed-handoff"
                label="失败/转人工"
                value={metrics?.failedOrHandoff ?? "—"}
                active={metric === "failed_handoff"}
                onClick={() => onToggle("failed_handoff")}
            />
            <MetricFilterItem
                id="publication-metrics-mall-live"
                label="商城已生效"
                value={metrics?.mallLive ?? "—"}
                active={metric === "mall_live"}
                onClick={() => onToggle("mall_live")}
            />
            <MetricFilterItem
                id="publication-metrics-paused"
                label="已暂停"
                value={metrics?.paused ?? "—"}
                active={metric === "paused"}
                onClick={() => onToggle("paused")}
            />
        </MetricStrip>
    )
}
