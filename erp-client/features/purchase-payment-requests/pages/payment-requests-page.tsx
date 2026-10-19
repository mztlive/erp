"use client"

import { useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import Link from "next/link"
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
    ListWorkspaceViews,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api"
import { hasPermission } from "@/lib/permissions"
import {
    paymentRequestKeys,
    usePaymentCandidatePageQuery,
} from "../hooks/queries"
import { useRequestList } from "../hooks/use-request-list"
import { emptyRequestFilters } from "../lib/list-query"
import { RequestListFilters } from "../components/request-list-filters"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import type { PaymentRequest } from "../types"
import { PaymentCandidatesList } from "../components/payment-candidates-list"
import { requestColumns } from "../components/request-list-columns"
import { RequestDraftDialog } from "../components/request-draft-dialog"

export function PaymentRequestsPage({
    purchaseOrderId,
    create = false,
}: {
    purchaseOrderId?: string
    create?: boolean
}) {
    const list = useRequestList()
    const { query } = list
    const view = list.url.view
    const queryClient = useQueryClient()
    const profile = useAccountProfileQuery()
    const [createOpen, setCreateOpen] = useState(create)
    const [initialPayableAccountIds, setInitialPayableAccountIds] = useState<
        string[]
    >([])
    const [preview, setPreview] = useState<PaymentRequest | null>(null)
    const canReadCandidates = hasPermission(
        profile.data?.permissions,
        "purchase_payment_request:create",
    )
    const canCreate =
        canReadCandidates &&
        hasPermission(
            profile.data?.permissions,
            "purchase_payment_request:submit",
        )
    // 标签显示全部可申请采购单数量，不受当前视图、筛选或分页影响。
    const candidateCountQuery = usePaymentCandidatePageQuery(
        { page: 1, page_size: 1 },
        canReadCandidates,
    )
    const rows = query.data?.items ?? []
    const views = (
        <ListWorkspaceViews
            ariaLabel="付款申请分类"
            items={(
                [
                    ["effective", "有效"],
                    ["available", "可申请"],
                    ["draft", "草稿"],
                ] as const
            ).map(([value, label]) => ({
                id: `purchase-payment-requests-view-${value}`,
                label,
                count:
                    value === "available" &&
                    canReadCandidates &&
                    candidateCountQuery.isSuccess
                        ? candidateCountQuery.data.total
                        : undefined,
                active: view === value,
                onClick: () => {
                    list.update({
                        ...emptyRequestFilters,
                        view: value,
                        page: 1,
                    })
                    setPreview(null)
                },
            }))}
        />
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
                    onClick={() =>
                        void queryClient.invalidateQueries({
                            queryKey: paymentRequestKeys.all,
                        })
                    }
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
                    onClick={() => {
                        setInitialPayableAccountIds([])
                        setCreateOpen(true)
                    }}
                >
                    <PlusIcon />
                    新建付款申请
                </Button>
            </ListWorkspaceHeader>
            {view === "available" ? (
                <PaymentCandidatesList
                    views={views}
                    canRead={canReadCandidates}
                    canCreate={canCreate}
                    onApply={(ids) => {
                        setInitialPayableAccountIds(ids)
                        setCreateOpen(true)
                    }}
                />
            ) : (
                <ListWorkSurface
                    key={view}
                    views={views}
                    ariaLabel="付款申请列表"
                    toolbar={
                        <RequestListFilters
                            key={JSON.stringify([view, list.applied])}
                            applied={list.applied}
                            view={view}
                            onApply={(filters) => {
                                list.update({ ...filters, page: 1 })
                                setPreview(null)
                            }}
                        />
                    }
                    table={
                        <>
                            <DataTable
                                idPrefix="purchase-payment-requests-table"
                                data={rows}
                                rowCount={query.data?.total ?? 0}
                                columns={requestColumns}
                                manualPagination
                                manualSorting
                                sorting={list.sorting}
                                onSortingChange={(next) => {
                                    list.sort(next)
                                    setPreview(null)
                                }}
                                pagination={{
                                    pageIndex: list.url.page - 1,
                                    pageSize: list.url.page_size,
                                }}
                                pageSizeOptions={[20, 50, 100]}
                                onPaginationChange={(next) => {
                                    list.update({
                                        page:
                                            next.pageSize === list.url.page_size
                                                ? next.pageIndex + 1
                                                : 1,
                                        page_size: next.pageSize,
                                    })
                                    setPreview(null)
                                }}
                                getRowId={(row) => row.id}
                                onRowPreview={setPreview}
                                highlightedRowId={preview?.id}
                                loading={query.isFetching}
                                errorState={
                                    query.isError ? (
                                        <p
                                            role="alert"
                                            className="py-6 text-sm text-destructive"
                                        >
                                            {getErrorMessage(
                                                query.error,
                                                "付款申请查询失败，请刷新后重试",
                                            )}
                                        </p>
                                    ) : undefined
                                }
                                caption="采购付款申请"
                            />
                        </>
                    }
                />
            )}
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
                purchaseOrderId={
                    initialPayableAccountIds.length
                        ? undefined
                        : purchaseOrderId
                }
                initialPayableAccountIds={initialPayableAccountIds}
            />
        </PageScaffold>
    )
}
