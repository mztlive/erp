"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import {
    PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL,
    type ProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/types"

/** 规则命中范围的可读文案，供列表主列展示。 */
export function ruleScope(rule: ProcurementResponsibilityRule): string {
    switch (rule.ruleType) {
        case "SKU":
            return rule.skuLabel ?? "SKU 待补充"
        case "CATEGORY_SERVICE_REGION":
            return `${rule.categoryLabel ?? "分类待补充"} · ${rule.serviceRegion ?? "区域待补充"}`
        case "CATEGORY":
            return rule.categoryLabel ?? "分类待补充"
        case "PRODUCT_KIND":
            return rule.productKind
                ? PRODUCT_KIND_LABELS[rule.productKind]
                : "商品类型待补充"
        case "DEFAULT_DISPATCHER":
            return "未命中更具体规则时使用"
    }
}

/** 采购责任规则列表列：类型、范围、负责人、状态。编辑走行点击，不另开操作列。 */
export function useResponsibilityRulesColumns() {
    return React.useMemo<ColumnDef<ProcurementResponsibilityRule>[]>(
        () => [
            {
                id: "ruleType",
                accessorFn: (row) =>
                    PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL[row.ruleType],
                header: "规则类型",
                meta: { label: "规则类型", width: "status" },
                cell: ({ row }) => (
                    <span className="text-sm font-medium">
                        {
                            PROCUREMENT_RESPONSIBILITY_RULE_TYPE_LABEL[
                                row.original.ruleType
                            ]
                        }
                    </span>
                ),
            },
            {
                id: "scope",
                accessorFn: (row) => ruleScope(row),
                header: "匹配范围",
                meta: { label: "匹配范围", width: "flex" },
                cell: ({ row }) => (
                    <span className="text-sm text-foreground">
                        {ruleScope(row.original)}
                    </span>
                ),
            },
            {
                id: "owner",
                accessorFn: (row) => row.ownerName,
                header: "采购负责人",
                meta: { label: "采购负责人", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm">{row.original.ownerName}</span>
                ),
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
