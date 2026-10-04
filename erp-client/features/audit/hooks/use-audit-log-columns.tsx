"use client"

import { useMemo } from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { EyeIcon } from "lucide-react"
import { BusinessStatusBadge, TableRowActions } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    auditActionLabel,
    auditActorLabel,
    auditChangesLabel,
    auditObjectLabel,
    auditResultView,
    auditTimeLabel,
} from "../lib/display"
import type { AuditLogItem } from "../types"

export function useAuditLogColumns(openDetails: (row: AuditLogItem) => void) {
    return useMemo<ColumnDef<AuditLogItem>[]>(
        () => [
            {
                id: "time",
                header: "时间",
                size: 175,
                cell: ({ row }) => (
                    <span className="num whitespace-nowrap text-xs">
                        {auditTimeLabel(row.original)}
                    </span>
                ),
            },
            {
                id: "actor",
                header: "操作人",
                cell: ({ row }) => (
                    <div className="min-w-[7rem]">
                        <div className="text-sm font-medium">
                            {auditActorLabel(row.original)}
                        </div>
                        <div className="text-xs text-muted-foreground">
                            {row.original.structured_event?.actor_account ??
                                row.original.actor_account}
                        </div>
                    </div>
                ),
            },
            {
                id: "action",
                header: "业务动作",
                cell: ({ row }) => (
                    <span className="text-sm">
                        {auditActionLabel(row.original)}
                    </span>
                ),
            },
            {
                id: "object",
                header: "业务编号",
                size: 220,
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {auditObjectLabel(row.original)}
                    </span>
                ),
            },
            {
                id: "result",
                header: "执行结果",
                cell: ({ row }) => {
                    const result = auditResultView(row.original)
                    return (
                        <BusinessStatusBadge
                            label={result.label}
                            tone={result.tone}
                        />
                    )
                },
            },
            {
                id: "changes",
                header: "字段变化",
                size: 300,
                cell: ({ row }) => {
                    const changes = auditChangesLabel(row.original)
                    return (
                        <span
                            className="block max-w-[22rem] truncate text-sm"
                            title={changes}
                        >
                            {changes}
                        </span>
                    )
                },
            },
            {
                id: "actions",
                header: "查看",
                size: 104,
                minSize: 104,
                meta: { align: "end" },
                cell: ({ row }) => {
                    const segment = toAutomationIdSegment(row.original.id)
                    return (
                        <TableRowActions
                            moreId={`business-audit-row-${segment}-more`}
                            moreLabel={`${auditObjectLabel(row.original)}更多操作`}
                            actions={[
                                {
                                    id: `business-audit-row-${segment}-details`,
                                    label: "详情",
                                    icon: EyeIcon,
                                    onClick: () => openDetails(row.original),
                                },
                            ]}
                        />
                    )
                },
            },
        ],
        [openDetails],
    )
}
