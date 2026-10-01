"use client"

import { DownloadIcon, RefreshCwIcon } from "lucide-react"

import { DataFreshness, GuardedBusinessAction } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { LoadingButton } from "@/components/ui/loading-button"
import { formatClock, freshnessPresentation } from "../lib/presentation"
import type { CustomerQualityView } from "../types"

export function CustomerQualityPageHeader({
    freshness,
    refreshing,
    period,
    scopeLabel,
    onRefresh,
    canExport,
    filteredTotal,
    exportPending,
    onExport,
}: {
    freshness: CustomerQualityView["freshness"]
    refreshing: boolean
    period: CustomerQualityView["period"]
    scopeLabel: string
    onRefresh: () => void
    canExport: boolean
    filteredTotal: number
    exportPending: boolean
    onExport: () => void
}) {
    const freshUi = freshnessPresentation(
        freshness.state,
        freshness.refreshFailed,
        refreshing,
    )

    return (
        <ListWorkspaceHeader
            eyebrow="分析"
            title="客户经营质量"
            description={`期间 ${period.from} ~ ${period.to}${
                period.selectionSource === "SERVER_DEFAULT"
                    ? " · 系统默认"
                    : period.selectionSource === "CONFIGURED_PRESET"
                      ? " · 配置快捷项"
                      : " · 显式选择"
            } · ${scopeLabel}`}
        >
            <div className="flex flex-wrap items-center justify-end gap-3">
                <DataFreshness
                    updatedAt={formatClock(freshness.projectedAt)}
                    dateTime={freshness.projectedAt}
                    state={freshUi.state}
                    statusLabel={freshUi.statusLabel}
                    label="经营质量汇总"
                />
                <div className="flex flex-wrap items-center gap-2">
                    <LoadingButton
                        id="customers-quality-refresh"
                        type="button"
                        loading={refreshing}
                        variant="ghost"
                        size="sm"
                        className="text-muted-foreground hover:text-foreground"
                        disabled={refreshing}
                        onClick={() => {
                            void onRefresh()
                        }}
                    >
                        {!refreshing ? (
                            <RefreshCwIcon className="size-4" aria-hidden />
                        ) : null}
                        {refreshing ? "刷新中" : "刷新"}
                    </LoadingButton>
                    <GuardedBusinessAction
                        id="customers-quality-export"
                        loading={exportPending}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={
                            !canExport || filteredTotal === 0 || exportPending
                        }
                        reason={
                            !canExport
                                ? "当前角色无导出权限"
                                : filteredTotal === 0
                                  ? "当前没有客户结果可导出"
                                  : exportPending
                                    ? "导出任务进行中"
                                    : undefined
                        }
                        onClick={() => void onExport()}
                    >
                        {!exportPending ? (
                            <DownloadIcon className="size-4" aria-hidden />
                        ) : null}
                        {exportPending ? "导出中…" : "导出"}
                    </GuardedBusinessAction>
                </div>
            </div>
        </ListWorkspaceHeader>
    )
}
