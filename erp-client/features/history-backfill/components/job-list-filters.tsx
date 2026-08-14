"use client"

import { OptionCombobox } from "@/components/business"
import { MallSearchCombobox } from "@/features/entity-selectors"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import type {
    CostBasis,
    HistoryBackfillEnvironment,
    HistoryBackfillProcessingStatus,
    HistoryBackfillReportReviewStatus,
} from "@/features/history-backfill/types"
import {
    COST_BASIS_LABEL,
    PROCESSING_STATUS_LABEL,
    REPORT_REVIEW_STATUS_LABEL,
} from "@/features/history-backfill/types"

function JobListPrimaryFilters({
    urlState,
    patchUrl,
}: {
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
}) {
    return (
        <>
            <MallSearchCombobox
                value={urlState.mallId ?? null}
                onValueChange={(v) => {
                    patchUrl({
                        mallId: v ?? undefined,
                        page: 1,
                    })
                }}
                inputClassName="w-[10rem]"
                size="sm"
                placeholder="商城：全部"
                aria-label="商城"
                allowClear={false}
            />
            <OptionCombobox
                value={urlState.processingStatus ?? "all"}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({
                        processingStatus:
                            v === "all"
                                ? undefined
                                : (v as HistoryBackfillProcessingStatus),
                        page: 1,
                    })
                }}
                options={[
                    { value: "all", label: "全部处理状态" },
                    ...(
                        Object.keys(
                            PROCESSING_STATUS_LABEL,
                        ) as HistoryBackfillProcessingStatus[]
                    ).map((s) => ({
                        value: s,
                        label: PROCESSING_STATUS_LABEL[s],
                    })),
                ]}
                inputClassName="w-[11rem]"
                size="sm"
                placeholder="处理状态：全部"
                aria-label="处理状态"
                allowClear={false}
            />
            <OptionCombobox
                value={urlState.environment ?? "all"}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({
                        environment:
                            v === "all"
                                ? undefined
                                : (v as HistoryBackfillEnvironment),
                        page: 1,
                    })
                }}
                options={[
                    { value: "all", label: "全部环境" },
                    {
                        value: "production",
                        label: "生产环境",
                    },
                    {
                        value: "verification",
                        label: "验证环境",
                    },
                ]}
                inputClassName="w-[9rem]"
                size="sm"
                placeholder="环境：全部"
                aria-label="环境"
                allowClear={false}
            />
        </>
    )
}

function JobListSecondaryFilters({
    urlState,
    patchUrl,
}: {
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
}) {
    return (
        <>
            <OptionCombobox
                value={urlState.reportReviewStatus ?? "all"}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({
                        reportReviewStatus:
                            v === "all"
                                ? undefined
                                : (v as HistoryBackfillReportReviewStatus),
                        page: 1,
                    })
                }}
                options={[
                    { value: "all", label: "全部确认状态" },
                    ...(
                        Object.keys(
                            REPORT_REVIEW_STATUS_LABEL,
                        ) as HistoryBackfillReportReviewStatus[]
                    ).map((s) => ({
                        value: s,
                        label: REPORT_REVIEW_STATUS_LABEL[s],
                    })),
                ]}
                inputClassName="w-[11rem]"
                size="sm"
                placeholder="报告确认：全部"
                aria-label="报告确认"
                allowClear={false}
            />
            <OptionCombobox
                value={urlState.basis ?? "all"}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({
                        basis:
                            v === "all" ? undefined : (v as CostBasis),
                        page: 1,
                    })
                }}
                options={[
                    { value: "all", label: "全部口径" },
                    ...(
                        Object.keys(COST_BASIS_LABEL) as CostBasis[]
                    ).map((b) => ({
                        value: b,
                        label: COST_BASIS_LABEL[b],
                    })),
                ]}
                inputClassName="w-[10rem]"
                size="sm"
                placeholder="成本口径：全部"
                aria-label="成本口径"
                allowClear={false}
            />
        </>
    )
}

export { JobListPrimaryFilters, JobListSecondaryFilters }
