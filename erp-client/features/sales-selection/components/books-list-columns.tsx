"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    bookIdentity,
    bookletStatusTone,
    formatBookInstant,
} from "@/features/sales-selection/lib/presentation"
import type { SelectionBook } from "@/features/sales-selection/types"
import {
    BOOKLET_STATUS_LABEL,
    SELECTION_FORM_LABEL,
    SUBMIT_MODE_LABEL,
} from "@/features/sales-selection/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 选品册列表列：客户身份列可打开详情，已提交行链方案。 */
export function buildBooksListColumns(): ColumnDef<SelectionBook>[] {
    return [
        {
            id: "customer",
            accessorKey: "customer_name",
            header: "客户",
            enableSorting: false,
            meta: { label: "客户", width: "flex" },
            cell: ({ row }) => {
                const bookId = bookIdentity(row.original)
                const rowKey = toAutomationIdSegment(bookId)
                return (
                    <div className="flex min-w-0 flex-col gap-1">
                        <Button
                            id={`sales-selection-row-${rowKey}-open`}
                            variant="link"
                            size="xs"
                            className="h-auto min-h-0 justify-start px-0 text-sm leading-5 font-semibold text-foreground"
                            aria-label={`查看 ${row.original.customer_name} 的选品册`}
                            render={
                                <Link href={`/sales/selection/${bookId}`} />
                            }
                        >
                            {row.original.customer_name}
                        </Button>
                        <span className="num truncate text-xs text-muted-foreground">
                            {bookId}
                        </span>
                    </div>
                )
            },
        },
        {
            id: "form",
            header: "形态",
            enableSorting: false,
            meta: { label: "形态", width: "status" },
            cell: ({ row }) => (
                <span className="text-sm">
                    {
                        SELECTION_FORM_LABEL[
                            row.original.selection_form ?? row.original.form
                        ]
                    }
                </span>
            ),
        },
        {
            id: "submitMode",
            header: "提交方式",
            enableSorting: false,
            meta: { label: "提交方式", width: "status" },
            cell: ({ row }) => (
                <span className="text-sm">
                    {SUBMIT_MODE_LABEL[row.original.submit_mode]}
                </span>
            ),
        },
        {
            id: "status",
            header: "状态",
            enableSorting: false,
            meta: { label: "状态", width: "status" },
            cell: ({ row }) => (
                <BusinessStatusBadge
                    context="list"
                    label={BOOKLET_STATUS_LABEL[row.original.status]}
                    tone={bookletStatusTone(row.original.status)}
                />
            ),
        },
        {
            id: "display",
            header: "陈列",
            enableSorting: false,
            meta: { label: "陈列", width: "quantity", numeric: true },
            cell: ({ row }) => (
                <span className="num text-sm">
                    {row.original.display_count ?? "—"}
                </span>
            ),
        },
        {
            id: "proposal",
            header: "方案",
            enableSorting: false,
            meta: { label: "方案", width: "reference" },
            cell: ({ row }) => {
                const bookId = bookIdentity(row.original)
                const rowKey = toAutomationIdSegment(bookId)
                if (!row.original.proposal_id || !row.original.proposal_no) {
                    return (
                        <span className="text-sm text-muted-foreground">—</span>
                    )
                }
                return (
                    <Button
                        id={`sales-selection-row-${rowKey}-proposal`}
                        variant="link"
                        size="xs"
                        className="num h-auto min-h-0 px-0 text-sm leading-5 font-semibold text-foreground"
                        render={
                            <Link
                                href={`/sales/selection/proposals/${row.original.proposal_id}`}
                            />
                        }
                    >
                        {row.original.proposal_no}
                    </Button>
                )
            },
        },
        {
            id: "createdAt",
            header: "创建时间",
            enableSorting: false,
            meta: { label: "创建时间", width: "reference", numeric: true },
            cell: ({ row }) => (
                <span className="num text-sm text-muted-foreground">
                    {formatBookInstant(row.original.created_at)}
                </span>
            ),
        },
    ]
}
