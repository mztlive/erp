"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { BarChart3Icon, PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    ListDataTable,
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspacePage,
    MoneyValue,
    OptionCombobox,
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
import { subtractFixed } from "@/lib/fixed-decimal"
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

const DEFAULT_PAGE_SIZE = 20
const CREATE_LABEL = "新建开票申请"
const EMPTY_ROWS: InvoiceRequest[] = []
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

function safeRemaining(amount?: string, invoiced?: string): string {
    const a = amount || "0.00"
    const b = invoiced || "0.00"
    if (!/^-?\d+(?:\.\d{1,2})?$/.test(a) || !/^-?\d+(?:\.\d{1,2})?$/.test(b)) {
        return a
    }
    return subtractFixed(a, b, { maxScale: 2, outputScale: 2 })
}

/** 客户往来开票申请页；与客户往来 5 视图统一对齐，详情与表单进入独立 DetailWorkspacePage。 */
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
    const [selectedNames, setSelectedNames] = React.useState<{
        customer?: string
        order?: string
    }>({})
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
        setSelectedNames({})
        replaceFilters({
            q: "",
            customerId: "",
            salesOrderId: "",
            status: "",
        })
    }, [replaceFilters, setSearchDraft])

    const searchChanged = searchDraft.trim() !== q.trim()
    const customerChanged = customerDraft !== customerId
    const moreCount = (salesOrderId ? 1 : 0) + (appliedStatus ? 1 : 0)
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
            {
                id: "invoice-request-nav-receivable",
                label: "应收",
                href: "/finance/customer-accounts?view=receivable",
                active: false,
            },
            {
                id: "invoice-request-nav-receipt",
                label: "回款",
                href: "/finance/customer-accounts?view=receipt",
                active: false,
            },
            {
                id: "invoice-request-nav-sales_invoice",
                label: "销项发票",
                href: "/finance/customer-accounts?view=sales_invoice",
                active: false,
            },
            {
                id: "invoice-request-nav-requests",
                label: "开票申请",
                href: appliedQuery ? `${pathname}?${appliedQuery}` : pathname,
                active: true,
            },
            {
                id: "invoice-request-nav-unallocated",
                label: "待分配",
                href: "/finance/customer-accounts?view=unallocated",
                active: false,
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
            onOpenScope={() =>
                router.push("/finance/customer-accounts/invoice-requests/scope")
            }
            renderToolbar={(resultStatus, items) => {
                const resolvedOrderNo =
                    selectedNames.order ??
                    items?.find((item) => item.sales_order_id === salesOrderId)
                        ?.sales_order_no
                const resolvedCustomer =
                    selectedNames.customer ??
                    items?.find((item) => item.customer_id === customerId)?.data
                        ?.invoice_title
                const chips: ListWorkspaceFilterChip[] = []
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
                        label: resolvedCustomer
                            ? `客户：${resolvedCustomer}`
                            : "已选客户",
                        onClear: () => removeFilter("customerId"),
                    })
                }
                if (salesOrderId) {
                    chips.push({
                        key: "salesOrderId",
                        label: resolvedOrderNo
                            ? `销售单：${resolvedOrderNo}`
                            : "已选销售单",
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
                return (
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
                                onValueChange={(id) =>
                                    setCustomerDraft(id ?? "")
                                }
                                onItemChange={(item) =>
                                    setSelectedNames((prev) => ({
                                        ...prev,
                                        customer:
                                            item?.shortName || item?.legalName,
                                    }))
                                }
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
                                            onItemChange={(item) =>
                                                setSelectedNames((prev) => ({
                                                    ...prev,
                                                    order: item?.documentNumber,
                                                }))
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
                                                    value &&
                                                        isRequestStatus(value)
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
                )
            }}
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
                    display="exact"
                    className="font-semibold"
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
                    display="exact"
                />
            ),
        },
        {
            id: "remaining",
            header: "尚未开票",
            meta: { width: "amount", align: "end", numeric: true },
            cell: ({ row }) => (
                <MoneyValue
                    value={safeRemaining(
                        row.original.data?.amount,
                        row.original.invoiced_amount,
                    )}
                    display="exact"
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
                    numeric
                />
            ),
        },
        {
            id: "actions",
            header: () => <span className="sr-only">操作</span>,
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
                                label: "详情",
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
    onOpenScope,
    renderToolbar,
}: {
    query: RequestQuery
    views: ListWorkspaceViewsConfig
    initialRequestId?: string
    initialCreate: boolean
    hasActiveFilters: boolean
    onClearFilters: () => void
    onOpenScope: () => void
    renderToolbar: (
        status: string | null,
        items?: readonly InvoiceRequest[],
    ) => React.ReactNode
}) {
    const permissions = useInvoiceRequestPermissions()
    const queryClient = useQueryClient()
    const [pageState, setPageState] = React.useState<{
        number: number
        size: number
        scopeVersion?: string
    }>({ number: 1, size: DEFAULT_PAGE_SIZE })
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
        page: pageState.number,
        page_size: pageState.size,
        scope_version: pageState.scopeVersion,
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
        if (!permissions.canRead) {
            return (
                <DetailWorkspacePage>
                    <DetailWorkspaceChrome
                        back={{
                            id: "invoice-request-no-read-back",
                            href: "/finance/customer-accounts",
                            label: "返回客户往来",
                        }}
                        title="开票申请"
                    />
                    <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                        <BusinessFailureState
                            kind="permission"
                            title="当前账号没有查看开票申请的权限。"
                        />
                    </div>
                </DetailWorkspacePage>
            )
        }
        if (showForm) {
            const closeForm = () => {
                setCreating(false)
                setEditing(undefined)
            }
            return (
                <DetailWorkspacePage>
                    <DetailWorkspaceChrome
                        title={editing ? "修改开票申请" : "新建开票申请"}
                        primaryStatus={{
                            label: editing ? "修改中" : "新建",
                            tone: "neutral",
                        }}
                        meta={
                            editing?.request_no
                                ? `申请单号 ${editing.request_no}`
                                : "填写销方与购方开票明细，审批通过后由财务开票"
                        }
                        secondaryActions={
                            <Button
                                id="invoice-request-form-back"
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={closeForm}
                            >
                                返回申请列表
                            </Button>
                        }
                    />
                    <DetailWorkspaceBody>
                        <DetailWorkspaceSection width="form">
                            <InvoiceRequestForm
                                key={editing?.id ?? "new"}
                                existing={editing}
                                onCancel={closeForm}
                                onDone={(request) => {
                                    const id =
                                        request.id ||
                                        pendingInvoiceRequestDetailId()
                                    setCreating(false)
                                    setEditing(undefined)
                                    if (id) setDetailId(id)
                                }}
                            />
                        </DetailWorkspaceSection>
                    </DetailWorkspaceBody>
                </DetailWorkspacePage>
            )
        }
        if (detailId) {
            return (
                <InvoiceRequestDetail
                    key={detailId}
                    id={detailId}
                    onBack={() => {
                        dismissPendingInvoiceRequestDetail()
                        setDetailId(undefined)
                    }}
                    onEdit={setEditing}
                />
            )
        }
        return null
    }

    const total = data?.total ?? 0
    const listFailed = list.isError || (!list.isPending && !data)
    const handlePaginationChange = (next: PaginationState) => {
        if (next.pageSize !== pageState.size) {
            setPageState({ number: 1, size: next.pageSize })
            return
        }
        const nextNumber = next.pageIndex + 1
        if (nextNumber === 1) setPageState({ number: 1, size: pageState.size })
        else
            setPageState({
                number: nextNumber,
                size: pageState.size,
                scopeVersion,
            })
    }

    return (
        <ListWorkspacePage
            title="客户往来"
            className="min-h-0"
            views={views}
            actions={[
                {
                    id: "invoice-request-header-scope",
                    actionKey: "open-scope",
                    label: "按范围汇总",
                    icon: BarChart3Icon,
                    tier: "quiet",
                    onClick: onOpenScope,
                },
                ...(permissions.canSubmit
                    ? [
                          {
                              id: "invoice-request-create",
                              actionKey: "create",
                              label: CREATE_LABEL,
                              icon: PlusIcon,
                              tier: "primary" as const,
                              onClick: onCreate,
                          },
                      ]
                    : []),
            ]}
            loading={pageState.number === 1 && list.isPending}
            surface={{
                ariaLabel: "开票申请",
                toolbar: renderToolbar(
                    listFailed
                        ? "查询未完成"
                        : list.isFetching
                          ? "查询中…"
                          : null,
                    data?.items,
                ),
                className: "@container/invoice-requests min-h-64",
                tableClassName:
                    "flex flex-col @max-lg/invoice-requests:[&_[data-column-pinned]]:inset-x-auto! @max-lg/invoice-requests:[&_[data-pinned-edge]]:shadow-none!",
                table: (
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
                            pageIndex: pageState.number - 1,
                            pageSize: pageState.size,
                        }}
                        onPaginationChange={handlePaginationChange}
                        pageSizeOptions={[20, 50, 100]}
                        showPagination={
                            !listFailed && (!list.isPending || Boolean(data))
                        }
                        loading={list.isFetching}
                        isError={listFailed}
                        error={list.error}
                        retryButtonId="invoice-request-list-retry"
                        onRetry={() => {
                            if (
                                pageState.number > 1 &&
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
                                            page_size: pageState.size,
                                        },
                                    ],
                                    exact: true,
                                    refetchType: "none",
                                })
                                setPageState({
                                    number: 1,
                                    size: pageState.size,
                                })
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
                ),
            }}
        />
    )
}
