"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { BarChart3Icon, PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    DetailWorkspaceChrome,
    DetailWorkspacePage,
    DetailWorkspaceSkeleton,
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
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { listFilterText } from "@/lib/ui-text"
import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { isWorkspaceFrame, workspaceHost } from "@/lib/workspace-tabs"

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
import { InvoiceRequestForm } from "./request-form"

const LIST_HREF = "/finance/customer-accounts/invoice-requests"
const CREATE_HREF = `${LIST_HREF}?create=1`
const SCOPE_HREF = `${LIST_HREF}/scope`
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

const PRESET_STATUS_VIEWS: ReadonlyArray<{
    value: "" | "in_approval" | "approved" | "completed"
    id: string
    label: string
}> = [
    { value: "", id: "invoice-request-view-all", label: "全部" },
    {
        value: "in_approval",
        id: "invoice-request-view-in-approval",
        label: "审批中",
    },
    { value: "approved", id: "invoice-request-view-approved", label: "待开票" },
    {
        value: "completed",
        id: "invoice-request-view-completed",
        label: "已开票",
    },
]

function isRequestStatus(value: string): value is RequestStatus {
    return Object.hasOwn(requestStatusLabels, value)
}

function isInlinePresetStatus(status: RequestStatus | ""): boolean {
    return (
        status === "in_approval" ||
        status === "approved" ||
        status === "completed"
    )
}

function detailHref(id: string): string {
    return `${LIST_HREF}/${encodeURIComponent(id)}`
}

function safeRemaining(amount?: string, invoiced?: string): string {
    const a = amount || "0.00"
    const b = invoiced || "0.00"
    if (!/^-?\d+(?:\.\d{1,2})?$/.test(a) || !/^-?\d+(?:\.\d{1,2})?$/.test(b)) {
        return a
    }
    return subtractFixed(a, b, { maxScale: 2, outputScale: 2 })
}

