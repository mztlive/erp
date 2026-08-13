"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    ChevronDownIcon,
    DownloadIcon,
    FilterIcon,
    Loader2Icon,
    PlusIcon,
    SearchIcon,
} from "lucide-react"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FixedOptionRadioFilter,
    FormalActionResult,
    ListToolbar,
    MoneyValue,
    PageActions,
    PageHeader,
    PageScaffold,
    StatusTrackSummary,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { DateRangePicker } from "@/components/ui/date-picker"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import {
    isPendingReviewStage,
    NATURE_LABEL,
    ORIGIN_LABEL,
    stageDueDisplay,
    stageOwnerDisplay,
} from "@/features/sales-orders/lib/labels"
import { SalesOrderPaperDialog } from "@/features/sales-orders/components/sales-order-paper-dialog"
import {
    salesOrderCloseLabel,
    salesOrderCollectionLabel,
    salesOrderCommercialStatusLabel,
    salesOrderFulfillmentLabel,
    salesOrderInvoiceLabel,
    salesOrderReviewStatusLabel,
    salesOrderSummaryLabels,
    SALES_ORDER_CLOSE_OPTIONS,
    SALES_ORDER_COLLECTION_OPTIONS,
    SALES_ORDER_COMMERCIAL_STATUS_OPTIONS,
    SALES_ORDER_FULFILLMENT_OPTIONS,
    SALES_ORDER_INVOICE_OPTIONS,
    SALES_ORDER_REVIEW_STATUS_OPTIONS,
} from "@/features/sales-orders/lib/filter-orders"
import {
    downloadSalesOrderContractPdf,
    fetchSalesOrders,
    type SalesOrdersListQuery,
} from "@/features/sales-orders/api/sales-orders"
import { getErrorMessage } from "@/lib/api/errors"
import {
    useCreateSalesOrderExportJobMutation,
    useSalesOrdersQuery,
} from "@/features/sales-orders/hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    ContractSearchCombobox,
    CustomerSearchCombobox,
} from "@/features/entity-selectors"
import { OwnerCombobox } from "@/components/business"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import {
    mergeSalesOrdersSearchParams,
    normalizedSalesOrdersSearchParams,
    parseSalesOrdersSearchParams,
    type SalesOrdersUrlState,
} from "@/features/sales-orders/lib/url-state"

const SORT_COLUMN_TO_FIELD: Record<
    string,
    NonNullable<SalesOrdersListQuery["sortBy"]>
> = {
    document: "documentNumber",
    contract: "contractNumber",
    amount: "amountGross",
    owner: "ownerName",
    submittedAt: "submittedAt",
}

const BUSINESS_TIME_ZONE_OFFSET_SECONDS = 8 * 60 * 60

function businessDateBoundary(
    value: string | undefined,
    endOfDay: boolean,
): number | undefined {
    if (!value) return undefined
    const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value)
    if (!match) return undefined
    const [, year, month, day] = match
    const utcSeconds =
        Date.UTC(
            Number(year),
            Number(month) - 1,
            Number(day),
            endOfDay ? 23 : 0,
            endOfDay ? 59 : 0,
            endOfDay ? 59 : 0,
        ) / 1000
    const seconds = utcSeconds - BUSINESS_TIME_ZONE_OFFSET_SECONDS
    return Number.isFinite(seconds) ? seconds : undefined
}

function businessDateStart(value?: string): number | undefined {
    return businessDateBoundary(value, false)
}

function businessDateEnd(value?: string): number | undefined {
    return businessDateBoundary(value, true)
}

