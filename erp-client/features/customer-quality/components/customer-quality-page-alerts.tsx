"use client"

import { SystemFeedback } from "@/components/business/system-feedback"
import { formatDateTime } from "@/lib/datetime"
import { formatSourceWatermark } from "../lib/presentation"
import type { CustomerQualityView } from "../types"

export function CustomerQualityPageAlerts({
    refreshError,
    freshness,
}: {
    refreshError: string | null
    freshness: CustomerQualityView["freshness"]
}) {
    const backgroundFailed =
        freshness.refreshFailed || freshness.state === "failed"
    return (
        <>
            <SystemFeedback
                problemId="customer-quality-background-refresh"
                error={backgroundFailed ? "客户质量数据更新失败。" : null}
                title="客户质量数据尚未更新"
                description="已保留上次成功结果，业务记录未被修改。请稍后刷新或联系管理员。"
            />
            {freshness.state === "stale" || refreshError || backgroundFailed ? (
                <p role="status" className="text-xs text-muted-foreground">
                    当前显示上次成功结果，更新于{" "}
                    {formatDateTime(
                        freshness.projectedAt,
                        "full",
                        "passthrough",
                    )}
                    ； 来源更新时间{" "}
                    <span className="num">
                        {formatSourceWatermark(freshness.sourceWatermark)}
                    </span>
                    。
                </p>
            ) : null}
            {freshness.state === "rebuilding" ? (
                <p role="status" className="text-xs text-muted-foreground">
                    数据更新中，已保留最近成功结果。
                </p>
            ) : null}
        </>
    )
}
