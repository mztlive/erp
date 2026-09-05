import { DownloadIcon, LoaderCircleIcon, RefreshCwIcon } from "lucide-react"

import { DataFreshness, PageActions } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import type { DataFreshnessState } from "@/components/business/page"
import { formatDateTime } from "@/lib/datetime"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"

export function ProfitLossPageHeader({
    hasData,
    projectedAt,
    freshnessUi,
    analysisReady,
    exportDisabled,
    exportPending = false,
    onRefresh,
    onExport,
}: {
    hasData: boolean
    projectedAt?: string
    freshnessUi: { uiState: DataFreshnessState; statusLabel: string }
    analysisReady: boolean
    exportDisabled: boolean
    exportPending?: boolean
    onRefresh: () => void
    onExport: () => void
}) {
    return (
        <ListWorkspaceHeader
            eyebrow="分析"
            title="实际经营盈亏"
            description={`数据范围：${SCOPE_LABEL}`}
        >
            <div className="flex flex-wrap items-center justify-end gap-3">
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
                        {
                            actionKey: "refresh",
                            id: "actual-profit-loss-header-refresh",
                            label: "刷新",
                            icon: RefreshCwIcon,
                            variant: "ghost",
                            className:
                                "text-muted-foreground hover:text-foreground",
                            disabled: !analysisReady,
                            onClick: () => {
                                onRefresh()
                            },
                        },
                        {
                            actionKey: "export",
                            id: "actual-profit-loss-header-export",
                            label: exportPending ? "导出中…" : "导出",
                            icon: exportPending
                                ? LoaderCircleIcon
                                : DownloadIcon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled: exportDisabled,
                            onClick: () => {
                                onExport()
                            },
                        },
                    ]}
                />
            </div>
        </ListWorkspaceHeader>
    )
}
