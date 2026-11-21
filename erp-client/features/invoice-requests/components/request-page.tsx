"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import type { ColumnDef } from "@tanstack/react-table"
import { ChevronLeftIcon, ChevronRightIcon, PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceHeader,
    ListWorkspacePage,
    ListWorkspaceViews,
    MoneyValue,
    OptionCombobox,
    PageScaffold,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    listWorkspaceStyles,
    type ListWorkspaceFilterChip,
    type ListWorkspaceViewsConfig,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import { getErrorCode } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { restoreListRowFocus } from "@/lib/list-row-focus"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { listFilterText } from "@/lib/ui-text"
import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

import {
    requestStatusLabels,
    type InvoiceRequest,
    type RequestQuery,
    type RequestStatus,
} from "../api"
import {
    requestKeys,
    useInvoiceRequestPermissions,
    useInvoiceRequests,
} from "../hooks/queries"
import {
    dismissPendingInvoiceRequestDetail,
    pendingInvoiceRequestDetailId,
} from "../pending-detail"
import { InvoiceRequestDetail } from "./request-detail"
import { InvoiceRequestForm } from "./request-form"

const PAGE_SIZE = 10
const CREATE_LABEL = "新建开票申请"
const EMPTY_ROWS: InvoiceRequest[] = []
const FIRST_PAGE = { number: 1 }
const STATUS_TONES: Record<RequestStatus, StatusTone> = {
    draft: "neutral",
    rejected: "destructive",
    in_approval: "warning",
    approved: "info",
    completed: "success",
}

const STATUS_OPTIONS = (
    Object.entries(requestStatusLabels) as [RequestStatus, string][]
).map(([value, label]) => ({ value, label }))

function isRequestStatus(value: string): value is RequestStatus {
    return Object.hasOwn(requestStatusLabels, value)
}

/** 客户往来开票申请页；草稿不请求，查询后写入 URL 并回到第一页。 */
export function InvoiceRequestsPage() {
    const params = useSearchParams()
    const router = useRouter()
    const pathname = usePathname()
    const appliedQuery = params.toString()
    const q = params.get("q") ?? ""
    const customerId = params.get("customerId") ?? ""
    const salesOrderId = params.get("salesOrderId") ?? ""
    const statusParam = params.get("status") ?? ""
    const appliedStatus = isRequestStatus(statusParam) ? statusParam : ""
    const { searchDraft, setSearchDraft } = useSearchDraft(q)
    const [customerDraft, setCustomerDraft] = React.useState(customerId)
    const [orderDraft, setOrderDraft] = React.useState(salesOrderId)
    const [statusDraft, setStatusDraft] = React.useState(appliedStatus)
    const [panelOpen, setPanelOpen] = React.useState(false)
    const query: RequestQuery = {
        q: q.trim() || undefined,
        customer_id: customerId || undefined,
        sales_order_id: salesOrderId || undefined,
        status: appliedStatus || undefined,
    }

    React.useEffect(() => {
        setCustomerDraft(customerId)
        setOrderDraft(salesOrderId)
        setStatusDraft(appliedStatus)
    }, [appliedStatus, customerId, salesOrderId])

    const replaceFilters = React.useCallback(
        (nextValues: {
            q: string
            customerId: string
            salesOrderId: string
            status: string
        }) => {
            const next = new URLSearchParams(appliedQuery)
            const write = (key: string, value: string) => {
                const trimmed = value.trim()
                if (trimmed) next.set(key, trimmed)
                else next.delete(key)
            }
            write("q", nextValues.q)
            write("customerId", nextValues.customerId)
            write("salesOrderId", nextValues.salesOrderId)
            write(
                "status",
                isRequestStatus(nextValues.status) ? nextValues.status : "",
            )
            const queryString = next.toString()
            router.replace(
                queryString ? `${pathname}?${queryString}` : pathname,
                { scroll: false },
            )
            setPanelOpen(false)
        },
        [appliedQuery, pathname, router],
    )

    const applyFilters = React.useCallback(() => {
        replaceFilters({
            q: searchDraft,
            customerId: customerDraft,
            salesOrderId: orderDraft,
            status: statusDraft,
        })
    }, [customerDraft, orderDraft, replaceFilters, searchDraft, statusDraft])

    const resetMoreFilters = React.useCallback(() => {
        setOrderDraft("")
        setStatusDraft("")
    }, [])

    const cancelMoreFilters = React.useCallback(() => {
        setOrderDraft(salesOrderId)
        setStatusDraft(appliedStatus)
        setPanelOpen(false)
    }, [appliedStatus, salesOrderId])

    const removeFilter = React.useCallback(
        (key: "q" | "customerId" | "salesOrderId" | "status") => {
            if (key === "q") setSearchDraft("")
            if (key === "customerId") setCustomerDraft("")
            if (key === "salesOrderId") setOrderDraft("")
            if (key === "status") setStatusDraft("")
            replaceFilters({
                q: key === "q" ? "" : q,
                customerId: key === "customerId" ? "" : customerId,
                salesOrderId: key === "salesOrderId" ? "" : salesOrderId,
                status: key === "status" ? "" : appliedStatus,
            })
        },
        [
            appliedStatus,
            customerId,
            q,
            replaceFilters,
            salesOrderId,
            setSearchDraft,
        ],
    )

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setCustomerDraft("")
        setOrderDraft("")
        setStatusDraft("")
        replaceFilters({
            q: "",
            customerId: "",
            salesOrderId: "",
            status: "",
        })
    }, [replaceFilters, setSearchDraft])

    const searchChanged = searchDraft.trim() !== q.trim()
    const customerChanged = customerDraft !== customerId
    const chips: ListWorkspaceFilterChip[] = []
    const moreCount = (salesOrderId ? 1 : 0) + (appliedStatus ? 1 : 0)
    // 主筛选草稿与生效值不同时，保留生效条件的移除入口。
    if (q.trim() && searchChanged) {
        chips.push({
            key: "q",
            label: `搜索：${q.trim()}`,
            onClear: () => removeFilter("q"),
        })
    }
    if (customerId && customerChanged) {
        chips.push({
            key: "customerId",
            label: "已选客户",
            onClear: () => removeFilter("customerId"),
        })
    }
    if (salesOrderId) {
        chips.push({
            key: "salesOrderId",
            label: "已选销售单",
            onClear: () => removeFilter("salesOrderId"),
        })
    }
    if (appliedStatus) {
        chips.push({
            key: "status",
            label: `状态：${requestStatusLabels[appliedStatus]}`,
            onClear: () => removeFilter("status"),
        })
    }
    const hasActiveFilters = Boolean(
        q.trim() || customerId || salesOrderId || appliedStatus,
    )
    const hasPendingChanges =
        searchChanged ||
        customerChanged ||
        orderDraft !== salesOrderId ||
        statusDraft !== appliedStatus
    const views: ListWorkspaceViewsConfig = {
        ariaLabel: "客户往来工作视图",
        items: [
            ...[
                ["receivable", "应收"],
                ["receipt", "回款"],
                ["sales_invoice", "销项发票"],
                ["unallocated", "待分配"],
            ].map(([view, label]) => ({
                id: `invoice-request-nav-${view}`,
                label,
                href: `/finance/customer-accounts?view=${view}`,
                active: false,
            })),
            {
                id: "invoice-request-nav-requests",
                label: "开票申请",
                href: appliedQuery ? `${pathname}?${appliedQuery}` : pathname,
                active: true,
            },
        ],
    }

    return (
        <InvoiceRequestListWorkspace
            key={appliedQuery}
            query={query}
            views={views}
            initialRequestId={params.get("requestId") ?? undefined}
            initialCreate={params.get("create") === "1"}
            hasActiveFilters={hasActiveFilters}
            onClearFilters={clearFilters}
            renderToolbar={(resultStatus) => (
                <ListWorkspaceFilterBar
                    morePresentation="popover"
                    idPrefix="invoice-request-filter"
                    formAriaLabel="开票申请查询"
                    onSubmit={applyFilters}
                    queryButtonId="invoice-request-filter-apply"
                    resetMoreButtonId="invoice-request-filter-reset"
                    search={
                        <InvoiceRequestSearch
                            value={searchDraft}
                            onChange={setSearchDraft}
                        />
                    }
                    moreCount={moreCount}
                    moreOpen={panelOpen}
                    onToggleMore={() =>
                        panelOpen ? cancelMoreFilters() : setPanelOpen(true)
                    }
                    morePanelId="invoice-request-filter-more-panel"
                    morePanelAriaLabel="开票申请更多筛选条件"
                    onResetMore={resetMoreFilters}
                    primaryFilters={
                        <CustomerSearchCombobox
                            id="invoice-request-filter-customer"
                            className="w-56 max-w-full min-w-0"
                            filterLabel="客户"
                            purpose="filter"
                            scope="all_authorized"
                            value={customerDraft || undefined}
                            onValueChange={(id) => setCustomerDraft(id ?? "")}
                            placeholder="全部"
                        />
                    }
                    morePanel={
                        <ListFilterSections>
                            <ListFilterSection legend="申请条件">
                                <ListWorkspaceFilterField
                                    htmlFor="invoice-request-filter-order"
                                    label="销售单"
                                >
                                    <SalesOrderSearchCombobox
                                        id="invoice-request-filter-order"
                                        className="w-full min-w-0"
                                        value={orderDraft || undefined}
                                        onValueChange={(id) =>
                                            setOrderDraft(id ?? "")
                                        }
                                        placeholder="全部销售单"
                                    />
                                </ListWorkspaceFilterField>
                                <ListWorkspaceFilterField
                                    htmlFor="invoice-request-filter-status"
                                    label="状态"
                                >
                                    <OptionCombobox
                                        id="invoice-request-filter-status"
                                        className="w-full min-w-0"
                                        aria-label="状态"
                                        value={statusDraft || null}
                                        onValueChange={(value) =>
                                            setStatusDraft(
                                                value && isRequestStatus(value)
                                                    ? value
                                                    : "",
                                            )
                                        }
                                        options={STATUS_OPTIONS}
                                        placeholder="全部状态"
                                    />
                                </ListWorkspaceFilterField>
                            </ListFilterSection>
                        </ListFilterSections>
                    }
                    resultStatus={resultStatus}
                    chips={chips}
                    onClearAll={clearFilters}
                    extraPrimary={
                        hasActiveFilters && chips.length === 0 ? (
                            <Button
                                id="invoice-request-filter-clear"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={listWorkspaceStyles.quietButton}
                                onClick={clearFilters}
                            >
                                {listFilterText.clear}
                            </Button>
                        ) : undefined
                    }
                    hasPendingChanges={hasPendingChanges}
                />
            )}
        />
    )
}

