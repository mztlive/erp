"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
    return (
        <>
            {/* Distinct freshness / coverage alerts — not mutually substitutable */}
            {refreshError ? (
                <Alert variant="destructive">
                    <AlertTitle>刷新失败</AlertTitle>
                    <AlertDescription>{refreshError}</AlertDescription>
                </Alert>
            ) : null}
            {freshness.state === "stale" && !freshness.refreshFailed ? (
                <Alert variant="warning">
                    <AlertTitle>数据可能不是最新</AlertTitle>
                    <AlertDescription>
                        最近成功更新{" "}
                        {formatDateTime(
                            freshness.projectedAt,
                            "full",
                            "passthrough",
                        )}
                        ；来源更新时间{" "}
                        <span className="num">
                            {formatSourceWatermark(freshness.sourceWatermark)}
                        </span>
                        。数据可能不是最新，可点击刷新。
                    </AlertDescription>
                </Alert>
            ) : null}
            {freshness.state === "rebuilding" ? (
                <Alert variant="info">
                    <AlertTitle>数据更新中</AlertTitle>
                    <AlertDescription>
                        更新中，已保留最近成功结果。
                    </AlertDescription>
                </Alert>
            ) : null}
            {freshness.refreshFailed ? (
                <Alert variant="destructive">
                    <AlertTitle>刷新失败</AlertTitle>
                    <AlertDescription>
                        已保留旧结果。请重试；业务记录未被修改。
                    </AlertDescription>
                </Alert>
            ) : null}
            {freshness.state === "failed" ? (
                <Alert variant="destructive">
                    <AlertTitle>数据加载失败</AlertTitle>
                    <AlertDescription>
                        显示上次成功数据（若有）。请查看后台任务或稍后重试。
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )
}
