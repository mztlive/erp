"use client"

import { useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ArrowUpRightIcon, PlusIcon, RefreshCwIcon } from "lucide-react"
import {
    BusinessStatusBadge,
    DataTable,
    MoneyValue,
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api"
import { hasPermission } from "@/lib/permissions"
import { usePaymentRequestsQuery } from "../hooks/queries"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import type { PaymentRequest } from "../types"
import { RequestDraftDialog } from "../components/request-draft-dialog"

const columns: ColumnDef<PaymentRequest>[] = [
    {
        id: "number",
        accessorKey: "request_no",
        header: "申请单号",
        meta: { label: "申请单号", width: "reference" },
        cell: ({ row }) => (
            <span className="num">{row.original.request_no}</span>
        ),
    },
    {
        id: "supplier",
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
        id: "applicant",
        accessorKey: "applicant_name",
        header: "申请人",
        meta: { label: "申请人", width: "default" },
    },
    {
        id: "amount",
        accessorKey: "total_amount",
        header: "申请金额",
        meta: {
            label: "申请金额",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.total_amount} />,
    },
    {
        id: "paid",
        accessorKey: "paid_amount",
        header: "有效已付",
        meta: {
            label: "有效已付",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.paid_amount} />,
    },
    {
        id: "unpaid",
        accessorKey: "unpaid_amount",
        header: "剩余待付",
        meta: {
            label: "剩余待付",
            width: "amount",
            align: "end",
            numeric: true,
        },
        cell: ({ row }) => <MoneyValue value={row.original.unpaid_amount} />,
    },
    {
        id: "date",
        accessorKey: "desired_payment_date",
        header: "希望付款日",
        meta: { label: "希望付款日", width: "reference", numeric: true },
    },
    {
        id: "status",
        accessorKey: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        cell: ({ row }) => (
            <BusinessStatusBadge
                label={requestStatusLabel[row.original.status]}
                tone={requestStatusTone[row.original.status]}
            />
        ),
    },
]

export function PaymentRequestsPage({
    purchaseOrderId,
    create = false,
}: {
    purchaseOrderId?: string
    create?: boolean
}) {
    const query = usePaymentRequestsQuery()
    const profile = useAccountProfileQuery()
    const [createOpen, setCreateOpen] = useState(create)
    const [search, setSearch] = useState("")
    const [preview, setPreview] = useState<PaymentRequest | null>(null)
    const canCreate =
        hasPermission(
            profile.data?.permissions,
            "purchase_payment_request:create",
        ) &&
        hasPermission(
            profile.data?.permissions,
            "purchase_payment_request:submit",
        )
    const rows = (query.data ?? []).filter((request) =>
        [
            request.request_no,
            request.supplier_name,
            request.our_company_name,
            request.applicant_name,
            ...request.lines.map((line) => line.purchase_order_no),
        ]
            .join(" ")
            .toLowerCase()
            .includes(search.trim().toLowerCase()),
    )
    function closePreview() {
        const id = preview?.id
        setPreview(null)
        if (id)
            requestAnimationFrame(() =>
                document
                    .querySelector<HTMLElement>(
                        `[data-row-id="${CSS.escape(id)}"]`,
                    )
                    ?.focus(),
            )
    }
    return (
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <ListWorkspaceHeader
                eyebrow="采购 · 财务"
                title="采购付款申请"
                description="采购逐单申请，财务审核后由指定出纳付款。申请与实际付款分别记录。"
            >
                <Button
                    id="purchase-payment-requests-refresh"
                    variant="ghost"
                    onClick={() => void query.refetch()}
                >
                    <RefreshCwIcon />
                    刷新
                </Button>
                <Button
                    id="purchase-payment-requests-create"
                    disabled={!canCreate}
                    title={
                        !canCreate
                            ? "当前账号需要采购付款申请的创建和提交权限"
                            : undefined
                    }
                    onClick={() => setCreateOpen(true)}
                >
                    <PlusIcon />
                    新建付款申请
                </Button>
            </ListWorkspaceHeader>
            <ListWorkSurface
                ariaLabel="付款申请列表"
                toolbar={
                    <div className="space-y-2 py-3">
                        <Input
                            id="purchase-payment-requests-search"
                            className="max-w-md"
                            value={search}
                            placeholder="搜索申请单、供应商、采购单或申请人"
                            aria-label="搜索付款申请"
                            onChange={(event) => setSearch(event.target.value)}
                        />
                        <p className="text-xs text-muted-foreground">
                            共 {rows.length} 份申请
                        </p>
                    </div>
                }
                table={
                    <>
                        {query.isError && (
                            <div
                                role="alert"
                                className="py-6 text-sm text-destructive"
                            >
                                {getErrorMessage(
                                    query.error,
                                    "付款申请查询失败，请刷新后重试",
                                )}
                            </div>
                        )}
                        <DataTable
                            idPrefix="purchase-payment-requests-table"
                            data={rows}
                            rowCount={rows.length}
                            columns={columns}
                            getRowId={(row) => row.id}
                            onRowPreview={setPreview}
                            highlightedRowId={preview?.id}
                            loading={query.isPending}
                            caption="采购付款申请"
                        />
                    </>
                }
            />
            <QuickPreviewSheet
                idPrefix="ppr-list-preview"
                open={Boolean(preview)}
                onOpenChange={(open) => {
                    if (!open) closePreview()
                }}
                size="preview"
                identity={
                    <span className="num">申请单号：{preview?.request_no}</span>
                }
                title={preview?.supplier_name ?? "付款申请"}
                description={preview?.our_company_name}
                summary={
                    preview ? (
                        <BusinessStatusBadge
                            context="preview"
                            label={requestStatusLabel[preview.status]}
                            tone={requestStatusTone[preview.status]}
                        />
                    ) : undefined
                }
                footer={
                    <>
                        <Button
                            id="ppr-list-preview-footer-close"
                            variant="outline"
                            onClick={closePreview}
                        >
                            关闭
                        </Button>
                        <Button
                            id="ppr-list-preview-open"
                            render={
                                <Link
                                    href={`/procurement/payment-requests/${encodeURIComponent(preview?.id ?? "")}`}
                                />
                            }
                        >
                            打开付款申请
                            <ArrowUpRightIcon data-icon="inline-end" />
                        </Button>
                    </>
                }
            >
                {preview && (
                    <div className="space-y-5">
                        <div>
                            <p className="text-xs text-muted-foreground">
                                申请总额
                            </p>
                            <MoneyValue
                                value={preview.total_amount}
                                size="hero"
                            />
                        </div>
                        <dl className="space-y-3 text-sm">
                            <div className="flex justify-between">
                                <dt>有效已付</dt>
                                <dd>
                                    <MoneyValue value={preview.paid_amount} />
                                </dd>
                            </div>
                            <div className="flex justify-between">
                                <dt>剩余待付</dt>
                                <dd>
                                    <MoneyValue value={preview.unpaid_amount} />
                                </dd>
                            </div>
                            <div className="flex justify-between">
                                <dt>希望付款日</dt>
                                <dd className="num">
                                    {preview.desired_payment_date}
                                </dd>
                            </div>
                            <div className="flex justify-between">
                                <dt>申请人</dt>
                                <dd>{preview.applicant_name}</dd>
                            </div>
                        </dl>
                        <p className="text-sm text-muted-foreground">
                            {preview.description}
                        </p>
                    </div>
                )}
            </QuickPreviewSheet>
            <RequestDraftDialog
                open={createOpen && canCreate}
                onOpenChange={setCreateOpen}
                purchaseOrderId={purchaseOrderId}
            />
        </PageScaffold>
    )
}
