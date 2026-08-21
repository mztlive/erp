"use client"

import type * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataTable,
    MoneyValue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    VIEW_LABEL,
    type CustomerAccountsListView,
    type CustomerAccountsView,
    type ReceiptRow,
    type ReceivableAccountRow,
    type SalesInvoiceRow,
} from "@/features/customer-receivables/types"
import type { CustomerReceivablesPatchUrl } from "../hooks/use-customer-receivables-url-state"

type CustomerReceivablesTableProps = {
    view: CustomerAccountsView
    data: CustomerAccountsListView | undefined
    isPending: boolean
    metrics: CustomerAccountsListView["metrics"] | undefined
    pagination: PaginationState
    receivableColumns: ColumnDef<ReceivableAccountRow>[]
    receiptColumns: ColumnDef<ReceiptRow>[]
    invoiceColumns: ColumnDef<SalesInvoiceRow>[]
    toolbar: React.ReactNode
    patchUrl: CustomerReceivablesPatchUrl
    onPaginationChange: (next: PaginationState) => void
    clearFilters: () => void
}

export function CustomerReceivablesTable({
    view,
    data,
    isPending,
    metrics,
    pagination,
    receivableColumns,
    receiptColumns,
    invoiceColumns,
    toolbar,
    patchUrl,
    onPaginationChange,
    clearFilters,
}: CustomerReceivablesTableProps) {
    return (
        <>
            <Tabs
                value={view}
                onValueChange={(v) => {
                    // 非 receivable 视图隐藏 due/status/reviewStatus，切视图时清除残留
                    const patch: Record<string, string | null | undefined> = {
                        view: v,
                        page: null,
                    }
                    if (v !== "receivable") {
                        patch.due = null
                        patch.status = null
                        patch.reviewStatus = null
                    }
                    patchUrl(patch, { replace: true })
                }}
            >
                <TabsList>
                    {(
                        [
                            "receivable",
                            "receipt",
                            "sales_invoice",
                            "unallocated",
                        ] as const
                    ).map((v) => (
                        <TabsTrigger key={v} value={v}>
                            {VIEW_LABEL[v]}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            <BusinessTableFrame
                title={VIEW_LABEL[view]}
                description={
                    <span aria-live="polite">
                        {data?.filterSummary ?? "加载中…"}
                        {data ? (
                            <span className="text-muted-foreground">
                                {" "}
                                · 提交方式：{data.submitPolicy.label}
                            </span>
                        ) : null}
                    </span>
                }
                toolbar={toolbar}
                table={
                    isPending && !data ? (
                        <div className="h-64 animate-pulse rounded-xl bg-muted" />
                    ) : view === "unallocated" && data ? (
                        <div className="space-y-6 p-1">
                            <Alert variant="info">
                                <AlertTitle>待核销分区</AlertTitle>
                                <AlertDescription>
                                    {data.unallocated.note}
                                </AlertDescription>
                            </Alert>
                            <section className="space-y-2">
                                <h3 className="text-sm font-semibold">
                                    待分配回款
                                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                                        未分配{" "}
                                        <MoneyValue
                                            value={
                                                metrics?.unallocatedReceiptTotal ??
                                                "0"
                                            }
                                            className="inline"
                                        />
                                    </span>
                                </h3>
                                {data.unallocated.receipts.length === 0 ? (
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title="无待分配回款"
                                        description="已确认且仍有未分配余额的回款将出现在此。"
                                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    />
                                ) : (
                                    <DataTable
                                        data={[...data.unallocated.receipts]}
                                        columns={receiptColumns}
                                        getRowId={(r) => r.receiptId}
                                        rowCount={
                                            data.unallocated.receipts.length
                                        }
                                        layout="flush"
                                        defaultColumnPinning={{
                                            left: ["doc"],
                                            right: ["actions"],
                                        }}
                                    />
                                )}
                            </section>
                            <Separator />
                            <section className="space-y-2">
                                <h3 className="text-sm font-semibold">
                                    待分配销项发票
                                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                                        未分配{" "}
                                        <MoneyValue
                                            value={
                                                metrics?.unallocatedInvoiceTotal ??
                                                "0"
                                            }
                                            className="inline"
                                        />
                                        （独立统计）
                                    </span>
                                </h3>
                                {data.unallocated.invoices.length === 0 ? (
                                    <BusinessEmptyState
                                        kind="no-data"
                                        title="无待分配销项发票"
                                        description="已登记蓝票且仍有未分配余额的发票将出现在此。"
                                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                    />
                                ) : (
                                    <DataTable
                                        data={[...data.unallocated.invoices]}
                                        columns={invoiceColumns}
                                        getRowId={(r) => r.invoiceId}
                                        rowCount={
                                            data.unallocated.invoices.length
                                        }
                                        layout="flush"
                                        defaultColumnPinning={{
                                            left: ["doc"],
                                            right: ["actions"],
                                        }}
                                    />
                                )}
                            </section>
                        </div>
                    ) : data?.total === 0 ? (
                        data.emptyReason === "FILTER_NO_RESULT" ? (
                            <BusinessEmptyState
                                kind="filter"
                                title="无匹配往来记录"
                                description="无匹配记录，可清除筛选后重试。"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                action={
                                    <Button
                                        type="button"
                                        variant="secondary"
                                        className="rounded-lg shadow-none"
                                        onClick={clearFilters}
                                    >
                                        清除筛选
                                    </Button>
                                }
                            />
                        ) : (
                            <BusinessEmptyState
                                kind="no-data"
                                title="当前范围尚无客户往来记录"
                                description="可从销售单进入登记；登记后刷新查看。"
                                className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                            />
                        )
                    ) : view === "receivable" && data ? (
                        <DataTable
                            data={[...data.receivables]}
                            columns={receivableColumns}
                            getRowId={(r) => r.accountId}
                            rowCount={data.total}
                            pagination={pagination}
                            onPaginationChange={onPaginationChange}
                            layout="flush"
                            defaultColumnPinning={{
                                left: ["party"],
                                right: ["actions"],
                            }}
                        />
                    ) : view === "receipt" && data ? (
                        <DataTable
                            data={[...data.receipts]}
                            columns={receiptColumns}
                            getRowId={(r) => r.receiptId}
                            rowCount={data.total}
                            pagination={pagination}
                            onPaginationChange={onPaginationChange}
                            layout="flush"
                            defaultColumnPinning={{
                                left: ["doc"],
                                right: ["actions"],
                            }}
                        />
                    ) : view === "sales_invoice" && data ? (
                        <DataTable
                            data={[...data.invoices]}
                            columns={invoiceColumns}
                            getRowId={(r) => r.invoiceId}
                            rowCount={data.total}
                            pagination={pagination}
                            onPaginationChange={onPaginationChange}
                            layout="flush"
                            defaultColumnPinning={{
                                left: ["doc"],
                                right: ["actions"],
                            }}
                        />
                    ) : (
                        <div className="h-40 animate-pulse rounded-xl bg-muted" />
                    )
                }
            />
        </>
    )
}
