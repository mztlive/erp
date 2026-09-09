"use client"
import { useState, type ReactNode } from "react"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { MoneyValue } from "@/components/business"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    requestStatusLabels,
    type InvoiceRequest,
    type RequestQuery,
} from "../api"
import {
    useInvoiceRequests,
    useInvoiceRequestAmounts,
    useInvoiceRequestPermissions,
} from "../hooks/queries"
import { InvoiceRequestForm } from "./request-form"
import { InvoiceRequestDetail } from "./request-detail"
/** 销售单和客户往来共用的申请列表、提交表单与详情入口。 */
export function InvoiceRequestPanel({
    salesOrderId,
    accountId,
    title,
    initialRequestId,
    initialCreate = false,
    query: filters = {},
    toolbar,
}: {
    salesOrderId?: string
    accountId?: string
    title?: string
    initialRequestId?: string
    initialCreate?: boolean
    query?: RequestQuery
    toolbar?: ReactNode
}) {
    const permissions = useInvoiceRequestPermissions()
    const [page, setPage] = useState(1)
    const [detailId, setDetailId] = useState(initialRequestId)
    const [creating, setCreating] = useState(initialCreate)
    const [editing, setEditing] = useState<InvoiceRequest>()
    const list = useInvoiceRequests(
        {
            ...filters,
            sales_order_id: salesOrderId ?? filters.sales_order_id,
            page,
            page_size: 10,
        },
        permissions.canRead,
    )
    const amounts = useInvoiceRequestAmounts(accountId, permissions.canRead)
    if (!permissions.canRead)
        return (
            <p className="text-sm text-muted-foreground">
                当前账号没有查看开票申请的权限。
            </p>
        )
    if ((creating || editing) && permissions.canSubmit)
        return (
            <section className="space-y-4">
                <h2 className="font-semibold">
                    {editing ? "修改开票申请" : "申请开票"}
                </h2>
                <InvoiceRequestForm
                    salesOrderId={salesOrderId}
                    accountId={accountId}
                    title={title}
                    existing={editing}
                    onCancel={() => {
                        setCreating(false)
                        setEditing(undefined)
                    }}
                    onDone={(request) => {
                        setCreating(false)
                        setEditing(undefined)
                        setDetailId(request.id)
                    }}
                />
            </section>
        )
    if (detailId)
        return (
            <InvoiceRequestDetail
                id={detailId}
                onBack={() => setDetailId(undefined)}
                onEdit={(request) => setEditing(request)}
            />
        )
    return (
        <section className="space-y-4" aria-label="开票申请">
            {toolbar}
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                    <h2 className="font-semibold">开票申请</h2>
                    <p className="mt-1 text-sm text-muted-foreground">
                        提交申请，审批通过后由财务开票。
                    </p>
                </div>
                {permissions.canSubmit ? (
                    <Button
                        id="invoice-request-create"
                        disabled={Boolean(
                            salesOrderId &&
                            accountId &&
                            (!amounts.data ||
                                !/[1-9]/.test(amounts.data.available_amount)),
                        )}
                        onClick={() => setCreating(true)}
                    >
                        {salesOrderId ? "申请开票" : "新建开票申请"}
                    </Button>
                ) : null}
            </div>
            {amounts.data ? (
                <div className="grid gap-3 rounded-lg bg-muted p-4 sm:grid-cols-3">
                    {[
                        ["可申请金额", amounts.data.available_amount],
                        ["审批中金额", amounts.data.pending_amount],
                        [
                            "已批准待开票",
                            amounts.data.approved_remaining_amount,
                        ],
                    ].map(([label, amount]) => (
                        <div key={label}>
                            <div className="text-xs text-muted-foreground">
                                {label}
                            </div>
                            <div className="mt-1 text-lg font-semibold">
                                <MoneyValue value={amount!} />
                            </div>
                        </div>
                    ))}
                </div>
            ) : null}
            {list.isPending ? (
                <p>正在读取申请记录…</p>
            ) : list.isError ? (
                <div role="alert">
                    {getErrorMessage(list.error, "申请记录读取失败")}
                    <Button
                        id="invoice-request-list-retry"
                        onClick={() => void list.refetch()}
                    >
                        重试
                    </Button>
                </div>
            ) : list.data?.items.length ? (
                <div className="overflow-x-auto">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                {[
                                    "申请单号",
                                    "销售单",
                                    "开票抬头",
                                    "申请金额",
                                    "已开票",
                                    "状态",
                                    "申请人",
                                    "申请时间",
                                ].map((label) => (
                                    <TableHead key={label}>{label}</TableHead>
                                ))}
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {list.data.items.map((request) => (
                                <TableRow key={request.id}>
                                    <TableCell>
                                        <Button
                                            id={`invoice-request-view-${toAutomationIdSegment(request.id)}`}
                                            variant="link"
                                            className="h-auto p-0"
                                            onClick={() =>
                                                setDetailId(request.id)
                                            }
                                        >
                                            {request.request_no}
                                        </Button>
                                    </TableCell>
                                    <TableCell>
                                        {request.sales_order_no}
                                    </TableCell>
                                    <TableCell>
                                        {request.data.invoice_title}
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={request.data.amount}
                                        />
                                    </TableCell>
                                    <TableCell>
                                        <MoneyValue
                                            value={request.invoiced_amount}
                                        />
                                    </TableCell>
                                    <TableCell>
                                        {requestStatusLabels[request.status]}
                                    </TableCell>
                                    <TableCell>
                                        {request.created_by_name ?? "—"}
                                    </TableCell>
                                    <TableCell className="whitespace-nowrap">
                                        {new Date(
                                            request.created_at * 1000,
                                        ).toLocaleDateString("zh-CN")}
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
            ) : (
                <p className="py-6 text-center text-sm text-muted-foreground">
                    暂无开票申请。有开票需求时，可在此发起申请。
                </p>
            )}
            {(list.data?.total ?? 0) > 10 ? (
                <div className="flex items-center justify-end gap-3 text-sm">
                    <span>
                        第 {page} 页 · 共 {list.data?.total} 条
                    </span>
                    <Button
                        id="invoice-request-page-prev"
                        variant="outline"
                        disabled={page === 1}
                        onClick={() => setPage(page - 1)}
                    >
                        上一页
                    </Button>
                    <Button
                        id="invoice-request-page-next"
                        variant="outline"
                        disabled={page * 10 >= (list.data?.total ?? 0)}
                        onClick={() => setPage(page + 1)}
                    >
                        下一页
                    </Button>
                </div>
            ) : null}
        </section>
    )
}
