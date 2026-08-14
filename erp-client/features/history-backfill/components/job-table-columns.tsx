"use client"

import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { HistoryBackfillListItem } from "@/features/history-backfill/types"
import {
    ENVIRONMENT_LABEL,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
} from "@/features/history-backfill/types"

export function buildJobListColumns(
    onOpenJob: (id: string) => void,
): ColumnDef<HistoryBackfillListItem>[] {
    return [
        {
            id: "jobNo",
            header: "任务号",
            cell: ({ row }) => (
                <Button
                    variant="link"
                    className="h-auto p-0 font-mono text-sm"
                    onClick={() => onOpenJob(row.original.id)}
                >
                    {row.original.jobNo}
                </Button>
            ),
        },
        {
            id: "mall",
            header: "商城",
            cell: ({ row }) => (
                <div className="space-y-0.5">
                    <div className="text-sm">{row.original.mallName}</div>
                    <Badge
                        variant={
                            row.original.environment === "production"
                                ? "destructive"
                                : "secondary"
                        }
                        className="text-2xs"
                    >
                        {ENVIRONMENT_LABEL[row.original.environment]}
                    </Badge>
                </div>
            ),
        },
        {
            id: "range",
            header: "范围起点至截止时点",
            cell: ({ row }) => (
                <span className="num font-mono text-xs">
                    {row.original.rangeLabel}
                </span>
            ),
        },
        {
            id: "processing",
            header: "处理状态",
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={
                        PROCESSING_STATUS_LABEL[row.original.processingStatus]
                    }
                    tone={PROCESSING_STATUS_TONE[row.original.processingStatus]}
                />
            ),
        },
        {
            id: "reportReview",
            header: "报告确认",
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={
                        REPORT_REVIEW_STATUS_LABEL[
                            row.original.reportReviewStatus
                        ]
                    }
                    tone={
                        REPORT_REVIEW_STATUS_TONE[
                            row.original.reportReviewStatus
                        ]
                    }
                />
            ),
        },
        {
            id: "progress",
            header: "进度",
            cell: ({ row }) => (
                <span className="num text-sm">{row.original.progressLabel}</span>
            ),
        },
        {
            id: "dedupe",
            header: "去重",
            cell: ({ row }) => (
                <span className="num text-sm">
                    {row.original.deduplicatedCount.toLocaleString("zh-CN")}
                </span>
            ),
        },
        {
            id: "unattr",
            header: "未归集",
            cell: ({ row }) => (
                <span className="num text-sm">
                    {row.original.unattributedCount.toLocaleString("zh-CN")}
                </span>
            ),
        },
        {
            id: "cost",
            header: "成本覆盖",
            cell: ({ row }) => (
                <span className="text-xs">{row.original.costCoverageLabel}</span>
            ),
        },
        {
            id: "actions",
            header: "操作",
            cell: ({ row }) => (
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => onOpenJob(row.original.id)}
                >
                    打开
                </Button>
            ),
        },
    ]
}