function InvoiceRequestSearch({
    value,
    onChange,
}: {
    value: string
    onChange: (value: string) => void
}) {
    const inputRef = React.useRef<HTMLInputElement>(null)
    useSearchHotkey(inputRef)

    return (
        <ListSearchField
            id="invoice-request-search"
            searchInputRef={inputRef}
            value={value}
            onChange={onChange}
            placeholder="申请单号、抬头、事由"
            aria-label="搜索开票申请"
        />
    )
}

function invoiceRequestColumns(
    onOpen: (request: InvoiceRequest) => void,
): ColumnDef<InvoiceRequest>[] {
    return [
        {
            id: "request",
            header: "申请单号 / 开票抬头",
            meta: { width: "flex" },
            cell: ({ row: { original } }) => (
                <TableIdentityCell
                    className="w-0 min-w-full"
                    primary={
                        <TableTextCell value={original.request_no} numeric />
                    }
                    primaryTitle={original.request_no}
                    secondary={
                        original.data?.invoice_title?.trim() || (
                            <TableEmptyCell />
                        )
                    }
                    secondaryTitle={original.data?.invoice_title}
                />
            ),
        },
        {
            id: "salesOrder",
            header: "销售单",
            meta: { width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.sales_order_no}
                    clamp={1}
                    numeric
                />
            ),
        },
        {
            id: "amount",
            header: "申请金额",
            meta: { width: "amount", align: "end", numeric: true },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.data?.amount || "0.00"}
                    display="compact"
                />
            ),
        },
        {
            id: "invoiced",
            header: "已开票",
            meta: { width: "amount", align: "end", numeric: true },
            cell: ({ row }) => (
                <MoneyValue
                    value={row.original.invoiced_amount || "0.00"}
                    display="compact"
                />
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { width: "status" },
            cell: ({
                row: {
                    original: { status },
                },
            }) => (
                <BusinessStatusBadge
                    context="list"
                    label={requestStatusLabels[status]}
                    tone={STATUS_TONES[status]}
                    appearance={status === "completed" ? "quiet" : "badge"}
                />
            ),
        },
        {
            id: "applicant",
            header: "申请人",
            meta: { width: "default" },
            cell: ({ row }) => (
                <TableTextCell value={row.original.created_by_name} clamp={1} />
            ),
        },
        {
            id: "created",
            header: "申请时间",
            meta: { width: "default", numeric: true },
            cell: ({ row }) => (
                <TableTextCell
                    value={new Date(
                        row.original.created_at * 1000,
                    ).toLocaleDateString("zh-CN")}
                />
            ),
        },
        {
            id: "actions",
            header: "操作",
            meta: { width: "status", align: "end" },
            cell: ({ row: { original } }) => {
                const suffix = toAutomationIdSegment(original.id)
                return (
                    <TableRowActions
                        moreId={`invoice-request-row-${suffix}-more`}
                        moreLabel={`${original.request_no} 更多操作`}
                        actions={[
                            {
                                id: `invoice-request-view-${suffix}`,
                                label: "查看",
                                onClick: () => onOpen(original),
                            },
                        ]}
                    />
                )
            },
        },
    ]
}

