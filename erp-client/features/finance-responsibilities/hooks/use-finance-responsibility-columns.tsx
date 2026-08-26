"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import {
    FINANCE_OPERATION_LABEL,
    FINANCE_SCOPE_LABEL,
    type FinanceResponsibilityRule,
} from "@/features/finance-responsibilities/types"

function matchLabel(rule: FinanceResponsibilityRule): string {
    if (rule.scope === "DEFAULT") return "未命中指定往来方时使用"
    return rule.counterpartyNo ?? "往来方资料不可用"
}

/** 财务责任规则列表列；编辑沿用表格行打开合同。 */
export function useFinanceResponsibilityColumns() {
    return React.useMemo<ColumnDef<FinanceResponsibilityRule>[]>(
        () => [
            {
                id: "operation",
                accessorFn: (row) => FINANCE_OPERATION_LABEL[row.operation],
                header: "业务操作",
                meta: { label: "业务操作", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm font-medium">
                        {FINANCE_OPERATION_LABEL[row.original.operation]}
                    </span>
                ),
            },
            {
                id: "scope",
                accessorFn: (row) => FINANCE_SCOPE_LABEL[row.scope],
                header: "匹配层级",
                meta: { label: "匹配层级", width: "status" },
            },
            {
                id: "counterparty",
                accessorFn: matchLabel,
                header: "匹配范围",
                meta: { label: "匹配范围", width: "flex" },
                cell: ({ row }) => (
                    <span className="text-sm">{matchLabel(row.original)}</span>
                ),
            },
            {
                id: "owner",
                accessorFn: (row) => row.ownerName,
                header: "负责人",
                meta: { label: "负责人", width: "default" },
            },
            {
                id: "status",
                accessorFn: (row) => (row.enabled ? "已启用" : "已停用"),
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.enabled ? "已启用" : "已停用"}
                        tone={row.original.enabled ? "success" : "neutral"}
                    />
                ),
            },
        ],
        [],
    )
}
