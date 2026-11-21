"use client"

import * as React from "react"
import { ASSIGNMENT_STRATEGY_LABEL } from "@/components/business/responsibility-pool/model"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import {
    PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL,
    type ProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export const RESPONSIBILITY_RULES_LIST_ID = "procurement-responsibility-rules"

export const ruleColumnOrder = [
    "scope",
    "ruleType",
    "owner",
    "status",
    "strategy",
    "actions",
]

export const ruleColumnPinning = { left: ["scope"], right: ["actions"] }
export const ruleColumnVisibility = { strategy: false }

/** 规则命中范围的可读文案，供列表主列展示。 */
export function ruleScope(rule: ProcurementResponsibilityRule): string {
    switch (rule.ruleType) {
        case "INQUIRY":
            return "所有询价整单优先分派，优先于商品规则"
        case "SKU":
            return rule.skuLabel ?? "SKU 待补充"
        case "CATEGORY":
            return rule.categoryLabel ?? "分类待补充"
        case "PRODUCT_KIND":
            return rule.productKind
                ? PRODUCT_KIND_LABELS[rule.productKind]
                : "商品类型待补充"
        case "DEFAULT_DISPATCHER":
            return "未匹配到其他规则时默认使用"
    }
}

export function getResponsibilityRuleLabel(
    rule: ProcurementResponsibilityRule,
) {
    return `${PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL[rule.ruleType]}：${ruleScope(rule)}`
}

export function useResponsibilityRulesColumns({
    onEdit,
    canUseSku,
    editDisabledReason,
}: {
    onEdit?: (rule: ProcurementResponsibilityRule) => void
    canUseSku: boolean
    editDisabledReason?: string
}) {
    return React.useMemo<ColumnDef<ProcurementResponsibilityRule>[]>(() => {
        const columns: ColumnDef<ProcurementResponsibilityRule>[] = [
            {
                id: "scope",
                header: "匹配范围",
                meta: { label: "匹配范围", width: "flex" },
                cell: ({ row }) => {
                    const scope = ruleScope(row.original)
                    return (
                        <TableIdentityCell
                            className="w-0 min-w-full"
                            primary={scope}
                            primaryTitle={scope}
                        />
                    )
                },
            },
            {
                id: "ruleType",
                header: "规则类型",
                meta: { label: "规则类型", width: "status" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={
                            PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL[
                                row.original.ruleType
                            ]
                        }
                    />
                ),
            },
            {
                id: "owner",
                header: "负责人池",
                meta: { label: "负责人池", width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.ownerNames.join("、")}
                        className="whitespace-normal break-words"
                        clamp={2}
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
        ]
        if (onEdit) {
            columns.push({
                id: "actions",
                header: "操作",
                meta: { label: "操作", align: "end", width: "status" },
                cell: ({ row }) => {
                    const rule = row.original
                    if (!canUseSku && rule.ruleType === "SKU") return null
                    const id = `${RESPONSIBILITY_RULES_LIST_ID}-row-${toAutomationIdSegment(rule.ruleId)}`
                    return (
                        <TableRowActions
                            moreId={`${id}-more`}
                            moreLabel={`${getResponsibilityRuleLabel(rule)} 更多操作`}
                            actions={[
                                {
                                    id: `${id}-edit`,
                                    label: "编辑",
                                    disabled: Boolean(editDisabledReason),
                                    disabledReason: editDisabledReason,
                                    onClick: () => onEdit(rule),
                                },
                            ]}
                        />
                    )
                },
            })
        }
        return columns
    }, [canUseSku, editDisabledReason, onEdit])
}
