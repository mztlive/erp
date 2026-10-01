"use client"

import { EyeIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"
import { TableRowActions } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import type { BackgroundJobView } from "../api"
import {
    backgroundJobDomainLabel,
    formatJobDateTime,
    JOB_STATUS_LABELS,
} from "../labels"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"

/** 后台任务列展示；预览状态和焦点仍由工作台统一维护。 */
export function backgroundJobColumns(
    openPreview: (job: BackgroundJobView) => void,
): ColumnDef<BackgroundJobView>[] {
    const jobLabel = (job: BackgroundJobView) =>
        backgroundJobDomainLabel(job.domain_job_type, job.job_type)
    return [
        {
            id: "job",
            header: "任务",
            meta: { label: "任务" },
            cell: ({ row }) => (
                <div className="min-w-0">
                    <p className="truncate font-medium">
                        {jobLabel(row.original)}
                    </p>
                    <p className="num mt-0.5 truncate text-xs text-muted-foreground">
                        {row.original.job_no}
                    </p>
                </div>
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态" },
            cell: ({ row }) => (
                <StatusBadge
                    tone={
                        row.original.status === "succeeded"
                            ? "success"
                            : row.original.status === "failed"
                              ? "destructive"
                              : row.original.status === "cancelled"
                                ? "neutral"
                                : "info"
                    }
                    label={JOB_STATUS_LABELS[row.original.status]}
                />
            ),
        },
        {
            id: "progress",
            header: "进度",
            meta: { label: "进度", numeric: true },
            cell: ({ row }) => (
                <span className="num text-xs text-muted-foreground">
                    {row.original.processed_count} / {row.original.total_count}
                </span>
            ),
        },
        {
            id: "created",
            header: "创建时间",
            meta: { label: "创建时间", numeric: true },
            cell: ({ row }) => (
                <span className="num text-xs text-muted-foreground">
                    {formatJobDateTime(row.original.created_at)}
                </span>
            ),
        },
        {
            id: "actions",
            size: 104,
            minSize: 104,
            header: "操作",
            meta: { label: "操作", align: "end" },
            enableSorting: false,
            cell: ({ row }) => (
                <TableRowActions
                    moreId={`${ID_PREFIX}-row-${row.original.id}-more`}
                    moreLabel={`${jobLabel(row.original)} 更多操作`}
                    actions={[
                        {
                            id: `${ID_PREFIX}-row-${row.original.id}-preview`,
                            label: "查看",
                            icon: EyeIcon,
                            onClick: () => {
                                openPreview(row.original)
                            },
                        },
                    ]}
                />
            ),
        },
    ]
}
