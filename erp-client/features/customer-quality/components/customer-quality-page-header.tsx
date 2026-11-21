"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { DownloadIcon, RefreshCwIcon } from "lucide-react"

import { DataFreshness, GuardedBusinessAction } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
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
            className="gap-2 py-2 md:min-h-12 md:py-1 [&>div:last-child]:min-w-0 [&>div:last-child]:shrink"
            title="客户经营质量"
        >
            <div className="flex min-w-0 flex-wrap items-center justify-end gap-x-3 gap-y-1">
                <span className="text-xs text-muted-foreground">
                    期间{" "}
                    <span className="num">
                        {period.from} ~ {period.to}
                    </span>{" "}
                    · {scopeLabel}
                </span>
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
                        className={styles.quietButton}
                        disabled={refreshing}
                        onClick={() => {
                            void onRefresh()
                        }}
                    >
                        {!refreshing ? (
                            <RefreshCwIcon
                                data-icon="inline-start"
                                aria-hidden
                            />
                        ) : null}
                        {refreshing ? "刷新中" : "刷新"}
                    </LoadingButton>
                    <PermissionGuard
                        permission={["customer:list", "sales_order:list"]}
                    >
                        {canExport && (
                            <GuardedBusinessAction
                                id="customers-quality-export"
                                loading={exportPending}
                                type="button"
                                variant="ghost"
                                className={styles.quietButton}
                                size="sm"
                                disabled={filteredTotal === 0 || exportPending}
                                reason={
                                    filteredTotal === 0
                                        ? "当前没有客户结果可导出"
                                        : exportPending
                                          ? "导出任务进行中"
                                          : undefined
                                }
                                onClick={() => void onExport()}
                            >
                                {!exportPending ? (
                                    <DownloadIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                ) : null}
                                {exportPending ? "导出中…" : "导出"}
                            </GuardedBusinessAction>
                        )}
                    </PermissionGuard>
                </div>
            </div>
        </ListWorkspaceHeader>
    )
}
