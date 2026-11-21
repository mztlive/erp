"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
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
                header: "时间",
                meta: { width: "default", numeric: true },
                cell: ({ row }) => (
                    <TableTextCell
                        value={formatDateTime(row.original.recordedAt, "full")}
                        numeric
                        clamp={1}
                    />
                ),
            },
            {
                id: "actor",
                header: "操作者",
                cell: ({ row }) => (
                    <TableTextCell value={row.original.actorLabel} clamp={1} />
                ),
            },
            {
                id: "action",
                header: "操作",
                cell: ({ row }) => (
                    <TableTextCell value={row.original.actionLabel} clamp={1} />
                ),
            },
            {
                id: "object",
                header: "操作对象",
                meta: { width: "flex" },
                cell: ({ row }) => (
                    <TableIdentityCell
                        primary={row.original.objectLabel}
                        primaryTitle={row.original.objectLabel}
                        secondary={row.original.objectTypeLabel}
                        secondaryTitle={row.original.objectTypeLabel}
                    />
                ),
            },
            {
                id: "result",
                header: "结果",
                meta: { width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.resultLabel}
                        tone={row.original.resultTone}
                        appearance={
                            row.original.resultTone === "success"
                                ? "quiet"
                                : "badge"
                        }
                    />
                ),
            },
            {
                id: "fields",
                header: "变更",
                meta: { width: "reference" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={
                            row.original.changedFieldDisplay === "—"
                                ? null
                                : row.original.changedFieldDisplay
                        }
                        clamp={1}
                    />
                ),
            },
            {
                id: "actions",
                header: "查看",
                meta: { align: "end", width: "status" },
                cell: ({ row }) => {
                    const segment = toAutomationIdSegment(
                        row.original.auditEventId,
                    )
                    return (
                        <TableRowActions
                            moreId={`operations-audit-events-row-${segment}-more`}
                            moreLabel={`${row.original.objectLabel} 更多操作`}
                            actions={[
                                {
                                    id: `operations-audit-events-row-${segment}-detail`,
                                    label: "详情",
                                    buttonRef: (element) => {
                                        rowFocusRef.current.set(
                                            row.original.auditEventId,
                                            element,
                                        )
                                    },
                                    onClick: () =>
                                        openEvent(row.original.auditEventId),
                                },
                            ]}
                        />
                    )
                },
            },
        ],
        [openEvent, rowFocusRef],
    )
}

export { useAuditColumns }
