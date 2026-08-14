"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

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
                header: "时间",
                cell: ({ row }) => (
                    <span className="num text-xs">
                        {formatDateTime(row.original.recordedAt, "full")}
                    </span>
                ),
            },
            {
                id: "actor",
                header: "操作者",
                cell: ({ row }) => (
                    <div className="min-w-[7rem]">
                        <div className="font-medium">
                            {row.original.actorLabel}
                        </div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.actorId}
                        </div>
                    </div>
                ),
            },
            {
                id: "role",
                header: "责任角色",
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {row.original.actorRole}
                    </span>
                ),
            },
            {
                id: "action",
                header: "动作",
                cell: ({ row }) => row.original.actionLabel,
            },
            {
                id: "object",
                header: "对象",
                cell: ({ row }) => <span>{row.original.objectLabel}</span>,
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
                header: "变更字段",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.changedFieldDisplay !== "—"
                            ? row.original.changedFieldDisplay
                            : "—"}
                    </span>
                ),
            },
            {
                id: "trace",
                header: "请求追踪号",
                cell: ({ row }) => (
                    <span className="font-mono text-xs">
                        {row.original.traceId}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "查看",
                cell: ({ row }) => (
                    <div className="flex justify-end">
                        <Button
                            type="button"
                            size="xs"
                            variant="outline"
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
