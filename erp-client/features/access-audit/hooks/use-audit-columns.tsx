"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { toAutomationIdSegment } from "@/lib/automation-id"
import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { AuditEventRow } from "@/features/access-audit/types"
import { formatDateTime } from "@/lib/datetime"

type UseAuditColumnsInput = {
    rowFocusRef: { current: Map<string, HTMLButtonElement | null> }
    openEvent: (id: string) => void
}

function useAuditColumns({ rowFocusRef, openEvent }: UseAuditColumnsInput) {
    return React.useMemo<ColumnDef<AuditEventRow>[]>(
        () => [
            {
                id: "time",
                size: 175,
                header: "时间",
                cell: ({ row }) => (
                    <span
                        className="num whitespace-nowrap text-xs"
                        title={formatDateTime(row.original.recordedAt, "full")}
                    >
                        {formatDateTime(row.original.recordedAt, "full")}
                    </span>
                ),
            },
            {
                id: "actor",
                header: "操作者",
                cell: ({ row }) => (
                    <div className="min-w-[7rem]">
                        <div className="text-sm font-medium">
                            {row.original.actorLabel}
                        </div>
                    </div>
                ),
            },
            {
                id: "action",
                header: "操作",
                cell: ({ row }) => (
                    <span className="text-sm">{row.original.actionLabel}</span>
                ),
            },
            {
                id: "object",
                size: 240,
                header: "操作对象",
                cell: ({ row }) => (
                    <div className="min-w-[8rem]">
                        <div className="truncate text-sm">
                            {row.original.objectLabel}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.objectTypeLabel}
                        </div>
                    </div>
                ),
            },
            {
                id: "result",
                header: "结果",
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        label={row.original.resultLabel}
                        tone={row.original.resultTone}
                    />
                ),
            },
            {
                id: "fields",
                header: "变更",
                cell: ({ row }) => (
                    <span
                        className="block max-w-[12rem] truncate text-sm"
                        title={
                            row.original.changedFieldDisplay !== "—"
                                ? row.original.changedFieldDisplay
                                : undefined
                        }
                    >
                        {row.original.changedFieldDisplay !== "—"
                            ? row.original.changedFieldDisplay
                            : "—"}
                    </span>
                ),
            },
            {
                id: "actions",
                size: 85,
                header: () => <span className="block text-right">查看</span>,
                cell: ({ row }) => (
                    <div className="flex justify-end">
                        <Button
                            id={`operations-audit-events-row-${toAutomationIdSegment(row.original.auditEventId)}-detail`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            ref={(el) => {
                                rowFocusRef.current.set(
                                    row.original.auditEventId,
                                    el,
                                )
                            }}
                            onClick={() => openEvent(row.original.auditEventId)}
                        >
                            详情
                        </Button>
                    </div>
                ),
            },
        ],
        [openEvent, rowFocusRef],
    )
}

export { useAuditColumns }
