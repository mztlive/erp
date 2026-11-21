"use client"

import type { ColumnDef } from "@tanstack/react-table"
import {
    MoneyValue,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import {
    ListDataTable,
    ListSelectionBar,
    ListWorkSurface,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Alert } from "@/components/ui/alert"
import { useCandidateList } from "../hooks/use-candidate-list"
import { useCandidateSelection } from "../hooks/use-candidate-selection"
import { CandidateListFilters } from "./candidate-list-filters"
import type { PaymentCandidate } from "../types"
import { emptyCandidateFilters } from "../lib/candidate-list-query"

const columns: ColumnDef<PaymentCandidate>[] = [
    {
        accessorKey: "purchase_order_no",
        header: "采购单 / 采购内容",
        meta: { label: "采购单 / 采购内容", width: "flex" },
        cell: ({ row }) => (
            <TableIdentityCell
                primary={
                    <span className="num">
                        {row.original.purchase_order_no}
                    </span>
                }
                primaryTitle={row.original.purchase_order_no}
                secondary={row.original.purchase_summary}
                secondaryTitle={row.original.purchase_summary}
            />
        ),
    },
    {
        accessorKey: "supplier_name",
        header: "供应商 / 我方主体",
        meta: { label: "供应商 / 我方主体", width: "reference" },
        cell: ({ row }) => (
            <TableIdentityCell
                primary={row.original.supplier_name}
                primaryTitle={row.original.supplier_name}
                secondary={row.original.our_company_name}
                secondaryTitle={row.original.our_company_name}
            />
        ),
    },
    {
        accessorKey: "payment_term_name",
        header: "付款条件",
        meta: { label: "付款条件", width: "default" },
        cell: ({ row }) => (
            <TableTextCell value={row.original.payment_term_name} clamp={1} />
        ),
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
        cell: ({ row }) => (
            <MoneyValue display="compact" value={row.original[key]} />
        ),
    })),
]

const columnOrder = [
    "__selection",
    "purchase_order_no",
    "supplier_name",
    "payment_term_name",
    "total_amount",
    "paid_amount",
    "reserved_amount",
    "available_amount",
]
const columnPinning = { left: ["purchase_order_no"] }

export function PaymentCandidatesList({
    canRead,
    canCreate,
    onApply,
}: {
    canRead: boolean
    canCreate: boolean
    onApply: (ids: string[]) => void
}) {
    const list = useCandidateList(canRead)
    const { query } = list
    const selection = useCandidateSelection(list.applied, onApply)
    const rows = canRead ? (query.data?.items ?? []) : []
    const total = canRead ? (query.data?.total ?? 0) : 0
    const hasFilters = Object.values(list.applied).some(Boolean)
    const loading = canRead && (query.isPending || query.isFetching)
    const canSelect = canCreate && !loading && !query.isError && !selection.busy
    const resultStatus = query.isError
        ? "查询未完成"
        : query.isFetching
          ? "查询中…"
          : null
    const clearFilters = () => {
        selection.cancelPending()
        list.update({ ...emptyCandidateFilters, page: 1 })
    }
    return (
        <>
            {canCreate && selection.error && (
                <Alert variant="destructive">{selection.error}</Alert>
            )}
            <ListWorkSurface
                ariaLabel="可申请采购单列表"
                toolbar={
                    canRead ? (
                        <CandidateListFilters
                            key={JSON.stringify(list.applied)}
                            applied={list.applied}
                            canFilterSupplier={list.canFilterSupplier}
                            resultStatus={resultStatus}
                            onReset={clearFilters}
                            onApply={(filters) => {
                                selection.cancelPending()
                                list.update({ ...filters, page: 1 })
                            }}
                        />
                    ) : undefined
                }
                tableActions={
                    canCreate ? (
                        <Button
                            id="purchase-payment-candidates-select-all"
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={listWorkspaceStyles.quietButton}
                            disabled={!total || !canSelect}
                            title="跨页勾选全部符合已生效筛选条件的采购单"
                            onClick={() => void selection.select("all")}
                        >
                            {selection.busy && !selection.ids.length
                                ? "正在核对…"
                                : "全选筛选结果"}
                        </Button>
                    ) : undefined
                }
                selectionBarOpen={canCreate && selection.ids.length > 0}
                selectionBar={
                    canCreate ? (
                        <ListSelectionBar
                            idPrefix="purchase-payment-candidates"
                            resultCount={total}
                            pageRowCount={canSelect ? rows.length : 0}
                            selectedCount={selection.ids.length}
                            allPageSelected={
                                rows.length > 0 &&
                                rows.every(
                                    (row) =>
                                        selection.selection[
                                            row.payable_account_id
                                        ],
                                )
                            }
                            unit="张"
                            summaryExtra="· 按供应商和我方主体分组"
                            busy={selection.busy}
                            onSelectPage={() =>
                                selection.setSelection({
                                    ...selection.selection,
                                    ...Object.fromEntries(
                                        rows.map((row) => [
                                            row.payable_account_id,
                                            true,
                                        ]),
                                    ),
                                })
                            }
                            onClear={() => {
                                selection.setSelection({})
                                selection.cancelPending()
                            }}
                            actions={(display) => (
                                <Button
                                    id="purchase-payment-candidates-apply"
                                    type="button"
                                    variant={
                                        canSelect ? "default" : "secondary"
                                    }
                                    size="sm"
                                    disabled={
                                        !selection.ids.length || !canSelect
                                    }
                                    title={
                                        query.isError
                                            ? "请重试查询后发起申请"
                                            : undefined
                                    }
                                    onClick={() =>
                                        void selection.select("apply")
                                    }
                                >
                                    {selection.busy ? (
                                        "正在核对…"
                                    ) : (
                                        <>
                                            发起申请{" "}
                                            <span className="num">
                                                {display.selectedCount}
                                            </span>
                                        </>
                                    )}
                                </Button>
                            )}
                        />
                    ) : undefined
                }
                table={
                    <ListDataTable
                        idPrefix="purchase-payment-candidates-table"
                        entityName="可申请采购单"
                        columns={columns}
                        defaultColumnOrder={columnOrder}
                        defaultColumnPinning={columnPinning}
                        data={rows}
                        rowCount={total}
                        getRowId={(row) => row.payable_account_id}
                        rowLabel={(row) => row.purchase_order_no}
                        enableRowSelection={canCreate ? () => canSelect : false}
                        rowSelection={selection.selection}
                        onRowSelectionChange={(next) => {
                            if (canSelect) selection.setSelection(next)
                        }}
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
                        manualPagination
                        manualSorting
                        sorting={list.sorting}
                        onSortingChange={list.sort}
                        loading={loading}
                        isError={canRead && query.isError}
                        error={query.error}
                        onRetry={() => void query.refetch()}
                        noScope={!canRead}
                        hasActiveFilters={hasFilters}
                        onClearFilters={clearFilters}
                        emptyCopy={{
                            noScopeTitle: "当前账号没有采购付款申请的创建权限",
                            filterTitle: "没有匹配的采购单",
                            emptyTitle: "暂无可申请付款的采购单",
                            emptyDescription:
                                "仅显示已生效、有可申请余额且你有申请资格的采购单。",
                        }}
                        caption="可申请付款的采购单，按供应商和我方主体分别生成申请"
                    />
                }
            />
        </>
    )
}
