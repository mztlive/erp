"use client"

import { useMemo } from "react"
import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
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
                meta: { width: "default", numeric: true },
                cell: ({ row }) => (
                    <TableTextCell
                        value={auditTimeLabel(row.original)}
                        numeric
                        clamp={1}
                    />
                ),
            },
            {
                id: "actor",
                header: "操作人",
                cell: ({ row }) => {
                    const name = auditActorLabel(row.original)
                    const account =
                        row.original.structured_event?.actor_account ??
                        row.original.actor_account
                    return (
                        <TableIdentityCell
                            primary={name}
                            primaryTitle={name}
                            secondary={account}
                            secondaryTitle={account}
                        />
                    )
                },
            },
            {
                id: "action",
                header: "业务动作",
                cell: ({ row }) => (
                    <TableTextCell
                        value={auditActionLabel(row.original)}
                        clamp={1}
                    />
                ),
            },
            {
                id: "object",
                header: "业务编号",
                meta: { width: "flex" },
                cell: ({ row }) => {
                    const label = auditObjectLabel(row.original)
                    return (
                        <TableIdentityCell
                            className="num"
                            primary={label}
                            primaryTitle={label}
                        />
                    )
                },
            },
            {
                id: "result",
                header: "执行结果",
                meta: { width: "status" },
                cell: ({ row }) => {
                    const result = auditResultView(row.original)
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={result.label}
                            tone={result.tone}
                            appearance={
                                result.tone === "success" ? "quiet" : "badge"
                            }
                        />
                    )
                },
            },
            {
                id: "changes",
                header: "字段变化",
                meta: { width: "reference" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={auditChangesLabel(row.original)}
                        clamp={1}
                    />
                ),
            },
            {
                id: "actions",
                header: "查看",
                meta: { align: "end", width: "status" },
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
