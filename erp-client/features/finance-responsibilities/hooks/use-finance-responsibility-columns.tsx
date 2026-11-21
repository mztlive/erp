"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { ASSIGNMENT_STRATEGY_LABEL } from "@/components/business/responsibility-pool/model"
import {
    FINANCE_OPERATION_LABEL,
    FINANCE_SCOPE_LABEL,
    type FinanceResponsibilityRule,
} from "@/features/finance-responsibilities/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

function matchLabel(rule: FinanceResponsibilityRule): string {
    if (rule.scope === "DEFAULT") return "未单独指定客户/供应商时生效"
    return rule.counterpartyNo ?? "客户/供应商资料不可用"
}

export function describeFinanceResponsibilityRule(
    rule: FinanceResponsibilityRule,
): string {
    return `${FINANCE_OPERATION_LABEL[rule.operation]} · ${matchLabel(rule)}`
}

export function useFinanceResponsibilityColumns({
    canManage,
    ownerDisabledReason,
    onEdit,
}: {
    canManage: boolean
    ownerDisabledReason?: string
    onEdit: (rule: FinanceResponsibilityRule) => void
}) {
    return React.useMemo(() => {
        const columns: ColumnDef<FinanceResponsibilityRule>[] = [
            {
                id: "operation",
                accessorFn: (rule) => FINANCE_OPERATION_LABEL[rule.operation],
                header: "业务操作",
                meta: { label: "业务操作", width: "flex" },
                cell: ({ row }) => {
                    const rule = row.original
                    const match = matchLabel(rule)
                    return (
                        <TableIdentityCell
                            primary={FINANCE_OPERATION_LABEL[rule.operation]}
                            secondary={match}
                            secondaryTitle={match}
                            numericSecondary={
                                rule.scope === "COUNTERPARTY" &&
                                Boolean(rule.counterpartyNo)
                            }
                        />
                    )
                },
            },
            {
                id: "scope",
                header: "适用范围",
                meta: { label: "适用范围", width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={FINANCE_SCOPE_LABEL[row.original.scope]}
                    />
                ),
            },
            {
                id: "counterparty",
                header: "匹配范围",
                meta: { label: "匹配范围", width: "reference" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={matchLabel(row.original)}
                        clamp={1}
                        numeric={
                            row.original.scope === "COUNTERPARTY" &&
                            Boolean(row.original.counterpartyNo)
                        }
                    />
                ),
            },
            {
                id: "owner",
                header: "经办人组",
                meta: { label: "经办人组", width: "reference" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.ownerNames.join("、")}
                        clamp={2}
                    />
                ),
            },
            {
                id: "strategy",
                header: "分配方式",
                meta: { label: "分配方式", width: "status" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={ASSIGNMENT_STRATEGY_LABEL[row.original.strategy]}
                    />
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.enabled ? "已启用" : "已停用"}
                        tone={row.original.enabled ? "success" : "neutral"}
                        appearance={row.original.enabled ? "quiet" : "badge"}
                    />
                ),
            },
        ]
        if (canManage)
            columns.push({
                id: "actions",
                header: "操作",
                meta: { label: "操作", align: "end", width: "status" },
                cell: ({ row }) => {
                    const id = `finance-responsibilities-rules-row-${toAutomationIdSegment(row.original.id)}`
                    return (
                        <TableRowActions
                            moreId={`${id}-more`}
                            moreLabel={`${describeFinanceResponsibilityRule(row.original)} 更多操作`}
                            maxInline={1}
                            actions={[
                                {
                                    id: `${id}-edit`,
                                    label: "编辑",
                                    disabled: Boolean(ownerDisabledReason),
                                    disabledReason: ownerDisabledReason,
                                    onClick: () => onEdit(row.original),
                                },
                            ]}
                        />
                    )
                },
            })
        return columns
    }, [canManage, ownerDisabledReason, onEdit])
}
