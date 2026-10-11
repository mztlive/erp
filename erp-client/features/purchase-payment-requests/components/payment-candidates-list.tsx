"use client"

import { useState, type ReactNode } from "react"
import type { ColumnDef, RowSelectionState } from "@tanstack/react-table"
import { DataTable, MoneyValue } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { getErrorMessage } from "@/lib/api"
import { usePaymentCandidatesQuery } from "../hooks/queries"
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
    const query = usePaymentCandidatesQuery(canRead)
    const [search, setSearch] = useState("")
    const [selection, setSelection] = useState<RowSelectionState>({})
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const candidates = canRead ? (query.data ?? []) : []
    const rows = candidates.filter((item) =>
        [
            item.purchase_order_no,
            item.purchase_summary,
            item.supplier_name,
            item.our_company_name,
        ]
            .join(" ")
            .toLowerCase()
            .includes(search.trim().toLowerCase()),
    )
    // Refreshes may remove occupied sources. Never forward stale selections.
    const selectedIds = candidates
        .filter((item) => selection[item.payable_account_id])
        .map((item) => item.payable_account_id)
    const rowSelection = Object.fromEntries(selectedIds.map((id) => [id, true]))
    const allSelected =
        rows.length > 0 &&
        rows.every((item) => rowSelection[item.payable_account_id])
    return (
        <ListWorkSurface
            ariaLabel="可申请采购单列表"
            views={views}
            toolbar={
                <div className="space-y-2 py-3">
                    <Input
                        id="purchase-payment-candidates-search"
                        className="max-w-md"
                        value={search}
                        placeholder="搜索采购单、采购内容、供应商或我方主体"
                        aria-label="搜索可申请采购单"
                        onChange={(event) => {
                            setSearch(event.target.value)
                            setPagination((current) => ({
                                ...current,
                                pageIndex: 0,
                            }))
                        }}
                    />
                    <p className="text-xs text-muted-foreground">
                        仅显示已生效、有可申请余额且你有申请资格的采购单。支持跨供应商勾选，按供应商和我方主体分别生成申请。
                    </p>
                </div>
            }
            selectionBar={
                <div className="flex flex-wrap items-center gap-2 text-sm">
                    <span className="text-muted-foreground">
                        共 {rows.length} 张 · 已选 {selectedIds.length} 张
                    </span>
                    <Button
                        id="purchase-payment-candidates-select-all"
                        variant="ghost"
                        size="sm"
                        disabled={
                            !canCreate ||
                            !rows.length ||
                            allSelected ||
                            query.isError
                        }
                        onClick={() =>
                            setSelection({
                                ...rowSelection,
                                ...Object.fromEntries(
                                    rows.map((item) => [
                                        item.payable_account_id,
                                        true,
                                    ]),
                                ),
                            })
                        }
                    >
                        全选筛选结果
                    </Button>
                    <Button
                        id="purchase-payment-candidates-clear"
                        variant="ghost"
                        size="sm"
                        disabled={!selectedIds.length}
                        onClick={() => setSelection({})}
                    >
                        清空
                    </Button>
                    <Button
                        id="purchase-payment-candidates-apply"
                        size="sm"
                        disabled={
                            !canCreate ||
                            !selectedIds.length ||
                            query.isFetching ||
                            query.isError
                        }
                        title={
                            !canCreate
                                ? "当前账号需要采购付款申请的创建和提交权限"
                                : undefined
                        }
                        onClick={() => onApply(selectedIds)}
                    >
                        发起申请
                        {selectedIds.length ? `（${selectedIds.length}）` : ""}
                    </Button>
                </div>
            }
            table={
                <DataTable
                    idPrefix="purchase-payment-candidates-table"
                    columns={columns}
                    data={rows}
                    rowCount={rows.length}
                    getRowId={(row) => row.payable_account_id}
                    rowLabel={(row) => row.purchase_order_no}
                    enableRowSelection={canCreate && !query.isError}
                    rowSelection={rowSelection}
                    onRowSelectionChange={setSelection}
                    pagination={{
                        ...pagination,
                        pageIndex: Math.min(
                            pagination.pageIndex,
                            Math.max(
                                0,
                                Math.ceil(rows.length / pagination.pageSize) -
                                    1,
                            ),
                        ),
                    }}
                    onPaginationChange={setPagination}
                    manualPagination={false}
                    manualSorting={false}
                    loading={canRead && query.isPending}
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
                            : search.trim()
                              ? "没有匹配的采购单"
                              : "暂无可申请付款的采购单"
                    }
                    emptyDescription={
                        canRead
                            ? "请确认采购已生效、仍有可申请余额，或调整搜索条件。"
                            : undefined
                    }
                    caption="可申请付款的采购单"
                />
            }
        />
    )
}
