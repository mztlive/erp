import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CustomerDirectoryItem } from "@/features/customers/types"

/** 客户目录列定义；排序列由 URL 状态驱动，单元格为纯展示。 */
export function useCustomerDirectoryColumns(): ColumnDef<CustomerDirectoryItem>[] {
    return React.useMemo<ColumnDef<CustomerDirectoryItem>[]>(
        () => [
            {
                id: "customer",
                accessorFn: (row) => row.shortName || row.legalName,
                header: "客户",
                meta: { label: "客户", width: "reference" },
                enableSorting: false,
                cell: ({ row }) => (
                    <div className="min-w-0">
                        <Link
                            id={`customers-directory-row-${toAutomationIdSegment(row.original.id)}-open`}
                            href={`/sales/customers/${row.original.id}`}
                            className="font-medium text-foreground underline-offset-4 hover:underline"
                        >
                            {row.original.shortName || row.original.legalName}
                        </Link>
                        <div className="flex flex-wrap items-center gap-1.5">
                            <span className="num text-xs text-muted-foreground">
                                {row.original.customerNo}
                            </span>
                            {row.original.attentionTags?.map((tag) => (
                                <Badge
                                    key={tag}
                                    variant="outline"
                                    className="text-2xs"
                                >
                                    {tag}
                                </Badge>
                            ))}
                        </div>
                    </div>
                ),
            },
            {
                id: "owner",
                accessorKey: "ownerName",
                header: "负责销售",
                meta: { label: "负责销售", width: "default" },
                enableSorting: false,
                cell: ({ row }) => (
                    <div className="text-sm">
                        <div>{row.original.ownerName}</div>
                        {row.original.collaboratorCount > 0 ? (
                            <div className="text-xs text-muted-foreground">
                                协作 {row.original.collaboratorCount} 人
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "status",
                accessorFn: (row) => row.statusLabel.label,
                header: "状态",
                meta: { label: "状态", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        {...row.original.statusLabel}
                    />
                ),
            },
            {
                id: "business",
                accessorFn: (row) => row.updatedAt,
                header: "资料更新",
                meta: { label: "资料更新", width: "default", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm text-muted-foreground">
                        {row.original.updatedAt.slice(0, 10)}
                    </span>
                ),
            },
        ],
        [],
    )
}
