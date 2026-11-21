"use client"

import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import type { StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { BackgroundJobView, JobStatus } from "../api"
import {
    backgroundJobIdentity,
    formatJobDateTime,
    JOB_STATUS_LABELS,
} from "../labels"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"

const JOB_STATUS_TONES: Record<JobStatus, StatusTone> = {
    pending: "info",
    running: "info",
    partially_succeeded: "warning",
    succeeded: "success",
    failed: "destructive",
    cancelled: "neutral",
}

export function backgroundJobColumns(
    openPreview?: (job: BackgroundJobView) => void,
): ColumnDef<BackgroundJobView>[] {
    return [
        {
            id: "job",
            header: "任务",
            meta: { label: "任务", width: "flex" },
            cell: ({ row }) => {
                const { title, number } = backgroundJobIdentity(row.original)
                return (
                    <TableIdentityCell
                        primary={title}
                        primaryTitle={title}
                        secondary={number}
                        secondaryTitle={number}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    tone={JOB_STATUS_TONES[row.original.status]}
                    appearance={
                        row.original.status === "succeeded" ? "quiet" : "badge"
                    }
                    label={JOB_STATUS_LABELS[row.original.status]}
                />
            ),
        },
        {
            id: "progress",
            header: "进度",
            meta: {
                label: "进度",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <TableTextCell
                    value={`${row.original.processed_count} / ${row.original.total_count}`}
                    numeric
                />
            ),
        },
        {
            id: "created",
            header: "创建时间",
            meta: { label: "创建时间", width: "default", numeric: true },
            cell: ({ row }) => {
                const dateTime = formatJobDateTime(row.original.created_at)
                return (
                    <TableTextCell
                        value={dateTime === "—" ? null : dateTime}
                        clamp={1}
                        numeric
                    />
                )
            },
        },
        ...((openPreview
            ? [
                  {
                      id: "actions",
                      header: "操作",
                      meta: { label: "操作", width: "status", align: "end" },
                      enableSorting: false,
                      cell: ({ row }) => {
                          const rowId = `${ID_PREFIX}-row-${toAutomationIdSegment(row.original.id)}`
                          return (
                              <TableRowActions
                                  moreId={`${rowId}-more`}
                                  moreLabel={`${backgroundJobIdentity(row.original).title} 更多操作`}
                                  actions={[
                                      {
                                          id: `${rowId}-preview`,
                                          label: "查看",
                                          onClick: () =>
                                              openPreview(row.original),
                                      },
                                  ]}
                              />
                          )
                      },
                  },
              ]
            : []) satisfies ColumnDef<BackgroundJobView>[]),
    ]
}
