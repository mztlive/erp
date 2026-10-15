"use client"

import { type ReactNode } from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { DataTable, MoneyValue } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import { useCandidateList } from "../hooks/use-candidate-list"
import { useCandidateSelection } from "../hooks/use-candidate-selection"
import { CandidateListFilters } from "./candidate-list-filters"
import type { PaymentCandidate } from "../types"

const columns: ColumnDef<PaymentCandidate>[] = [
    {
        accessorKey: "purchase_order_no",
        header: "采购单 / 采购内容",
        meta: { label: "采购单 / 采购内容", width: "flex" },
        cell: ({ row }) => (
            <div>
                <p className="num font-medium">
                    {row.original.purchase_order_no}
                </p>
                <p className="text-xs text-muted-foreground">
                    {row.original.purchase_summary}
                </p>
            </div>
        ),
    },
    {
        accessorKey: "supplier_name",
        header: "供应商 / 我方主体",
        meta: { label: "供应商 / 我方主体", width: "flex" },
        cell: ({ row }) => (
            <div>
                <p className="font-medium">{row.original.supplier_name}</p>
                <p className="text-xs text-muted-foreground">
                    {row.original.our_company_name}
                </p>
            </div>
        ),
    },
    {
        accessorKey: "payment_term_name",
        header: "付款条件",
        meta: { label: "付款条件", width: "default" },
    },
    ...(
        [
            ["total_amount", "采购总额"],
            ["paid_amount", "有效已付"],
            ["reserved_amount", "申请占用"],
            ["available_amount", "可申请余额"],
        ] as const
    ).map(([key, label]): ColumnDef<PaymentCandidate> => ({
        accessorKey: key,
        header: label,
        meta: { label, width: "amount", align: "end", numeric: true },
        cell: ({ row }) => <MoneyValue value={row.original[key]} />,
    })),
]

export function PaymentCandidatesList({
    views,
    canRead,
    canCreate,
    onApply,
}: {
    views: ReactNode
    canRead: boolean
    canCreate: boolean
    onApply: (ids: string[]) => void
}) {
    const list = useCandidateList(canRead)
    const { query } = list
    const selection = useCandidateSelection(list.applied, onApply)
    const rows = canRead ? (query.data?.items ?? []) : []
    const total = canRead ? (query.data?.total ?? 0) : 0
    const hasFilters = Object.values(list.applied).some((value) => value.trim())
    return (
        <ListWorkSurface
            ariaLabel="可申请采购单列表"
            views={views}
            toolbar={
                <CandidateListFilters
                    key={JSON.stringify(list.applied)}
                    applied={list.applied}
                    onApply={(filters) => {
                        selection.cancelPending()
                        list.update({ ...filters, page: 1 })
                    }}
                />
            }
            selectionBar={
                <div className="flex flex-wrap items-center gap-2 text-sm">
                    <span className="text-muted-foreground">
                        共 {total} 张 · 已选 {selection.ids.length} 张
                    </span>
                    <Button
                        id="purchase-payment-candidates-select-all"
                        variant="ghost"
                        size="sm"
                        disabled={
                            !canCreate ||
                            !total ||
                            selection.busy ||
                            query.isFetching ||
                            query.isError
                        }
                        onClick={() => void selection.select("all")}
                    >
                        全选筛选结果
                    </Button>
                    <Button
                        id="purchase-payment-candidates-clear"
                        variant="ghost"
                        size="sm"
                        disabled={!selection.ids.length || selection.busy}
                        onClick={() => {
                            selection.setSelection({})
                            selection.cancelPending()
                        }}
                    >
                        清空
                    </Button>
                    <Button
                        id="purchase-payment-candidates-apply"
                        size="sm"
                        disabled={
                            !canCreate ||
                            !selection.ids.length ||
                            selection.busy ||
                            query.isFetching ||
                            query.isError
                        }
                        title={
                            !canCreate
                                ? "当前账号需要采购付款申请的创建和提交权限"
                                : undefined
                        }
                        onClick={() => void selection.select("apply")}
                    >
                        发起申请
                        {selection.busy
                            ? "（正在核对…）"
                            : selection.ids.length
                              ? `（${selection.ids.length}）`
                              : ""}
                    </Button>
                    {selection.error && (
                        <p
                            role="alert"
                            className="w-full text-sm text-destructive"
                        >
                            {selection.error}
                        </p>
                    )}
                </div>
            }
            table={
                <DataTable
                    idPrefix="purchase-payment-candidates-table"
                    columns={columns}
                    data={rows}
                    rowCount={total}
                    getRowId={(row) => row.payable_account_id}
                    rowLabel={(row) => row.purchase_order_no}
                    enableRowSelection={
                        canCreate &&
                        !query.isError &&
                        !query.isFetching &&
                        !selection.busy
                    }
                    rowSelection={selection.selection}
                    onRowSelectionChange={selection.setSelection}
                    pagination={{
                        pageIndex: list.url.page - 1,
                        pageSize: list.url.page_size,
                    }}
                    onPaginationChange={(next) =>
                        list.update({
                            page:
                                next.pageSize === list.url.page_size
                                    ? next.pageIndex + 1
                                    : 1,
                            page_size: next.pageSize,
                        })
                    }
                    pageSizeOptions={[20, 50, 100]}
                    manualPagination
                    manualSorting
                    sorting={list.sorting}
                    onSortingChange={list.sort}
                    loading={canRead && query.isFetching}
                    errorState={
                        canRead && query.isError ? (
                            <p
                                role="alert"
                                className="py-6 text-sm text-destructive"
                            >
                                {getErrorMessage(
                                    query.error,
                                    "可申请采购单查询失败，请刷新后重试",
                                )}
                            </p>
                        ) : undefined
                    }
                    emptyTitle={
                        !canRead
                            ? "当前账号没有采购付款申请的创建权限"
                            : hasFilters
                              ? "没有匹配的采购单"
                              : "暂无可申请付款的采购单"
                    }
                    emptyDescription={
                        canRead
                            ? "请确认采购已生效、仍有可申请余额，或调整查询条件。"
                            : undefined
                    }
                    caption="可申请付款的采购单"
                />
            }
        />
    )
}