export function SalesOrdersListPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const profileQuery = useAccountProfileQuery()
    const ownerOptionsQuery = useOwnerOptionsQuery()
    const currentUserId = profileQuery.data?.userid?.trim() ?? ""
    const url = React.useMemo(
        () => parseSalesOrdersSearchParams(searchParams),
        [searchParams],
    )

    const pushUrl = React.useCallback(
        (patch: Partial<SalesOrdersUrlState>) => {
            const next = { ...url, ...patch }
            const qs = mergeSalesOrdersSearchParams(searchParams, next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router, searchParams, url],
    )

    React.useEffect(() => {
        const normalized = normalizedSalesOrdersSearchParams(searchParams, url)
        if (normalized === undefined) return
        router.replace(`${pathname}${normalized}`, { scroll: false })
    }, [pathname, router, searchParams, url])

    const query = React.useMemo<SalesOrdersListQuery>(
        () => ({
            page: url.page,
            pageSize: url.pageSize,
            search: url.search,
            customerId: url.customerId,
            contractId: url.contractId,
            createdBy: url.createdBy,
            nature: url.nature,
            summary: url.summary,
            currentUserId,
            origin: url.origin,
            commercialStatus: url.commercialStatus,
            reviewStatus: url.reviewStatus,
            fulfillment: url.fulfillment,
            collection: url.collection,
            invoice: url.invoice,
            closeStatus: url.closeStatus,
            createdFrom: businessDateStart(url.createdFrom),
            createdTo: businessDateEnd(url.createdTo),
            sortBy: url.sort ? SORT_COLUMN_TO_FIELD[url.sort] : undefined,
            sortDir: url.dir,
        }),
        [url, currentUserId],
    )

    const identityReady =
        (url.summary !== "mine" && url.summary !== "createdByMe") ||
        Boolean(currentUserId)
    const ordersQuery = useSalesOrdersQuery(query, identityReady)
    const exportMutation = useCreateSalesOrderExportJobMutation()
    const items = React.useMemo(
        () => ordersQuery.data?.items ?? [],
        [ordersQuery.data?.items],
    )
    const total = ordersQuery.data?.total ?? 0

    const [searchDraft, setSearchDraft] = React.useState(url.search ?? "")
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(
        Boolean(
            url.customerId ||
            url.contractId ||
            url.createdBy ||
            url.nature !== "all" ||
            url.origin !== "all" ||
            url.commercialStatus !== "all" ||
            url.reviewStatus !== "all" ||
            url.fulfillment !== "all" ||
            url.collection !== "all" ||
            url.invoice !== "all" ||
            url.closeStatus !== "all" ||
            url.createdFrom ||
            url.createdTo,
        ),
    )
    const [filterDraft, setFilterDraft] = React.useState(() => ({
        customerId: url.customerId ?? "",
        contractId: url.contractId ?? "",
        createdBy: url.createdBy ?? "",
        nature: url.nature,
        origin: url.origin,
        commercialStatus: url.commercialStatus,
        reviewStatus: url.reviewStatus,
        fulfillment: url.fulfillment,
        collection: url.collection,
        invoice: url.invoice,
        closeStatus: url.closeStatus,
        createdFrom: url.createdFrom ?? "",
        createdTo: url.createdTo ?? "",
    }))
    const [paperId, setPaperId] = React.useState<string | null>(null)
    const [exportJob, setExportJob] = React.useState<{
        jobId: string
        rowCount: number
        downloadLabel: string
        exportedAt: string
        fileName: string
    } | null>(null)
    const [focusedIndex, setFocusedIndex] = React.useState(0)
    const [downloadingContractId, setDownloadingContractId] = React.useState<
        string | null
    >(null)
    const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())

    const openPaperPreview = React.useCallback((id: string) => {
        setPaperId(id)
    }, [])

    const downloadContract = React.useCallback(
        async (order: SalesOrderListItem) => {
            const contractId = order.contractId.trim()
            if (!contractId || downloadingContractId) return
            setDownloadingContractId(contractId)
            try {
                await downloadSalesOrderContractPdf(contractId)
            } catch (error) {
                toast.add({
                    title: "合同下载失败",
                    description: getErrorMessage(error, "请稍后重试"),
                    type: "error",
                    timeout: 4000,
                })
            } finally {
                setDownloadingContractId(null)
            }
        },
        [downloadingContractId],
    )

    const hasStructuredFilters = Boolean(
        url.customerId ||
        url.contractId ||
        url.createdBy ||
        url.nature !== "all" ||
        url.origin !== "all" ||
        url.commercialStatus !== "all" ||
        url.reviewStatus !== "all" ||
        url.fulfillment !== "all" ||
        url.collection !== "all" ||
        url.invoice !== "all" ||
        url.closeStatus !== "all" ||
        url.createdFrom ||
        url.createdTo,
    )

    React.useEffect(() => {
        setSearchDraft(url.search ?? "")
        setFilterDraft({
            customerId: url.customerId ?? "",
            contractId: url.contractId ?? "",
            createdBy: url.createdBy ?? "",
            nature: url.nature,
            origin: url.origin,
            commercialStatus: url.commercialStatus,
            reviewStatus: url.reviewStatus,
            fulfillment: url.fulfillment,
            collection: url.collection,
            invoice: url.invoice,
            closeStatus: url.closeStatus,
            createdFrom: url.createdFrom ?? "",
            createdTo: url.createdTo ?? "",
        })
        setFilterPanelOpen(hasStructuredFilters)
    }, [
        hasStructuredFilters,
        url.closeStatus,
        url.collection,
        url.commercialStatus,
        url.contractId,
        url.createdBy,
        url.createdFrom,
        url.createdTo,
        url.customerId,
        url.fulfillment,
        url.invoice,
        url.nature,
        url.origin,
        url.reviewStatus,
        url.search,
    ])

    React.useEffect(() => {
        setFocusedIndex(0)
    }, [
        url.closeStatus,
        url.collection,
        url.commercialStatus,
        url.contractId,
        url.createdBy,
        url.createdFrom,
        url.createdTo,
        url.customerId,
        url.fulfillment,
        url.invoice,
        url.nature,
        url.origin,
        url.page,
        url.reviewStatus,
        url.search,
        url.summary,
        items.length,
    ])

    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                if (event.key === "/" && target.tagName !== "INPUT") {
                    // allow
                } else if (event.key !== "Escape") {
                    return
                }
            }

            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="so-list-search"]',
                    )
                    ?.focus()
                return
            }

            if (items.length === 0) return

            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                setFocusedIndex((i) => Math.min(items.length - 1, i + 1))
            } else if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                setFocusedIndex((i) => Math.max(0, i - 1))
            } else if (event.key === "Enter") {
                event.preventDefault()
                const row = items[focusedIndex]
                if (row) router.push(`/sales/orders/${row.id}`)
            } else if (event.key === "Escape" && paperId) {
                event.preventDefault()
                const id = paperId
                setPaperId(null)
                requestAnimationFrame(() => {
                    rowRefs.current.get(id)?.focus()
                })
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [focusedIndex, items, openPaperPreview, paperId, router])

    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, url.page - 1),
            pageSize: url.pageSize,
        }),
        [url.page, url.pageSize],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
        },
        [pushUrl],
    )

    const sorting = React.useMemo<SortingState>(
        () =>
            url.sort && SORT_COLUMN_TO_FIELD[url.sort]
                ? [{ id: url.sort, desc: url.dir === "desc" }]
                : [],
        [url.dir, url.sort],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            pushUrl({
                sort:
                    head && SORT_COLUMN_TO_FIELD[head.id] ? head.id : undefined,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [pushUrl],
    )

    const paperOrder = React.useMemo(
        () => items.find((item) => item.id === paperId) ?? null,
        [items, paperId],
    )

    const exportCsv = React.useCallback(async () => {
        if (total === 0) return
        const job = await exportMutation.mutateAsync({ rowCount: total })
        const all = await fetchSalesOrders({
            ...query,
            page: 1,
            pageSize: total,
        })
        const now = new Date()
        const pad = (n: number) => String(n).padStart(2, "0")
        const datePart = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`
        const timePart = `${pad(now.getHours())}${pad(now.getMinutes())}`
        const fileName = `销售单列表_${datePart}_${timePart}.csv`
        setExportJob({
            jobId: job.jobId,
            rowCount: job.rowCount,
            downloadLabel: fileName,
            exportedAt: now.toISOString(),
            fileName,
        })

        const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
        const rows = all.items.map((order) =>
            [
                order.documentNumber,
                order.customerName,
                order.contractNumber,
                NATURE_LABEL[order.nature],
                order.primaryStatus.label,
                ORIGIN_LABEL[order.originSystem],
                order.amountGross,
                order.ownerName,
                order.submittedAt,
            ]
                .map((value) => quote(String(value)))
                .join(","),
        )
        const csv = [
            `# 导出时间 ${now.toLocaleString("zh-CN")}；仅包含当前筛选结果，金额以列表页最新数据为准。`,
            "销售单号,客户,合同,业务性质,状态,创建来源,成交金额（含税）,负责人,提交时间",
            ...rows,
        ].join("\n")
        const url = URL.createObjectURL(
            new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }),
        )
        const anchor = document.createElement("a")
        anchor.href = url
        anchor.download = fileName
        anchor.click()
        URL.revokeObjectURL(url)
    }, [exportMutation, query, total])

    const filtersActive = Boolean(url.search) || hasStructuredFilters

    const applyFilters = React.useCallback(() => {
        const [createdFrom, createdTo] =
            filterDraft.createdFrom &&
            filterDraft.createdTo &&
            filterDraft.createdFrom > filterDraft.createdTo
                ? [filterDraft.createdTo, filterDraft.createdFrom]
                : [filterDraft.createdFrom, filterDraft.createdTo]
        const summaryConflictsWithDraft =
            (url.summary === "mine" &&
                (Boolean(filterDraft.createdBy) ||
                    filterDraft.commercialStatus !== "all" ||
                    filterDraft.reviewStatus !== "all")) ||
            (url.summary === "createdByMe" && Boolean(filterDraft.createdBy)) ||
            (url.summary === "exception" &&
                (filterDraft.commercialStatus !== "all" ||
                    filterDraft.reviewStatus !== "all"))

        pushUrl({
            search: searchDraft.trim() || undefined,
            customerId: filterDraft.customerId || undefined,
            contractId: filterDraft.contractId || undefined,
            createdBy: filterDraft.createdBy || undefined,
            nature: filterDraft.nature,
            summary: summaryConflictsWithDraft ? "all" : url.summary,
            origin: filterDraft.origin,
            commercialStatus: filterDraft.commercialStatus,
            reviewStatus: filterDraft.reviewStatus,
            fulfillment: filterDraft.fulfillment,
            collection: filterDraft.collection,
            invoice: filterDraft.invoice,
            closeStatus: filterDraft.closeStatus,
            createdFrom: createdFrom || undefined,
            createdTo: createdTo || undefined,
            page: 1,
        })
    }, [filterDraft, pushUrl, searchDraft, url.summary])

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setFilterPanelOpen(false)
        setFilterDraft({
            customerId: "",
            contractId: "",
            createdBy: "",
            nature: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: "",
            createdTo: "",
        })
        pushUrl({
            search: undefined,
            customerId: undefined,
            contractId: undefined,
            createdBy: undefined,
            nature: "all",
            origin: "all",
            commercialStatus: "all",
            reviewStatus: "all",
            fulfillment: "all",
            collection: "all",
            invoice: "all",
            closeStatus: "all",
            createdFrom: undefined,
            createdTo: undefined,
            page: 1,
        })
    }, [pushUrl])

    const columns = React.useMemo<ColumnDef<SalesOrderListItem>[]>(
        () => [
            {
                id: "document",
                accessorKey: "documentNumber",
                header: "销售单",
                meta: { label: "销售单", width: "reference" },
                cell: ({ row }) => (
                    <div
                        className="flex min-w-0 items-center gap-2"
                        ref={(el) => {
                            if (el) rowRefs.current.set(row.original.id, el)
                            else rowRefs.current.delete(row.original.id)
                        }}
                        tabIndex={
                            items[focusedIndex]?.id === row.original.id ? 0 : -1
                        }
                        data-focused={
                            items[focusedIndex]?.id === row.original.id
                                ? "true"
                                : undefined
                        }
                    >
                        <div className="min-w-0 flex-1 space-y-1">
                            <div className="flex items-center gap-2">
                                <Button
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    className="num px-0"
                                    aria-label={`预览 ${row.original.documentNumber}`}
                                    onClick={() =>
                                        openPaperPreview(row.original.id)
                                    }
                                >
                                    {row.original.documentNumber}
                                </Button>
                                <BusinessStatusBadge
                                    context="list"
                                    label={row.original.primaryStatus.label}
                                    tone={row.original.primaryStatus.tone}
                                />
                            </div>
                            <div className="truncate text-xs text-muted-foreground">
                                {row.original.customerName}
                            </div>
                        </div>
                    </div>
                ),
            },
            {
                id: "nature",
                header: "业务性质",
                meta: { label: "业务性质", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <Badge variant="secondary">
                        {NATURE_LABEL[row.original.nature]}
                    </Badge>
                ),
            },
            {
                id: "contract",
                accessorKey: "contractNumber",
                header: "合同",
                meta: { label: "合同", width: "reference" },
                cell: ({ row }) => {
                    const order = row.original
                    const contractNo = order.contractNumber.trim()
                    const companyName = order.contractCompanyName.trim()
                    if (!order.contractId && !contractNo) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    const downloading =
                        downloadingContractId === order.contractId
                    return (
                        <div className="min-w-0 space-y-1">
                            {order.contractId ? (
                                <Button
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    className="num px-0"
                                    disabled={downloading}
                                    aria-label={`下载合同 ${contractNo || order.contractId}`}
                                    onClick={() => {
                                        void downloadContract(order)
                                    }}
                                >
                                    {downloading ? (
                                        <>
                                            <Loader2Icon
                                                data-icon="inline-start"
                                                className="animate-spin"
                                                aria-hidden="true"
                                            />
                                            下载中
                                        </>
                                    ) : (
                                        contractNo || "下载合同"
                                    )}
                                </Button>
                            ) : (
                                <span className="num text-sm">
                                    {contractNo || "—"}
                                </span>
                            )}
                            <div className="truncate text-xs text-muted-foreground">
                                {companyName || "—"}
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "tracks",
                header: "进度",
                meta: { label: "多轨进度", width: "tracks" },
                enableSorting: false,
                cell: ({ row }) => (
                    <StatusTrackSummary
                        variant="inline"
                        className="flex-nowrap gap-x-2 gap-y-0"
                        tracks={[
                            {
                                id: "fulfillment",
                                label: "履约",
                                status: row.original.fulfillment,
                            },
                            {
                                id: "collection",
                                label: "回款",
                                status: row.original.collection,
                            },
                            {
                                id: "invoicing",
                                label: "开票",
                                status: row.original.invoicing,
                            },
                        ]}
                    />
                ),
            },
            {
                id: "amount",
                accessorKey: "amountGross",
                header: "成交金额",
                meta: {
                    label: "成交金额",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue
                        value={row.original.amountGross}
                        taxBasis="gross"
                    />
                ),
            },
            {
                id: "owner",
                accessorKey: "ownerName",
                header: "负责人",
                meta: { label: "负责人", width: "default" },
            },
            {
                id: "currentOwner",
                header: "当前责任 / 时限",
                meta: { label: "当前责任 / 时限", width: "default" },
                enableSorting: false,
                cell: ({ row }) => {
                    const order = row.original
                    if (!isPendingReviewStage(order.primaryStatus.code)) {
                        return (
                            <span className="text-sm text-muted-foreground">
                                —
                            </span>
                        )
                    }
                    const due = stageDueDisplay(order)
                    return (
                        <div className="text-sm">
                            <div>{stageOwnerDisplay(order)}</div>
                            <div className="text-xs text-muted-foreground">
                                {due ? (
                                    <time dateTime={due.dateTime}>
                                        {due.label}
                                    </time>
                                ) : (
                                    "时限未设置"
                                )}
                            </div>
                        </div>
                    )
                },
            },
            {
                id: "submittedAt",
                accessorKey: "submittedAt",
                header: "提交时间",
                meta: { label: "提交时间", width: "default", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-sm text-muted-foreground">
                        {row.original.submittedAt}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default", align: "end" },
                enableSorting: false,
                cell: ({ row }) => (
                    <div className="flex justify-end gap-1">
                        <Button
                            type="button"
                            variant="outline"
                            size="xs"
                            render={
                                <Link
                                    href={`/sales/orders/${row.original.id}`}
                                />
                            }
                        >
                            查看详情
                        </Button>
                    </div>
                ),
            },
        ],
        [
            downloadingContractId,
            downloadContract,
            focusedIndex,
            items,
            openPaperPreview,
        ],
    )

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="销售单"
                breadcrumbs={[
                    { id: "sales", label: "销售", href: "/sales/orders" },
                    { id: "orders", label: "销售单", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={
                            ordersQuery.isError
                                ? "查询失败"
                                : ordersQuery.data
                                  ? ordersQuery.data.queriedAt.slice(11, 16)
                                  : "正在查询"
                        }
                        dateTime={ordersQuery.data?.queriedAt}
                        state={
                            ordersQuery.isError
                                ? "failed"
                                : ordersQuery.isFetching
                                  ? "syncing"
                                  : ordersQuery.data
                                    ? "fresh"
                                    : "unknown"
                        }
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "create",
                                label: "新建销售单",
                                icon: PlusIcon,
                                render: (
                                    <Link href="/sales/orders?mode=create" />
                                ),
                            },
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled:
                                    total === 0 || exportMutation.isPending,
                                onClick: () => {
                                    void exportCsv()
                                },
                            },
                        ]}
                    />
                }
            />

            {exportJob ? (
                <FormalActionResult
                    status="succeeded"
                    title="导出完成"
                    description={`已生成 CSV 文件，共 ${exportJob.rowCount} 行，仅包含当前筛选结果；导出后金额与状态以列表页最新数据为准。`}
                    facts={[
                        {
                            label: "文件",
                            value: exportJob.fileName,
                        },
                        {
                            label: "行数",
                            value: String(exportJob.rowCount),
                        },
                        {
                            label: "导出时间",
                            value: new Date(
                                exportJob.exportedAt,
                            ).toLocaleString("zh-CN"),
                        },
                    ]}
                />
            ) : null}

            <ToggleGroup
                value={[url.summary]}
                onValueChange={(values) => {
                    const next = values[0] as
                        | SalesOrdersUrlState["summary"]
                        | undefined
                    // 工作视图会约束创建人或审核轨；切换时清掉重叠条件，避免同字段冲突。
                    pushUrl({
                        summary: next ?? "all",
                        createdBy: undefined,
                        commercialStatus: "all",
                        reviewStatus: "all",
                        page: 1,
                    })
                }}
                variant="outline"
                size="sm"
                spacing={0}
                aria-label="销售单工作视图"
            >
                <ToggleGroupItem value="all">全部</ToggleGroupItem>
                <ToggleGroupItem value="mine">待我处理</ToggleGroupItem>
                <ToggleGroupItem value="createdByMe">我创建的</ToggleGroupItem>
                <ToggleGroupItem value="exception">异常</ToggleGroupItem>
            </ToggleGroup>

            <BusinessTableFrame
                title="销售单列表"
                description={
                    !filtersActive
                        ? "设置一个或多个条件后统一搜索；筛选条件会保存在网址中，便于刷新、返回与分享。"
                        : `当前筛选：${[
                              url.summary !== "all"
                                  ? salesOrderSummaryLabels(url.summary)
                                  : null,
                              url.nature !== "all"
                                  ? NATURE_LABEL[url.nature]
                                  : null,
                              url.origin !== "all"
                                  ? ORIGIN_LABEL[url.origin]
                                  : null,
                              url.commercialStatus !== "all"
                                  ? salesOrderCommercialStatusLabel(
                                        url.commercialStatus,
                                    )
                                  : null,
                              url.reviewStatus !== "all"
                                  ? salesOrderReviewStatusLabel(
                                        url.reviewStatus,
                                    )
                                  : null,
                              url.fulfillment !== "all"
                                  ? salesOrderFulfillmentLabel(url.fulfillment)
                                  : null,
                              url.collection !== "all"
                                  ? salesOrderCollectionLabel(url.collection)
                                  : null,
                              url.invoice !== "all"
                                  ? salesOrderInvoiceLabel(url.invoice)
                                  : null,
                              url.closeStatus !== "all"
                                  ? salesOrderCloseLabel(url.closeStatus)
                                  : null,
                              url.customerId ? "已选客户" : null,
                              url.contractId ? "已选合同" : null,
                              url.createdBy ? "已选创建人" : null,
                              url.createdFrom || url.createdTo
                                  ? `创建日期 ${url.createdFrom || "不限"} 至 ${url.createdTo || "不限"}`
                                  : null,
                              url.search ? `关键词“${url.search}”` : null,
                          ]
                              .filter(Boolean)
                              .join(" · ")}`
                }
                toolbar={
                    <form
                        onSubmit={(event) => {
                            event.preventDefault()
                            applyFilters()
                        }}
                    >
                        <ListToolbar
                            search={
                                <InputGroup>
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        data-slot="so-list-search"
                                        value={searchDraft}
                                        onChange={(event) => {
                                            setSearchDraft(event.target.value)
                                        }}
                                        placeholder="销售单号"
                                        aria-label="搜索销售单号"
                                    />
                                </InputGroup>
                            }
                            filters={
                                <>
                                    {!filterPanelOpen ? (
                                        <Button type="submit" size="sm">
                                            <SearchIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            搜索
                                        </Button>
                                    ) : null}
                                    <Button
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        aria-expanded={filterPanelOpen}
                                        aria-controls="sales-order-filter-panel"
                                        onClick={() => {
                                            setFilterPanelOpen((open) => !open)
                                        }}
                                    >
                                        <FilterIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        高级筛选
                                        {hasStructuredFilters ? (
                                            <Badge variant="info">已启用</Badge>
                                        ) : null}
                                        <ChevronDownIcon
                                            data-icon="inline-end"
                                            aria-hidden="true"
                                            className={
                                                filterPanelOpen
                                                    ? "rotate-180 transition-transform"
                                                    : "transition-transform"
                                            }
                                        />
                                    </Button>
                                </>
                            }
                            secondary={
                                filterPanelOpen ? (
                                    <div
                                        id="sales-order-filter-panel"
                                        className="flex w-full flex-col gap-4 rounded-lg border border-border/60 bg-muted/30 px-3 py-3"
                                        aria-label="销售单筛选条件"
                                    >
                                        <FixedOptionRadioFilter
                                            label="业务性质"
                                            value={filterDraft.nature}
                                            onValueChange={(nature) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    nature,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                {
                                                    value: "physical_service",
                                                    label: "实物与服务",
                                                },
                                                {
                                                    value: "card_voucher",
                                                    label: "卡券",
                                                },
                                            ]}
                                        />
                                        <FixedOptionRadioFilter
                                            label="创建来源"
                                            value={filterDraft.origin}
                                            onValueChange={(origin) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    origin,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                { value: "erp", label: "ERP" },
                                                {
                                                    value: "mall",
                                                    label: "商城",
                                                },
                                            ]}
                                        />
                                        <FixedOptionRadioFilter
                                            label="商业状态"
                                            value={filterDraft.commercialStatus}
                                            onValueChange={(
                                                commercialStatus,
                                            ) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    commercialStatus,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                ...SALES_ORDER_COMMERCIAL_STATUS_OPTIONS,
                                            ]}
                                        />
                                        <FixedOptionRadioFilter
                                            label="审核状态"
                                            value={filterDraft.reviewStatus}
                                            onValueChange={(reviewStatus) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    reviewStatus,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                ...SALES_ORDER_REVIEW_STATUS_OPTIONS,
                                            ]}
                                        />
                                        <FixedOptionRadioFilter
                                            label="履约进度"
                                            value={filterDraft.fulfillment}
                                            onValueChange={(fulfillment) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    fulfillment,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                ...SALES_ORDER_FULFILLMENT_OPTIONS,
                                            ]}
                                        />
                                        <FixedOptionRadioFilter
                                            label="回款进度"
                                            value={filterDraft.collection}
                                            onValueChange={(collection) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    collection,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                ...SALES_ORDER_COLLECTION_OPTIONS,
                                            ]}
                                        />
                                        <FixedOptionRadioFilter
                                            label="开票进度"
                                            value={filterDraft.invoice}
                                            onValueChange={(invoice) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    invoice,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                ...SALES_ORDER_INVOICE_OPTIONS,
                                            ]}
                                        />
                                        <FixedOptionRadioFilter
                                            label="关闭状态"
                                            value={filterDraft.closeStatus}
                                            onValueChange={(closeStatus) => {
                                                setFilterDraft((draft) => ({
                                                    ...draft,
                                                    closeStatus,
                                                }))
                                            }}
                                            options={[
                                                { value: "all", label: "全部" },
                                                ...SALES_ORDER_CLOSE_OPTIONS,
                                            ]}
                                        />

                                        <FieldGroup className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
                                            <Field>
                                                <FieldLabel>客户</FieldLabel>
                                                <CustomerSearchCombobox
                                                    purpose="filter"
                                                    scope="all_authorized"
                                                    value={
                                                        filterDraft.customerId ||
                                                        undefined
                                                    }
                                                    onValueChange={(
                                                        customerId,
                                                    ) => {
                                                        setFilterDraft(
                                                            (draft) => ({
                                                                ...draft,
                                                                customerId:
                                                                    customerId ??
                                                                    "",
                                                                contractId:
                                                                    customerId ===
                                                                    draft.customerId
                                                                        ? draft.contractId
                                                                        : "",
                                                            }),
                                                        )
                                                    }}
                                                    placeholder="全部客户"
                                                />
                                            </Field>
                                            <Field>
                                                <FieldLabel>合同</FieldLabel>
                                                <ContractSearchCombobox
                                                    purpose="filter"
                                                    customerId={
                                                        filterDraft.customerId ||
                                                        undefined
                                                    }
                                                    value={
                                                        filterDraft.contractId ||
                                                        undefined
                                                    }
                                                    onValueChange={(
                                                        contractId,
                                                    ) => {
                                                        setFilterDraft(
                                                            (draft) => ({
                                                                ...draft,
                                                                contractId:
                                                                    contractId ??
                                                                    "",
                                                            }),
                                                        )
                                                    }}
                                                    placeholder="全部合同"
                                                />
                                            </Field>
                                            <Field>
                                                <FieldLabel>创建人</FieldLabel>
                                                <OwnerCombobox
                                                    owners={
                                                        ownerOptionsQuery.data ??
                                                        []
                                                    }
                                                    loading={
                                                        ownerOptionsQuery.isFetching
                                                    }
                                                    value={
                                                        filterDraft.createdBy ||
                                                        undefined
                                                    }
                                                    onValueChange={(
                                                        createdBy,
                                                    ) => {
                                                        setFilterDraft(
                                                            (draft) => ({
                                                                ...draft,
                                                                createdBy:
                                                                    createdBy ??
                                                                    "",
                                                            }),
                                                        )
                                                    }}
                                                    placeholder="全部创建人"
                                                />
                                            </Field>
                                            <Field>
                                                <FieldLabel>
                                                    创建日期
                                                </FieldLabel>
                                                <DateRangePicker
                                                    className="w-full"
                                                    value={
                                                        filterDraft.createdFrom ||
                                                        filterDraft.createdTo
                                                            ? {
                                                                  from:
                                                                      filterDraft.createdFrom ||
                                                                      undefined,
                                                                  to:
                                                                      filterDraft.createdTo ||
                                                                      undefined,
                                                              }
                                                            : undefined
                                                    }
                                                    onValueChange={(range) => {
                                                        setFilterDraft(
                                                            (draft) => ({
                                                                ...draft,
                                                                createdFrom:
                                                                    range?.from ??
                                                                    "",
                                                                createdTo:
                                                                    range?.to ??
                                                                    "",
                                                            }),
                                                        )
                                                    }}
                                                    placeholder="全部日期"
                                                />
                                            </Field>
                                        </FieldGroup>

                                        <div className="flex justify-end">
                                            <Button type="submit" size="sm">
                                                <SearchIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                搜索
                                            </Button>
                                        </div>
                                    </div>
                                ) : undefined
                            }
                            actions={
                                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                    <span aria-live="polite">
                                        共 {total.toLocaleString("zh-CN")} 条
                                    </span>
                                    <span
                                        className="hidden md:inline"
                                        aria-hidden="true"
                                    >
                                        ·
                                    </span>
                                    <span className="hidden md:inline">
                                        / 聚焦搜索 · ↑↓ 选择行 · Enter 打开详情
                                    </span>
                                    {filtersActive ? (
                                        <Button
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            onClick={clearFilters}
                                        >
                                            清除筛选
                                        </Button>
                                    ) : null}
                                </div>
                            }
                        />
                    </form>
                }
                table={
                    ordersQuery.isError ? (
                        <BusinessFailureState
                            title="销售单列表加载失败"
                            error={ordersQuery.error}
                            onRetry={() => {
                                void ordersQuery.refetch()
                            }}
                        />
                    ) : !ordersQuery.isPending && items.length === 0 ? (
                        <BusinessEmptyState
                            kind={filtersActive ? "filter" : "no-data"}
                            title={filtersActive ? undefined : "还没有销售单"}
                            description={
                                filtersActive
                                    ? "换一个关键词或清除筛选后再试。"
                                    : "当前业务范围内还没有销售单，可新建第一张单。"
                            }
                            action={
                                filtersActive ? (
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={clearFilters}
                                    >
                                        清除筛选
                                    </Button>
                                ) : (
                                    <Button
                                        type="button"
                                        size="sm"
                                        render={
                                            <Link href="/sales/orders?mode=create" />
                                        }
                                    >
                                        <PlusIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        新建销售单
                                    </Button>
                                )
                            }
                        />
                    ) : (
                        <DataTable
                            data={items}
                            columns={columns}
                            getRowId={(row) => row.id}
                            rowCount={total}
                            sorting={sorting}
                            onSortingChange={handleSortingChange}
                            pagination={pagination}
                            onPaginationChange={handlePaginationChange}
                            loading={ordersQuery.isPending}
                            layout="flush"
                            density="comfortable"
                            className="[&_[data-slot=table-cell]]:h-auto [&_[data-slot=table-cell]]:min-h-(--table-row-height) [&_[data-slot=table-cell]]:py-2.5 [&_[data-slot=table-head]]:h-auto [&_[data-slot=table-head]]:min-h-(--table-row-height) [&_[data-slot=table-head]]:py-2.5"
                            defaultColumnPinning={{
                                left: ["document"],
                                right: ["actions"],
                            }}
                            onRowPreview={(row) =>
                                router.push(`/sales/orders/${row.id}`)
                            }
                            onRowOpen={(row) =>
                                router.push(`/sales/orders/${row.id}`)
                            }
                        />
                    )
                }
            />

            <SalesOrderPaperDialog
                order={paperOrder}
                open={paperOrder != null}
                onOpenChange={(open) => {
                    if (!open) setPaperId(null)
                }}
            />
        </PageScaffold>
    )
}