function InvoiceRequestListWorkspace({
    query: filters,
    views,
    initialRequestId,
    initialCreate,
    hasActiveFilters,
    onClearFilters,
    renderToolbar,
}: {
    query: RequestQuery
    views: ListWorkspaceViewsConfig
    initialRequestId?: string
    initialCreate: boolean
    hasActiveFilters: boolean
    onClearFilters: () => void
    renderToolbar: (status: string | null) => React.ReactNode
}) {
    const permissions = useInvoiceRequestPermissions()
    const queryClient = useQueryClient()
    const [page, setPage] = React.useState<{
        number: number
        scopeVersion?: string
    }>(FIRST_PAGE)
    const [detailId, setDetailId] = React.useState(
        () => initialRequestId || pendingInvoiceRequestDetailId(),
    )
    const [creating, setCreating] = React.useState(
        () => initialCreate && !pendingInvoiceRequestDetailId(),
    )
    const [editing, setEditing] = React.useState<InvoiceRequest>()
    const lastFocusedRowId = React.useRef<string | undefined>(undefined)
    const listQuery: RequestQuery & { scope_version?: string } = {
        ...filters,
        page: page.number,
        page_size: PAGE_SIZE,
        scope_version: page.scopeVersion,
    }
    const list = useInvoiceRequests(listQuery, permissions.canRead)
    const data = list.data
    const scopeVersion =
        data &&
        "scope_version" in data &&
        typeof data.scope_version === "string"
            ? data.scope_version
            : undefined
    const noScope = Boolean(
        data && "empty_reason" in data && data.empty_reason === "no_scope",
    )
    const showForm = (creating || Boolean(editing)) && permissions.canSubmit
    const showList = permissions.canRead && !showForm && !detailId

    React.useEffect(() => {
        if (initialRequestId) return
        const pending = pendingInvoiceRequestDetailId()
        if (!pending) return
        setDetailId(pending)
        setCreating(false)
        setEditing(undefined)
    }, [initialRequestId])

    React.useEffect(() => {
        if (showList && data && lastFocusedRowId.current) {
            restoreListRowFocus(lastFocusedRowId.current)
            lastFocusedRowId.current = undefined
        }
    }, [showList, data])

    const onCreate = () => {
        dismissPendingInvoiceRequestDetail()
        setCreating(true)
    }
    const onOpen = React.useCallback((request: InvoiceRequest) => {
        lastFocusedRowId.current = request.id
        setDetailId(request.id)
    }, [])
    const columns = React.useMemo(() => invoiceRequestColumns(onOpen), [onOpen])

    if (!showList) {
        return (
            <PageScaffold density="compact" className="gap-6">
                <ListWorkspaceHeader
                    eyebrow="财务"
                    title="客户往来"
                    description="管理客户开票申请，跟踪审批与财务开票进度。"
                />
                <ListWorkspaceViews {...views} />
                {!permissions.canRead ? (
                    <BusinessFailureState
                        kind="permission"
                        title="当前账号没有查看开票申请的权限。"
                    />
                ) : showForm ? (
                    <section className="flex flex-col gap-4">
                        <h2 className="text-section-title font-semibold tracking-tight">
                            {editing ? "修改开票申请" : "申请开票"}
                        </h2>
                        <InvoiceRequestForm
                            key={editing?.id ?? "new"}
                            existing={editing}
                            onCancel={() => {
                                setCreating(false)
                                setEditing(undefined)
                            }}
                            onDone={(request) => {
                                const id =
                                    request.id ||
                                    pendingInvoiceRequestDetailId()
                                setCreating(false)
                                setEditing(undefined)
                                if (id) setDetailId(id)
                            }}
                        />
                    </section>
                ) : detailId ? (
                    <InvoiceRequestDetail
                        key={detailId}
                        id={detailId}
                        onBack={() => {
                            dismissPendingInvoiceRequestDetail()
                            setDetailId(undefined)
                        }}
                        onEdit={setEditing}
                    />
                ) : null}
            </PageScaffold>
        )
    }

    const total = data?.total ?? 0
    const listFailed = list.isError || (!list.isPending && !data)
    const changePage = (number: number) => {
        if (number === 1) setPage(FIRST_PAGE)
        else setPage({ number, scopeVersion })
    }

    return (
        <ListWorkspacePage
            title="开票申请"
            className="min-h-0"
            views={views}
            actions={
                permissions.canSubmit
                    ? [
                          {
                              id: "invoice-request-create",
                              actionKey: "create",
                              label: CREATE_LABEL,
                              icon: PlusIcon,
                              tier: "primary",
                              onClick: onCreate,
                          },
                      ]
                    : undefined
            }
            loading={page.number === 1 && list.isPending}
            surface={{
                ariaLabel: "开票申请",
                toolbar: renderToolbar(
                    listFailed
                        ? "查询未完成"
                        : list.isFetching
                          ? "查询中…"
                          : null,
                ),
                className: "@container/invoice-requests min-h-64",
                tableClassName:
                    "flex flex-col @max-lg/invoice-requests:[&_[data-column-pinned]]:inset-x-auto! @max-lg/invoice-requests:[&_[data-pinned-edge]]:shadow-none!",
                table: (
                    <>
                        <ListDataTable
                            id="invoice-request-table"
                            entityName="开票申请"
                            caption="开票申请"
                            data={data?.items ?? EMPTY_ROWS}
                            columns={columns}
                            defaultColumnPinning={{
                                left: ["request"],
                                right: ["actions"],
                            }}
                            getRowId={(request) => request.id}
                            rowLabel={(request) => request.request_no}
                            rowCount={total}
                            pagination={{
                                pageIndex: page.number - 1,
                                pageSize: PAGE_SIZE,
                            }}
                            onPaginationChange={({ pageIndex }) =>
                                changePage(pageIndex + 1)
                            }
                            showPagination={false}
                            loading={list.isFetching}
                            isError={listFailed}
                            error={list.error}
                            retryButtonId="invoice-request-list-retry"
                            onRetry={() => {
                                if (
                                    page.number > 1 &&
                                    getErrorCode(list.error) ===
                                        "DATA_SCOPE_CHANGED"
                                ) {
                                    void queryClient.invalidateQueries({
                                        queryKey: [
                                            ...requestKeys.all,
                                            "list",
                                            {
                                                ...filters,
                                                page: 1,
                                                page_size: PAGE_SIZE,
                                            },
                                        ],
                                        exact: true,
                                        refetchType: "none",
                                    })
                                    setPage(FIRST_PAGE)
                                } else {
                                    void list.refetch()
                                }
                            }}
                            noScope={noScope}
                            hasActiveFilters={hasActiveFilters}
                            onClearFilters={onClearFilters}
                            canCreate={permissions.canSubmit}
                            onCreate={onCreate}
                            createLabel={CREATE_LABEL}
                            emptyCopy={{
                                emptyTitle: "暂无开票申请",
                                emptyDescription: permissions.canSubmit
                                    ? "有开票需求时，可在此发起申请。"
                                    : undefined,
                            }}
                            onRowOpen={onOpen}
                            scrollable
                        />
                        {data && !listFailed && (
                            <InvoiceRequestPagination
                                page={page.number}
                                total={total}
                                onPageChange={changePage}
                            />
                        )}
                    </>
                ),
            }}
        />
    )
}

// 保留固定页大小与原有翻页按钮 ID。
function InvoiceRequestPagination({
    page,
    total,
    onPageChange,
}: {
    page: number
    total: number
    onPageChange: (page: number) => void
}) {
    const pageCount = Math.ceil(total / PAGE_SIZE)

    return (
        <div
            data-slot="data-table-pagination"
            className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 text-muted-foreground"
        >
            <span className="num">共 {total.toLocaleString("zh-CN")} 条</span>
            {pageCount > 1 && (
                <div
                    className="flex items-center gap-1.5"
                    role="group"
                    aria-label="翻页"
                >
                    <span className="num">
                        第 {page} / {pageCount} 页
                    </span>
                    <Button
                        id="invoice-request-page-prev"
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="上一页"
                        disabled={page <= 1}
                        onClick={() => onPageChange(page - 1)}
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                    </Button>
                    <Button
                        id="invoice-request-page-next"
                        type="button"
                        variant="outline"
                        size="icon-sm"
                        aria-label="下一页"
                        disabled={page >= pageCount}
                        onClick={() => onPageChange(page + 1)}
                    >
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </div>
            )}
        </div>
    )
}
