import { DownloadIcon, RefreshCwIcon } from "lucide-react"

import { DataFreshness, PageActions } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceQuietAction,
} from "@/components/business/list-workspace"
import type { DataFreshnessState } from "@/components/business/page"
import { formatDateTime } from "@/lib/datetime"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"

export function ProfitLossPageHeader({
    hasData,
    projectedAt,
    freshnessUi,
    analysisReady,
    canExport,
    exportDisabled,
    exportPending = false,
    refreshing = false,
    onRefresh,
    onExport,
}: {
    hasData: boolean
    projectedAt?: string
    freshnessUi: { uiState: DataFreshnessState; statusLabel: string }
    analysisReady: boolean
    canExport: boolean
    exportDisabled: boolean
    exportPending?: boolean
    refreshing?: boolean
    onRefresh: () => void
    onExport: () => void
}) {
    return (
        <ListWorkspaceHeader
            title="实际经营盈亏"
            description={`数据范围：${SCOPE_LABEL}`}
        >
            <div className="flex min-w-0 flex-wrap items-center justify-end gap-x-3 gap-y-1">
                {hasData ? (
                    <DataFreshness
                        updatedAt={formatDateTime(projectedAt ?? "", "full")}
                        dateTime={projectedAt}
                        state={freshnessUi.uiState}
                        statusLabel={freshnessUi.statusLabel}
                        label="经营汇总"
                    />
                ) : (
                    <DataFreshness
                        updatedAt="—"
                        state="unknown"
                        label="经营汇总"
                        statusLabel="待选择口径"
                    />
                )}
                <PageActions
                    actions={[
                        listWorkspaceQuietAction({
                            actionKey: "refresh",
                            id: "actual-profit-loss-header-refresh",
                            loading: refreshing,
                            label: "刷新",
                            icon: RefreshCwIcon,
                            disabled: !analysisReady,
                            onClick: () => {
                                onRefresh()
                            },
                        }),
                        listWorkspaceQuietAction({
                            hidden: !canExport,
                            actionKey: "export",
                            id: "actual-profit-loss-header-export",
                            loading: exportPending,
                            label: exportPending ? "导出中…" : "导出",
                            icon: DownloadIcon,
                            title: "导出全部匹配行，采用生成时的最新数据",
                            disabled: exportDisabled,
                            onClick: () => {
                                onExport()
                            },
                        }),
                    ]}
                />
            </div>
        </ListWorkspaceHeader>
    )
}