/** 开票申请独立工作面：页头内联 4 项状态视图，新建与详情进入独立 DetailWorkspacePage。 */
export function InvoiceRequestsPage() {
    const params = useSearchParams()
    const router = useRouter()
    const pathname = usePathname()
    const permissions = useInvoiceRequestPermissions()
    const legacyRequestId = params.get("requestId")?.trim() || ""
    const isCreateMode = params.get("create") === "1"

    // 兼容历史 ?requestId= 深链，转入独立详情路由。
    React.useEffect(() => {
        if (!legacyRequestId) return
        router.replace(detailHref(legacyRequestId))
    }, [legacyRequestId, router])

    const closeCreate = React.useCallback(() => {
        dismissPendingInvoiceRequestDetail()
        const host = workspaceHost()
        if (host) {
            host.closeCurrent(isWorkspaceFrame() ? window : null, LIST_HREF)
        } else {
            router.push(LIST_HREF)
        }
    }, [router])

    const appliedQuery = params.toString()
    const q = params.get("q") ?? ""
    const customerId = params.get("customerId") ?? ""
    const salesOrderId = params.get("salesOrderId") ?? ""
    const statusParam = params.get("status") ?? ""
    const appliedStatus = isRequestStatus(statusParam) ? statusParam : ""
    const { searchDraft, setSearchDraft } = useSearchDraft(q)
    const [customerDraft, setCustomerDraft] = React.useState(customerId)
    const [orderDraft, setOrderDraft] = React.useState(salesOrderId)
    const [statusDraft, setStatusDraft] = React.useState<RequestStatus | "">(
        appliedStatus,
    )
    const [selectedNames, setSelectedNames] = React.useState<{
        customer?: string
        order?: string
    }>({})
    const [panelOpen, setPanelOpen] = React.useState(false)

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
            next.delete("create")
            next.delete("requestId")
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

    const changeStatusView = React.useCallback(
        (nextStatus: "" | "in_approval" | "approved" | "completed") => {
            setStatusDraft(nextStatus)
            replaceFilters({
                q,
                customerId,
                salesOrderId,
                status: nextStatus,
            })
        },
        [customerId, q, replaceFilters, salesOrderId],
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

    const openScope = React.useCallback(() => {
        router.push(SCOPE_HREF)
    }, [router])

    const openCreate = React.useCallback(() => {
        dismissPendingInvoiceRequestDetail()
        router.push(CREATE_HREF)
    }, [router])

    const openDetail = React.useCallback(
        (request: InvoiceRequest) => {
            router.push(detailHref(request.id))
        },
        [router],
    )

    if (legacyRequestId) {
        return (
            <DetailWorkspacePage>
                <div className="px-(--scaffold-px) py-6">
                    <p role="status" className="text-xs text-muted-foreground">
                        正在打开开票申请详情…
                    </p>
                </div>
            </DetailWorkspacePage>
        )
    }

    if (isCreateMode) {
        if (permissions.isPending) {
            return (
                <DetailWorkspaceSkeleton
                    title={CREATE_LABEL}
                    description="正在核对当前账号权限…"
                    sidebar
                />
            )
        }
        if (!permissions.canSubmit) {
            return (
                <DetailWorkspacePage>
                    <DetailWorkspaceChrome
                        back={{
                            id: "invoice-request-create-no-permission-back",
                            href: permissions.canRead
                                ? LIST_HREF
                                : "/workspace",
                            label: permissions.canRead
                                ? "返回开票申请"
                                : "返回工作台",
                        }}
                        title={CREATE_LABEL}
                    />
                    <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                        <BusinessFailureState
                            kind="permission"
                            title="当前账号没有提交开票申请的权限。"
                        />
                    </div>
                </DetailWorkspacePage>
            )
        }
        return (
            <DetailWorkspacePage>
                <InvoiceRequestForm
                    key="new"
                    presentation="workspace"
                    workspaceChrome={{
                        title: CREATE_LABEL,
                        statusLabel: "新建",
                        back: {
                            id: "invoice-request-form-back",
                            label: "返回开票申请",
                            onClick: closeCreate,
                        },
                    }}
                    onCancel={closeCreate}
                    onDone={(request) => {
                        const id = request.id || pendingInvoiceRequestDetailId()
                        dismissPendingInvoiceRequestDetail()
                        if (id) {
                            router.replace(detailHref(id))
                        } else {
                            closeCreate()
                        }
                    }}
                />
            </DetailWorkspacePage>
        )
    }

    const query: RequestQuery = {
        q: q.trim() || undefined,
        customer_id: customerId || undefined,
        sales_order_id: salesOrderId || undefined,
        status: appliedStatus || undefined,
    }

    const searchChanged = searchDraft.trim() !== q.trim()
    const customerChanged = customerDraft !== customerId
    const foldedStatus =
        appliedStatus && !isInlinePresetStatus(appliedStatus)
            ? appliedStatus
            : ""
    const moreCount = (salesOrderId ? 1 : 0) + (foldedStatus ? 1 : 0)
    const hasToolbarFilters = Boolean(
        q.trim() || customerId || salesOrderId || foldedStatus,
    )
    const hasActiveFilters = Boolean(
        q.trim() || customerId || salesOrderId || appliedStatus,
    )
    const hasPendingChanges =
        searchChanged ||
        customerChanged ||
        orderDraft !== salesOrderId ||
        statusDraft !== appliedStatus

    return (
        <InvoiceRequestListWorkspace
            appliedQuery={appliedQuery}
            appliedStatus={appliedStatus}
            query={query}
            permissions={permissions}
            hasActiveFilters={hasActiveFilters}
            onClearFilters={clearFilters}
            onChangeStatusView={changeStatusView}
            onOpenScope={openScope}
            onCreate={openCreate}
            onOpenDetail={openDetail}
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
                if (foldedStatus) {
                    chips.push({
                        key: "status",
                        label: `状态：${requestStatusLabels[foldedStatus]}`,
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
                            hasToolbarFilters && chips.length === 0 ? (
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
    appliedQuery,
    appliedStatus,
    query: filters,
    permissions,
    hasActiveFilters,
    onClearFilters,
    onChangeStatusView,
    onOpenScope,
    onCreate,
    onOpenDetail,
    renderToolbar,
}: {
    appliedQuery: string
    appliedStatus: RequestStatus | ""
    query: RequestQuery
    permissions: ReturnType<typeof useInvoiceRequestPermissions>
    hasActiveFilters: boolean
    onClearFilters: () => void
    onChangeStatusView: (
        next: "" | "in_approval" | "approved" | "completed",
    ) => void
    onOpenScope: () => void
    onCreate: () => void
    onOpenDetail: (request: InvoiceRequest) => void
    renderToolbar: (
        status: string | null,
        items?: readonly InvoiceRequest[],
    ) => React.ReactNode
}) {
    const queryClient = useQueryClient()
    const [pageState, setPageState] = React.useState<{
        queryKey: string
        number: number
        size: number
        scopeVersion?: string
    }>({ queryKey: appliedQuery, number: 1, size: DEFAULT_PAGE_SIZE })
    const [listSettled, setListSettled] = React.useState(false)

    const currentPage =
        pageState.queryKey === appliedQuery ? pageState.number : 1
    const currentScopeVersion =
        pageState.queryKey === appliedQuery ? pageState.scopeVersion : undefined

    const listQuery: RequestQuery & { scope_version?: string } = {
        ...filters,
        page: currentPage,
        page_size: pageState.size,
        scope_version: currentScopeVersion,
    }
    const list = useInvoiceRequests(listQuery, permissions.canRead)
    const data = list.data
    const listFailed = list.isError || (!list.isPending && !data)

    React.useEffect(() => {
        if (
            !listSettled &&
            permissions.canRead &&
            (!list.isPending || listFailed)
        ) {
            setListSettled(true)
        }
    }, [list.isPending, listFailed, listSettled, permissions.canRead])

    const scopeVersion =
        data &&
        "scope_version" in data &&
        typeof data.scope_version === "string"
            ? data.scope_version
            : undefined
    const noScope = Boolean(
        data && "empty_reason" in data && data.empty_reason === "no_scope",
    )
    const columns = React.useMemo(
        () => invoiceRequestColumns(onOpenDetail),
        [onOpenDetail],
    )

    const total = data?.total ?? 0
    const handlePaginationChange = (next: PaginationState) => {
        if (next.pageSize !== pageState.size) {
            setPageState({
                queryKey: appliedQuery,
                number: 1,
                size: next.pageSize,
            })
            return
        }
        const nextNumber = next.pageIndex + 1
        if (nextNumber === 1) {
            setPageState({
                queryKey: appliedQuery,
                number: 1,
                size: pageState.size,
            })
        } else {
            setPageState({
                queryKey: appliedQuery,
                number: nextNumber,
                size: pageState.size,
                scopeVersion,
            })
        }
    }

    const views: ListWorkspaceViewsConfig | undefined = permissions.canRead
        ? {
              ariaLabel: "开票申请状态视图",
              items: PRESET_STATUS_VIEWS.map((option) => ({
                  id: option.id,
                  label: option.label,
                  count:
                      appliedStatus === option.value && data && !listFailed
                          ? total.toLocaleString("zh-CN")
                          : undefined,
                  active: appliedStatus === option.value,
                  onClick: () => onChangeStatusView(option.value),
              })),
          }
        : undefined

    const initialLoading =
        permissions.isPending ||
        (permissions.canRead && !listSettled && list.isPending && !listFailed)

    return (
        <ListWorkspacePage
            eyebrow=""
            title="开票申请"
            className="min-h-0"
            views={views}
            actions={[
                ...(permissions.canRead
                    ? [
                          {
                              id: "invoice-request-header-scope",
                              actionKey: "open-scope",
                              label: "按范围汇总",
                              icon: BarChart3Icon,
                              tier: "quiet" as const,
                              onClick: onOpenScope,
                          },
                      ]
                    : []),
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
            loading={initialLoading}
            alerts={
                !permissions.isPending && !permissions.canRead ? (
                    <BusinessFailureState
                        kind="permission"
                        title="当前账号没有查看开票申请的权限。"
                    />
                ) : undefined
            }
            surface={
                permissions.canRead
                    ? {
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
                                      pageIndex: currentPage - 1,
                                      pageSize: pageState.size,
                                  }}
                                  onPaginationChange={handlePaginationChange}
                                  pageSizeOptions={[20, 50, 100]}
                                  showPagination={
                                      !listFailed &&
                                      (!list.isPending || Boolean(data))
                                  }
                                  loading={list.isFetching}
                                  isError={listFailed}
                                  error={list.error}
                                  retryButtonId="invoice-request-list-retry"
                                  onRetry={() => {
                                      if (
                                          currentPage > 1 &&
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
                                              queryKey: appliedQuery,
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
                                  onRowPreview={onOpenDetail}
                                  onRowOpen={onOpenDetail}
                                  scrollable
                              />
                          ),
                      }
                    : undefined
            }
        />
    )
}
