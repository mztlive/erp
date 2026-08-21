"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import { FixedOptionRadioFilter, OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { MallSearchCombobox } from "@/features/entity-selectors"
import type {
    CostBasis,
    HistoryBackfillEnvironment,
    HistoryBackfillProcessingStatus,
    HistoryBackfillReportReviewStatus,
} from "@/features/history-backfill/types"
import {
    PROCESSING_STATUS_LABEL,
    REPORT_REVIEW_STATUS_LABEL,
} from "@/features/history-backfill/types"

type SetState<T> = React.Dispatch<React.SetStateAction<T>>

const ENVIRONMENT_FILTER_OPTIONS: ReadonlyArray<{
    value: HistoryBackfillEnvironment | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "production", label: "生产环境" },
    { value: "verification", label: "验证环境" },
]

const BASIS_FILTER_OPTIONS: ReadonlyArray<{
    value: CostBasis | "all"
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "ACTUAL", label: "实际成本" },
    { value: "STANDARD", label: "时点标准成本" },
    { value: "NONE", label: "未覆盖" },
]

const PROCESSING_STATUS_FILTER_OPTIONS: ReadonlyArray<{
    value: HistoryBackfillProcessingStatus | "all"
    label: string
}> = [
    { value: "all", label: "全部处理状态" },
    ...(
        Object.keys(PROCESSING_STATUS_LABEL) as HistoryBackfillProcessingStatus[]
    ).map((status) => ({
        value: status,
        label: PROCESSING_STATUS_LABEL[status],
    })),
]

const REPORT_REVIEW_FILTER_OPTIONS: ReadonlyArray<{
    value: HistoryBackfillReportReviewStatus | "all"
    label: string
}> = [
    { value: "all", label: "全部确认状态" },
    ...(
        Object.keys(
            REPORT_REVIEW_STATUS_LABEL,
        ) as HistoryBackfillReportReviewStatus[]
    ).map((status) => ({
        value: status,
        label: REPORT_REVIEW_STATUS_LABEL[status],
    })),
]

/**
 * 「更多筛选」折叠面板（docs/ui-filter-design.md §3.4）。
 * 只编辑 Draft，提交走外层唯一 <form> 的 applyFilters。
 */
function JobListFilterPanel({
    panelId,
    environmentDraft,
    setEnvironmentDraft,
    basisDraft,
    setBasisDraft,
    mallIdDraft,
    setMallIdDraft,
    processingStatusDraft,
    setProcessingStatusDraft,
    reportReviewStatusDraft,
    setReportReviewStatusDraft,
    resetMoreFilters,
}: {
    panelId: string
    environmentDraft: HistoryBackfillEnvironment | "all"
    setEnvironmentDraft: SetState<HistoryBackfillEnvironment | "all">
    basisDraft: CostBasis | "all"
    setBasisDraft: SetState<CostBasis | "all">
    mallIdDraft: string | null
    setMallIdDraft: SetState<string | null>
    processingStatusDraft: HistoryBackfillProcessingStatus | "all"
    setProcessingStatusDraft: SetState<HistoryBackfillProcessingStatus | "all">
    reportReviewStatusDraft: HistoryBackfillReportReviewStatus | "all"
    setReportReviewStatusDraft: SetState<
        HistoryBackfillReportReviewStatus | "all"
    >
    resetMoreFilters: () => void
}) {
    return (
        <div
            id={panelId}
            className="flex w-full flex-col gap-3 border-t pt-3"
            aria-label="历史回填任务更多筛选条件"
        >
            <FixedOptionRadioFilter
                label="环境"
                value={environmentDraft}
                onValueChange={setEnvironmentDraft}
                options={ENVIRONMENT_FILTER_OPTIONS}
            />
            <FixedOptionRadioFilter
                label="成本口径"
                value={basisDraft}
                onValueChange={setBasisDraft}
                options={BASIS_FILTER_OPTIONS}
            />
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                    <span className="text-muted-foreground">商城</span>
                    <MallSearchCombobox
                        className="w-full"
                        value={mallIdDraft}
                        onValueChange={(v) => setMallIdDraft(v ?? null)}
                        placeholder="全部商城"
                        aria-label="商城"
                    />
                </div>
                <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                    <span className="text-muted-foreground">处理状态</span>
                    <OptionCombobox
                        className="w-full"
                        value={processingStatusDraft}
                        aria-label="处理状态"
                        onValueChange={(v) =>
                            setProcessingStatusDraft(
                                (v ?? "all") as
                                    | HistoryBackfillProcessingStatus
                                    | "all",
                            )
                        }
                        options={PROCESSING_STATUS_FILTER_OPTIONS}
                        placeholder="全部处理状态"
                    />
                </div>
                <div className="flex min-w-0 flex-col gap-1.5 text-sm">
                    <span className="text-muted-foreground">报告确认</span>
                    <OptionCombobox
                        className="w-full"
                        value={reportReviewStatusDraft}
                        aria-label="报告确认"
                        onValueChange={(v) =>
                            setReportReviewStatusDraft(
                                (v ?? "all") as
                                    | HistoryBackfillReportReviewStatus
                                    | "all",
                            )
                        }
                        options={REPORT_REVIEW_FILTER_OPTIONS}
                        placeholder="全部确认状态"
                    />
                </div>
            </div>
            <div className="flex flex-col gap-3 border-t pt-3 sm:flex-row sm:items-center sm:justify-between">
                <p className="text-xs text-muted-foreground">
                    将同时应用上方关键词和以下筛选条件；结果也用于导出。
                </p>
                <div className="flex flex-wrap items-center gap-2 sm:justify-end">
                    <Button
                        type="button"
                        variant="ghost"
                        onClick={resetMoreFilters}
                    >
                        重置更多条件
                    </Button>
                    <Button type="submit">
                        <SearchIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        应用全部筛选
                    </Button>
                </div>
            </div>
        </div>
    )
}

export { JobListFilterPanel }
