"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { OptionCombobox, PageScaffold } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import {
    useInvoiceRequestPermissions,
    useInvoiceRequests,
} from "../hooks/queries"
import { requestStatusLabels, type RequestStatus } from "../api"
import { InvoiceRequestPanel } from "./request-panel"

const STATUS_OPTIONS = (
    Object.entries(requestStatusLabels) as [RequestStatus, string][]
).map(([value, label]) => ({ value, label }))

function isRequestStatus(value: string): value is RequestStatus {
    return value in requestStatusLabels
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
    const [searchDraft, setSearchDraft] = React.useState(q)
    const [customerDraft, setCustomerDraft] = React.useState(customerId)
    const [orderDraft, setOrderDraft] = React.useState(salesOrderId)
    const [statusDraft, setStatusDraft] = React.useState(appliedStatus)
    const [panelOpen, setPanelOpen] = React.useState(false)
    const permissions = useInvoiceRequestPermissions()
    const query = {
        q: q.trim() || undefined,
        customer_id: customerId || undefined,
        sales_order_id: salesOrderId || undefined,
        status: appliedStatus || undefined,
    }
    const listQuery = useInvoiceRequests(
        {
            ...query,
            page: 1,
            page_size: 10,
        },
        permissions.canRead,
    )

    React.useEffect(() => {
        setSearchDraft(q)
        setCustomerDraft(customerId)
        setOrderDraft(salesOrderId)
        setStatusDraft(appliedStatus)
    }, [appliedStatus, customerId, q, salesOrderId])

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
        [appliedStatus, customerId, q, replaceFilters, salesOrderId],
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
    }, [replaceFilters])

    const chips = React.useMemo(() => {
        const items: { key: string; label: string }[] = []
        const queryText = q.trim()
        if (queryText) items.push({ key: "q", label: `搜索：${queryText}` })
        if (customerId) items.push({ key: "customerId", label: "已选客户" })
        if (salesOrderId)
            items.push({ key: "salesOrderId", label: "已选销售单" })
        if (appliedStatus) {
            items.push({
                key: "status",
                label: `状态：${requestStatusLabels[appliedStatus]}`,
            })
        }
        return items
    }, [appliedStatus, customerId, q, salesOrderId])

    const moreCount = chips.filter(
        ({ key }) => key === "salesOrderId" || key === "status",
    ).length
    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        customerDraft !== customerId ||
        orderDraft !== salesOrderId ||
        statusDraft !== appliedStatus

    return (
        <PageScaffold density="compact" className="space-y-6">
            <header>
                <h1 className="text-2xl font-semibold">客户往来</h1>
                <p className="mt-2 text-sm text-muted-foreground">
                    管理客户开票申请，跟踪审批与财务开票进度。
                </p>
            </header>
            <nav
                aria-label="客户往来工作视图"
                className="flex gap-5 overflow-x-auto border-b pb-3 text-sm"
            >
                {[
                    ["receivable", "应收"],
                    ["receipt", "回款"],
                    ["sales_invoice", "销项发票"],
                    ["unallocated", "待分配"],
                ].map(([view, label]) => (
                    <Link
                        id={`invoice-request-nav-${view}`}
                        key={view}
                        href={`/finance/customer-accounts?view=${view}`}
                        className="whitespace-nowrap text-muted-foreground"
                    >
                        {label}
                    </Link>
                ))}
                <span
                    aria-current="page"
                    className="whitespace-nowrap font-semibold"
                >
                    开票申请
                </span>
            </nav>
            <InvoiceRequestPanel
                key={appliedQuery}
                query={query}
                toolbar={
                    <ListWorkspaceFilterBar
                        morePresentation="popover"
                        moreSize="compact"
                        className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
                        idPrefix="invoice-request-filter"
                        formAriaLabel="开票申请查询"
                        onSubmit={applyFilters}
                        queryButtonId="invoice-request-filter-apply"
                        resetMoreButtonId="invoice-request-filter-reset"
                        search={
                            <ListSearchField
                                id="invoice-request-search"
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="申请单号、抬头、事由"
                                aria-label="搜索开票申请"
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
                                placeholder="全部"
                            />
                        }
                        morePanel={
                            <div className="grid min-w-0 gap-4">
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
                            </div>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: listQuery.isFetching,
                            failed: listQuery.isError,
                            resultCount: listQuery.data?.total,
                            noun: "条申请",
                            loadingLabel: "正在加载申请…",
                        })}
                        chips={chips}
                        onClearChip={(key) =>
                            removeFilter(
                                key as
                                    | "q"
                                    | "customerId"
                                    | "salesOrderId"
                                    | "status",
                            )
                        }
                        onClearAll={clearFilters}
                        hasPendingChanges={hasPendingChanges}
                        pendingHint="条件已修改，待查询"
                    />
                }
                initialRequestId={params.get("requestId") ?? undefined}
                initialCreate={params.get("create") === "1"}
            />
        </PageScaffold>
    )
}
