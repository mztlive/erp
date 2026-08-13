"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"
import type { SettlementListRow } from "@/features/supplier-settlements/types"

export function useSettlementListColumns(
    patchUrl: (patch: Partial<SettlementsUrlState>) => void,
    onOpen: (statementId: string) => void,
) {
    return React.useMemo<ColumnDef<SettlementListRow>[]>(
        () => [
            {
                id: "statementNo",
                accessorFn: (row) => row.statementNo,
                header: "结算单号",
                meta: { label: "结算单号", width: "reference" },
                cell: ({ row }) => (
                    <div className="num text-sm font-medium">
                        {row.original.statementNo}
                    </div>
                ),
            },
            {
                id: "supplier",
                accessorFn: (row) => row.supplierName,
                header: "供应商",
                meta: { label: "供应商" },
                cell: ({ row }) => (
                    <span className="text-sm">{row.original.supplierName}</span>
                ),
            },
            {
                id: "period",
                accessorFn: (row) => row.periodLabel,
                header: "期间",
                meta: { label: "期间", width: "status" },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.periodStart} ~ {row.original.periodEnd}
                    </span>
                ),
            },
            {
                id: "erpAmount",
                accessorFn: (row) => row.erpAmountGross,
                header: "ERP 金额",
                meta: {
                    label: "ERP 计算金额（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue
                        value={row.original.erpAmountGross}
                        taxBasis="gross"
                    />
                ),
            },
            {
                id: "supplierAmount",
                accessorFn: (row) => row.supplierAmountGross ?? "",
                header: "账单金额",
                meta: {
                    label: "供应商账单金额（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) =>
                    row.original.supplierAmountGross != null ? (
                        <MoneyValue
                            value={row.original.supplierAmountGross}
                            taxBasis="gross"
                        />
                    ) : (
                        <span className="text-xs text-muted-foreground">
                            账单未同步
                        </span>
                    ),
            },
            {
                id: "difference",
                accessorFn: (row) => row.differenceAmountGross ?? "",
                header: "差异",
                meta: {
                    label: "差异金额（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-right">
                        {row.original.differenceAmountGross != null ? (
                            <MoneyValue
                                value={row.original.differenceAmountGross}
                                taxBasis="gross"
                            />
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                —
                            </span>
                        )}
                        {row.original.differenceDirectionLabel ? (
                            <div className="text-tiny text-muted-foreground">
                                {row.original.differenceDirectionLabel}
                            </div>
                        ) : null}
                        {row.original.unresolvedDifferenceCount > 0 ? (
                            <Badge
                                variant="outline"
                                className="mt-0.5 text-2xs"
                            >
                                未决 {row.original.unresolvedDifferenceCount}
                            </Badge>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "status",
                accessorFn: (row) => row.statusLabel,
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "actors",
                accessorFn: (row) =>
                    `${row.preparedByLabel}/${row.reviewedByLabel}`,
                header: "经办/复核",
                meta: { label: "经办/复核" },
                cell: ({ row }) => (
                    <div className="text-xs text-muted-foreground">
                        <div>经办 {row.original.preparedByLabel}</div>
                        <div>复核 {row.original.reviewedByLabel}</div>
                    </div>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <div className="flex flex-wrap gap-1">
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                patchUrl({ preview: row.original.statementId })
                            }
                        >
                            预览
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            onClick={() => onOpen(row.original.statementId)}
                        >
                            打开
                        </Button>
                    </div>
                ),
            },
        ],
        [onOpen, patchUrl],
    )
}
