"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ContractListRow } from "@/features/contracts/types"
import { contractOwnerLabel } from "@/features/contracts/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function useContractListColumns() {
    return React.useMemo<ColumnDef<ContractListRow>[]>(
        () => [
            {
                id: "contractNo",
                accessorKey: "contractNo",
                header: "合同编号",
                meta: { label: "合同编号", width: "reference" },
                cell: ({ row }) => (
                    <div className="min-w-0">
                        <Button
                            id={`card-contracts-list-row-${toAutomationIdSegment(row.original.contractId)}-open`}
                            type="button"
                            variant="link"
                            size="xs"
                            className="num px-0"
                            aria-label={`打开合同 ${row.original.contractNo}`}
                            render={
                                <Link
                                    href={`/sales/contracts/${row.original.contractId}`}
                                />
                            }
                        >
                            {row.original.contractNo}
                        </Button>
                        <div className="truncate text-xs text-muted-foreground">
                            {row.original.customer.displayName}
                            {" · "}
                            <span className="num">
                                {row.original.customer.customerNo}
                            </span>
                        </div>
                    </div>
                ),
            },
            {
                id: "settlement",
                accessorFn: (row) => row.settlementParty.displayName,
                header: "结算主体",
                meta: { label: "结算主体", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.settlementParty.displayName}
                    </span>
                ),
            },
            {
                id: "validity",
                header: "有效期",
                meta: { label: "有效期", width: "default", numeric: true },
                cell: ({ row }) => (
                    <div className="num text-sm">
                        <div>
                            {row.original.validFrom} ~ {row.original.validTo}
                        </div>
                        {row.original.expiringWithin30Days ? (
                            <div className="text-xs text-warning-foreground">
                                将到期
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                    />
                ),
            },
            {
                id: "revision",
                header: "版本",
                meta: { label: "版本", width: "status", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        v{row.original.revisionNo}
                    </span>
                ),
            },
            {
                id: "sales",
                header: "销售单",
                meta: { label: "关联销售单", width: "status", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm">
                        {row.original.salesOrderCount}
                        {row.original.activeSalesOrderCount > 0 ? (
                            <span className="text-muted-foreground">
                                {" "}
                                · 进行中 {row.original.activeSalesOrderCount}
                            </span>
                        ) : null}
                    </span>
                ),
            },
            {
                id: "owner",
                accessorKey: "ownerLabel",
                header: "负责人",
                meta: { label: "负责人", width: "default" },
                cell: ({ row }) => (
                    <span className="text-sm text-muted-foreground">
                        {contractOwnerLabel(row.original.ownerLabel)}
                    </span>
                ),
            },
        ],
        [],
    )
}
