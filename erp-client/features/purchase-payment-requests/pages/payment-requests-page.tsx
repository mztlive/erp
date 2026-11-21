"use client"

import { useMemo, useRef, useState } from "react"
import { useQueryClient } from "@tanstack/react-query"
import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"
import { ArrowUpRightIcon, PlusIcon, RefreshCwIcon } from "lucide-react"
import {
    BusinessFailureState,
    BusinessStatusBadge,
    MoneyValue,
    PageActions,
    QuickPreviewSheet,
    TableTextCell,
} from "@/components/business"
import {
    ListDataTable,
    ListWorkSurface,
    ListWorkspacePage,
    listWorkspaceQuietAction,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import {
    paymentRequestKeys,
    usePaymentCandidatePageQuery,
} from "../hooks/queries"
import { useRequestList } from "../hooks/use-request-list"
import { emptyRequestFilters, type RequestListUrl } from "../lib/list-query"
import { RequestListFilters } from "../components/request-list-filters"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import type { PaymentRequest } from "../types"
import { PaymentCandidatesList } from "../components/payment-candidates-list"
import {
    buildRequestColumns,
    defaultRequestColumnOrder,
    defaultRequestColumnPinning,
    defaultRequestColumnVisibility,
} from "../components/request-list-columns"
import { RequestDraftDialog } from "../components/request-draft-dialog"

export function PaymentRequestsPage({
    purchaseOrderId,
    create = false,
}: {
    purchaseOrderId?: string
    create?: boolean
}) {
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canList = hasPermission(permissions, "purchase_payment_request:list")
    const canOpenRequest = hasPermission(
        permissions,
        "purchase_payment_request:detail",
    )
    const canOpenPurchaseOrder = hasPermission(
        permissions,
        "purchase_order:detail",
    )
    const canReadCandidates = hasPermission(
        permissions,
        "purchase_payment_request:create",
    )
    const canCreate =
        canReadCandidates &&
        hasPermission(permissions, "purchase_payment_request:submit")
    const canFilterSupplier = hasPermission(permissions, "supplier:list")
    const list = useRequestList(
        canList,
        canReadCandidates,
        canFilterSupplier,
        profile.isSuccess,
    )
    const { query } = list
    const view = list.url.view
    const queryClient = useQueryClient()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const listReturnHref = useMemo(() => {
        const params = new URLSearchParams(searchParams.toString())
        params.delete("returnTo")
        params.delete("create")
        params.delete("purchaseOrderId")
        const query = params.toString()
        return `${pathname}${query ? `?${query}` : ""}`
    }, [pathname, searchParams])
    const columns = useMemo(
        () => buildRequestColumns({ listReturnHref, canOpenPurchaseOrder }),
        [listReturnHref, canOpenPurchaseOrder],
    )
    const [createOpen, setCreateOpen] = useState(create)
    const [initialPayableAccountIds, setInitialPayableAccountIds] = useState<
        string[]
    >([])
    const [previewState, setPreview] = useState<{
        href: string
        row: PaymentRequest
    } | null>(null)
    const preview =
        canList && view !== "available" && previewState?.href === listReturnHref
            ? previewState.row
            : null
    if (previewState && !preview) setPreview(null)
    const lastFocusedRowId = useRef<string | null>(null)
    // 标签显示全部可申请采购单数量，不受当前视图、筛选或分页影响。
    const candidateCountQuery = usePaymentCandidatePageQuery(
        { page: 1, page_size: 1 },
        canReadCandidates,
    )
    const hasFilters =
        Boolean(list.purchaseOrderScope) ||
        Object.values(list.applied).some(Boolean)
    const resultStatus = query.isError
        ? "查询未完成"
        : query.isFetching
          ? "查询中…"
          : null
    const updateList = (patch: Partial<RequestListUrl>) => {
        list.update(patch)
        setPreview(null)
    }
    const clearFilters = () =>
        updateList({
            ...emptyRequestFilters,
            purchase_order_id: undefined,
            page: 1,
        })
    const changeView = (value: RequestListUrl["view"]) =>
        updateList({
            ...emptyRequestFilters,
            purchase_order_id: undefined,
            view: value,
            page: 1,
        })
    const openCreate = () => {
        setInitialPayableAccountIds([])
        setCreateOpen(true)
    }
    const openPreview = (row: PaymentRequest) => {
        lastFocusedRowId.current = row.id
        setPreview({ href: listReturnHref, row })
    }
    const viewItems = (
        [
            ["effective", "有效"],
            ["available", "可申请"],
            ["draft", "草稿"],
        ] as const
    )
        .filter(([value]) =>
            value === "available" ? canReadCandidates : canList,
        )
        .map(([value, label]) => ({
            id: `purchase-payment-requests-view-${value}`,
            label,
            count:
                value === "available" && candidateCountQuery.isSuccess
                    ? candidateCountQuery.data.total
                    : undefined,
            active: view === value,
            onClick: () => changeView(value),
        }))

    return (
        <ListWorkspacePage
            eyebrow=""
            title="采购付款申请"
            loading={!profile.data && profile.isPending}
            views={{ ariaLabel: "付款申请分类", items: viewItems }}
            headerActions={
                canList || canReadCandidates ? (
                    <PageActions
                        id="purchase-payment-requests-actions"
                        size="sm"
                        className="gap-1.5"
                        actions={[
                            listWorkspaceQuietAction({
                                id: "purchase-payment-requests-refresh",
                                actionKey: "refresh",
                                label: "刷新",
                                icon: RefreshCwIcon,
                                onClick: () =>
                                    void queryClient.invalidateQueries({
                                        queryKey: paymentRequestKeys.all,
                                    }),
                            }),
                            {
                                id: "purchase-payment-requests-create",
                                actionKey: "create",
                                label: "新建付款申请",
                                icon: PlusIcon,
                                hidden: !canCreate,
                                onClick: openCreate,
                            },
                        ]}
                    />
                ) : undefined
            }
        >
            {view === "available" ? (
                <PaymentCandidatesList
                    canRead={canReadCandidates}
                    canCreate={canCreate}
                    onApply={(ids) => {
                        setInitialPayableAccountIds(ids)
                        setCreateOpen(true)
                    }}
                />
            ) : !canList ? (
                <div className="py-3">
                    {profile.isError && !profile.data ? (
                        <BusinessFailureState
                            idPrefix="purchase-payment-requests-profile"
                            error={profile.error}
                            onRetry={() => void profile.refetch()}
                        />
                    ) : (
                        <BusinessFailureState
                            kind="permission"
                            title="当前账号无权查看采购付款申请"
                            action={
                                canReadCandidates ? (
                                    <Button
                                        id="purchase-payment-requests-view-available"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        onClick={() => changeView("available")}
                                    >
                                        查看可申请采购单
                                    </Button>
                                ) : undefined
                            }
                        />
                    )}
                </div>
            ) : (
                <ListWorkSurface
                    key={view}
                    ariaLabel={
                        view === "draft"
                            ? "草稿付款申请列表"
                            : "有效付款申请列表"
                    }
                    toolbar={
                        <RequestListFilters
                            key={JSON.stringify([view, list.applied])}
                            applied={list.applied}
                            canFilterSupplier={canFilterSupplier}
                            resultStatus={resultStatus}
                            scopeChip={
                                list.purchaseOrderScope
                                    ? {
                                          label: list.purchaseOrderScope.number
                                              ? `采购单：${list.purchaseOrderScope.number}`
                                              : "仅显示当前采购单",
                                          onClear: () =>
                                              updateList({
                                                  purchase_order_id: undefined,
                                                  page: 1,
                                              }),
                                      }
                                    : undefined
                            }
                            onReset={clearFilters}
                            onApply={(filters, clearScope) =>
                                updateList({
                                    ...filters,
                                    page: 1,
                                    ...(clearScope && {
                                        purchase_order_id: undefined,
                                    }),
                                })
                            }
                        />
                    }
                    table={
                        <ListDataTable
                            idPrefix="purchase-payment-requests-table"
                            entityName="付款申请"
                            data={query.data?.items ?? []}
                            rowCount={query.data?.total ?? 0}
                            columns={columns}
                            defaultColumnVisibility={
                                defaultRequestColumnVisibility
                            }
                            defaultColumnOrder={defaultRequestColumnOrder}
                            defaultColumnPinning={defaultRequestColumnPinning}
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
                            onPaginationChange={(next) =>
                                updateList({
                                    page:
                                        next.pageSize === list.url.page_size
                                            ? next.pageIndex + 1
                                            : 1,
                                    page_size: next.pageSize,
                                })
                            }
                            getRowId={(row) => row.id}
                            rowLabel={(row) =>
                                `${row.supplier_name}（${row.request_no}）`
                            }
                            onRowPreview={openPreview}
                            onRowOpen={openPreview}
                            highlightedRowId={preview?.id}
                            loading={query.isPending || query.isFetching}
                            isError={query.isError}
                            error={query.error}
                            onRetry={() => void query.refetch()}
                            hasActiveFilters={hasFilters}
                            onClearFilters={clearFilters}
                            canCreate={canCreate}
                            onCreate={openCreate}
                            createLabel="新建付款申请"
                            emptyCopy={{
                                emptyTitle:
                                    view === "draft"
                                        ? "暂无草稿"
                                        : "暂无有效付款申请",
                                emptyDescription:
                                    view === "draft"
                                        ? "未提交、已驳回和已撤回的申请会显示在这里。"
                                        : "提交后的付款申请会显示在这里，包括已关闭的申请。",
                            }}
                            caption={
                                view === "draft"
                                    ? "未提交、已驳回和已撤回的采购付款申请"
                                    : "已提交的采购付款申请，包括已关闭的申请"
                            }
                        />
                    }
                />
            )}
            <QuickPreviewSheet
                idPrefix="ppr-list-preview"
                open={Boolean(preview)}
                onOpenChange={(open) => {
                    if (!open) setPreview(null)
                }}
                finalFocus={() => {
                    const id = lastFocusedRowId.current
                    return id
                        ? (document
                              .getElementById("purchase-payment-requests-table")
                              ?.querySelector<HTMLElement>(
                                  `[data-row-id="${CSS.escape(id)}"]`,
                              ) ?? true)
                        : true
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
                            onClick={() => setPreview(null)}
                        >
                            关闭
                        </Button>
                        {canOpenRequest && preview && (
                            <Button
                                id="ppr-list-preview-open"
                                render={
                                    <Link
                                        href={`/procurement/payment-requests/${encodeURIComponent(preview.id)}?returnTo=${encodeURIComponent(listReturnHref)}`}
                                    />
                                }
                            >
                                打开付款申请
                                <ArrowUpRightIcon data-icon="inline-end" />
                            </Button>
                        )}
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
                        <dl className="space-y-3 text-body-compact">
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
                        {preview.lines.length > 0 && (
                            <div className="space-y-2">
                                <p className="text-xs text-muted-foreground">
                                    关联采购单
                                </p>
                                <ul className="space-y-2 text-body-compact">
                                    {preview.lines.map((line) => (
                                        <li
                                            key={line.payable_account_id}
                                            className="flex items-center justify-between gap-3"
                                        >
                                            {canOpenPurchaseOrder ? (
                                                <Link
                                                    id={`ppr-list-preview-purchase-order-${toAutomationIdSegment(line.payable_account_id)}`}
                                                    href={`/procurement/orders/${encodeURIComponent(line.purchase_order_id)}?${new URLSearchParams(
                                                        {
                                                            section: "payable",
                                                            returnTo: `/procurement/payment-requests/${encodeURIComponent(preview.id)}`,
                                                        },
                                                    )}`}
                                                    className="num min-w-0 truncate text-primary underline-offset-2 hover:underline"
                                                    title={
                                                        line.purchase_order_no
                                                    }
                                                >
                                                    {line.purchase_order_no}
                                                </Link>
                                            ) : (
                                                <TableTextCell
                                                    value={
                                                        line.purchase_order_no
                                                    }
                                                    numeric
                                                    clamp={1}
                                                />
                                            )}
                                            <MoneyValue
                                                value={line.requested_amount}
                                            />
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        )}
                        <p className="text-xs text-muted-foreground">
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
        </ListWorkspacePage>
    )
}
