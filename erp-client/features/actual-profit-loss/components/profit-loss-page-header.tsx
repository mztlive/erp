import { DownloadIcon, RefreshCwIcon } from "lucide-react"

import {
    DataFreshness,
    PageActions,
    PageHeader,
} from "@/components/business"
import type { DataFreshnessState } from "@/components/business/page"
import { formatDateTime } from "@/lib/datetime"
import { PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL } from "@/features/actual-profit-loss/lib/presentation"

export function ProfitLossPageHeader({
    hasData,
    projectedAt,
    freshnessUi,
    analysisReady,
    exportDisabled,
    onRefresh,
    onExport,
}: {
    hasData: boolean
    projectedAt?: string
    freshnessUi: { uiState: DataFreshnessState; statusLabel: string }
    analysisReady: boolean
    exportDisabled: boolean
    onRefresh: () => void
    onExport: () => void
}) {
    return (
        <PageHeader
            title={`实际经营盈亏（${SCOPE_LABEL}）`}
            breadcrumbs={[
                { id: "an", label: "分析", href: "/analytics/profit-loss" },
                { id: "pl", label: "实际经营盈亏", current: true },
            ]}
            metadata={
                hasData ? (
                    <div className="flex flex-col gap-1">
                        <DataFreshness
                            updatedAt={formatDateTime(projectedAt ?? "", "full")}
                            dateTime={projectedAt}
                            state={freshnessUi.uiState}
                            statusLabel={freshnessUi.statusLabel}
                            label="经营汇总"
                        />
                    </div>
                ) : (
                    <DataFreshness
                        updatedAt="—"
                        state="unknown"
                        label="经营汇总"
                        statusLabel="待选择口径"
                    />
                )
            }
            actions={
                <PageActions
                    actions={[
                        {
                            actionKey: "refresh",
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
                            label: "导出",
                            icon: DownloadIcon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled: exportDisabled,
                            onClick: () => {
                                onExport()
                            },
                        },
                    ]}
                />
            }
        />
    )
}
