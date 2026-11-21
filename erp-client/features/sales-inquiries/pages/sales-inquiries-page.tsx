"use client"

import * as React from "react"
import Link from "next/link"
import { useStore } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    OptionCombobox,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    listWorkspaceStyles,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button, buttonVariants } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { patchUrl } from "@/lib/patch-search-params"
import { hasPermission } from "@/lib/permissions"
import { listFilterText } from "@/lib/ui-text"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { InquiryCreateImport } from "../components/inquiry-create-import"
import { TRANSFER_LABEL, VALIDITY_LABEL } from "../components/inquiry-progress"
import { inquiryKeys, useInquiries } from "../hooks/queries"
import { inquiryDate } from "../lib/format"
import {
    INQUIRY_STATUS_LABEL,
    type InquiryListQuery,
    type InquiryStatus,
    type ProcurementInquiry,
    type SalesInquiry,
} from "../types"

type InquiryRow = SalesInquiry | ProcurementInquiry

const PAGE_SIZE = 20
const STATUS_OPTIONS = Object.entries(INQUIRY_STATUS_LABEL)
    .filter(([value]) => value !== "awaiting_procurement")
    .map(([value, label]) => ({ value, label }))
const TASK_STATUS_OPTIONS = [
    { value: "open", label: "待我处理" },
    { value: "completed", label: "已处理" },
] as const
const EMPTY_FILTERS = { q: "", status: "", taskStatus: "" }
const DEFAULT_COLUMN_VISIBILITY = {
    inquiry_no: false,
    requirement_version: false,
    feedback_round: false,
}
const TABLE_CLASS_NAME =
    "[&_[data-column-id=actions]]:w-28 [&_[data-column-id=actions]]:min-w-28 max-sm:[&_[data-column-id='customer_name']]:min-w-36"

function useInquiryListFilters(procurement: boolean) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const rawStatus = searchParams.get("status")
    const rawTaskStatus = searchParams.get("task_status")
    const q = searchParams.get("q")?.trim() ?? ""
    const status =
        rawStatus === "awaiting_procurement"
            ? "procurement_processing"
            : (STATUS_OPTIONS.find((option) => option.value === rawStatus)
                  ?.value ?? "")
    const taskStatus = procurement
        ? TASK_STATUS_OPTIONS.find((option) => option.value === rawTaskStatus)
              ?.value
        : undefined
    const removeStatus = rawStatus !== null && !status
    const removeTaskStatus = rawTaskStatus !== null && !taskStatus
    const pageParam = searchParams.get("page")
    const rawPage = Number(pageParam)
    const removePage =
        pageParam !== null && (!Number.isSafeInteger(rawPage) || rawPage < 1)
    const page =
        !removeStatus && !removeTaskStatus && !removePage && pageParam !== null
            ? rawPage
            : 1
    const query: InquiryListQuery = {
        q: q || undefined,
        status: status || undefined,
        task_status: taskStatus,
        page,
        page_size: PAGE_SIZE,
    }
    const updateUrl = React.useCallback(
        (patch: Parameters<typeof patchUrl>[1]) =>
            patchUrl({ router, pathname, searchParams }, patch, {
                replace: true,
                scroll: false,
            }),
        [pathname, router, searchParams],
    )
    const form = useAppForm({
        defaultValues: { q, status, taskStatus: taskStatus ?? "" },
        onSubmit: ({ value, formApi }) => {
            const search = value.q.trim()
            formApi.reset({ ...value, q: search })
            updateUrl({
                q: search,
                status: value.status,
                task_status: procurement ? value.taskStatus : null,
                page: null,
            })
        },
    })
    const draft = useStore(form.store, (state) => state.values)

    React.useEffect(() => {
        form.reset({ q, status, taskStatus: taskStatus ?? "" })
    }, [form, q, status, taskStatus])

    React.useEffect(() => {
        if (removeStatus || removeTaskStatus || removePage) {
            updateUrl({
                ...(removeStatus ? { status: null } : {}),
                ...(removeTaskStatus ? { task_status: null } : {}),
                page: null,
            })
        }
    }, [removeStatus, removeTaskStatus, removePage, updateUrl])

    const clear = () => {
        form.reset(EMPTY_FILTERS)
        updateUrl({ q: null, status: null, task_status: null, page: null })
    }
    const changePage = React.useCallback(
        (next: number) => updateUrl({ page: next > 1 ? String(next) : null }),
        [updateUrl],
    )

    return {
        query,
        form,
        draft,
        clear,
        changePage,
        hasActiveFilters: Boolean(q || status || taskStatus),
        hasPendingChanges:
            draft.q.trim() !== q ||
            draft.status !== status ||
            draft.taskStatus !== (taskStatus ?? ""),
    }
}

function useInquiryListColumns(
    procurement: boolean,
    prefix: string,
    detailHref: ((row: InquiryRow) => string) | undefined,
) {
    return React.useMemo(() => {
        const columns: ColumnDef<InquiryRow, unknown>[] = [
            {
                accessorKey: "customer_name",
                header: "客户 / 询价单号",
                meta: { label: "客户 / 询价单号", width: "flex" },
                cell: ({ row: { original } }) => (
                    <TableIdentityCell
                        className="w-0 min-w-full"
                        primary={
                            <TableTextCell value={original.customer_name} />
                        }
                        primaryTitle={original.customer_name}
                        secondary={original.inquiry_no}
                        secondaryTitle={original.inquiry_no}
                        numericSecondary
                    />
                ),
            },
            {
                accessorKey: "status",
                header: "处理状态",
                meta: { label: "处理状态", width: "status" },
                cell: ({
                    row: {
                        original: { status },
                    },
                }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={INQUIRY_STATUS_LABEL[status] ?? "处理中"}
                        tone={inquiryStatusTone(status)}
                        appearance={status === "confirmed" ? "quiet" : "badge"}
                    />
                ),
            },
        ]
        if (procurement) {
            columns.push({
                id: "task_status",
                header: "我的处理进度",
                meta: { label: "我的处理进度", width: "status" },
                cell: ({ row: { original } }) => {
                    if (!("work_items" in original)) return <TableEmptyCell />
                    const pending = original.work_items.some(
                        (task) => task.status === "OPEN",
                    )
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={pending ? "待我处理" : "已处理"}
                            tone={pending ? "info" : "success"}
                            appearance={pending ? "badge" : "quiet"}
                        />
                    )
                },
            })
        } else {
            columns.push(
                {
                    id: "transfer_status",
                    header: "转单进度",
                    meta: { label: "转单进度", width: "status" },
                    cell: ({ row: { original } }) => {
                        if (!("sales_progress" in original))
                            return <TableEmptyCell />
                        const status = original.sales_progress.transfer_status
                        return (
                            <BusinessStatusBadge
                                context="list"
                                label={TRANSFER_LABEL[status]}
                                tone={
                                    status === "complete"
                                        ? "success"
                                        : status === "partial"
                                          ? "warning"
                                          : "neutral"
                                }
                                appearance={
                                    status === "partial" ? "badge" : "quiet"
                                }
                            />
                        )
                    },
                },
                {
                    id: "quote_validity",
                    header: "报价状态",
                    meta: { label: "报价状态", width: "status" },
                    cell: ({ row: { original } }) => {
                        if (!("sales_progress" in original))
                            return <TableEmptyCell />
                        const validity = original.sales_progress.quote_validity
                        return (
                            <BusinessStatusBadge
                                context="list"
                                label={VALIDITY_LABEL[validity]}
                                tone={
                                    validity === "valid" ? "success" : "neutral"
                                }
                                appearance={
                                    validity === "valid" ? "quiet" : "badge"
                                }
                            />
                        )
                    },
                },
            )
        }
        columns.push(
            {
                accessorKey: "deadline_at",
                header: "询价截止",
                meta: { label: "询价截止", numeric: true },
                cell: ({ row: { original } }) =>
                    original.deadline_at ? (
                        <TableTextCell
                            value={inquiryDate(original.deadline_at)}
                        />
                    ) : (
                        <TableEmptyCell variant="label" label="未设置" />
                    ),
            },
            {
                accessorKey: "inquiry_no",
                header: "询价单号",
                meta: { label: "询价单号", numeric: true },
                cell: ({ row: { original } }) => (
                    <TableTextCell value={original.inquiry_no} clamp={1} />
                ),
            },
            {
                accessorKey: "requirement_version",
                header: "需求版本",
                meta: {
                    label: "需求版本",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row: { original } }) => (
                    <TableTextCell
                        value={`第 ${original.requirement_version} 版`}
                        numeric
                    />
                ),
            },
            {
                accessorKey: "feedback_round",
                header: "反馈轮次",
                meta: {
                    label: "反馈轮次",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row: { original } }) =>
                    original.feedback_round ? (
                        <TableTextCell
                            value={`第 ${original.feedback_round} 轮`}
                            numeric
                        />
                    ) : (
                        <TableEmptyCell variant="label" label="待首次反馈" />
                    ),
            },
        )
        if (detailHref) {
            columns.push({
                id: "actions",
                header: "操作",
                meta: { label: "操作", align: "end" },
                cell: ({ row: { original } }) => {
                    const rowId = `${prefix}-${toAutomationIdSegment(original.id)}`
                    return (
                        <TableRowActions
                            moreId={`${rowId}-more`}
                            moreLabel={`${original.inquiry_no} 更多操作`}
                            maxInline={2}
                            actions={[
                                {
                                    id: `${rowId}-detail`,
                                    label: (
                                        <>
                                            查看详情
                                            <span className="sr-only">
                                                ：{original.inquiry_no}
                                            </span>
                                        </>
                                    ),
                                    href: detailHref(original),
                                },
                            ]}
                        />
                    )
                },
            })
        }
        return columns
    }, [procurement, prefix, detailHref])
}

function inquiryStatusTone(status: InquiryStatus) {
    switch (status) {
        case "draft":
            return "neutral"
        case "confirmed":
            return "success"
        case "closed":
            return "void"
        case "awaiting_sales_supplement":
        case "awaiting_sales":
            return "warning"
        default:
            return "info"
    }
}

function InquiryListWorkspace({
    procurement,
    root,
    prefix,
    title,
    headerActions,
    canCreate,
    canImport,
    canOpenDetail,
}: {
    procurement: boolean
    root: string
    prefix: string
    title: string
    headerActions: React.ReactNode
    canCreate: boolean
    canImport: boolean
    canOpenDetail: boolean
}) {
    const router = useRouter()
    const queryClient = useQueryClient()
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    useSearchHotkey(searchInputRef)
    const filters = useInquiryListFilters(procurement)
    const list = useInquiries(filters.query, procurement)
    const detailHref = React.useCallback(
        (row: InquiryRow) => {
            const task =
                procurement && "work_items" in row
                    ? row.work_items[0]
                    : undefined
            return `${root}/${encodeURIComponent(row.id)}${task ? `?task_id=${encodeURIComponent(task.id)}` : ""}`
        },
        [procurement, root],
    )
    const columns = useInquiryListColumns(
        procurement,
        prefix,
        canOpenDetail ? detailHref : undefined,
    )
    const defaultColumnOrder = [
        "customer_name",
        "status",
        ...(procurement
            ? ["task_status"]
            : ["transfer_status", "quote_validity"]),
        "deadline_at",
        "inquiry_no",
        "requirement_version",
        "feedback_round",
        ...(canOpenDetail ? ["actions"] : []),
    ]
    const noScope = list.data?.no_scope === true
    const listFailed = list.isError || (!list.isPending && !list.data)
    const scopeChanged = isDataScopeChanged(list.error)
    const resetScope = scopeChanged && filters.query.page > 1
    const total = list.data?.total ?? 0
    const pagination = React.useMemo<PaginationState>(
        () => ({ pageIndex: filters.query.page - 1, pageSize: PAGE_SIZE }),
        [filters.query.page],
    )
    const { changePage } = filters
    const resetEmptyPage =
        filters.query.page > 1 &&
        list.isSuccess &&
        !list.isPlaceholderData &&
        !list.isFetching &&
        total === 0
    React.useEffect(() => {
        if (resetEmptyPage) changePage(1)
    }, [changePage, resetEmptyPage])
    const handlePaginationChange = React.useCallback(
        ({ pageIndex }: PaginationState) => {
            // 上一次查询的总数不能修正当前 URL 页码。
            if (!list.isPlaceholderData) changePage(pageIndex + 1)
        },
        [changePage, list.isPlaceholderData],
    )
    const openCreate = () => router.push("/sales/inquiries/new")
    const retryList = () => {
        if (resetScope) {
            void queryClient.invalidateQueries({
                queryKey: inquiryKeys.list(
                    { ...filters.query, page: 1 },
                    procurement,
                ),
                exact: true,
            })
            filters.changePage(1)
        } else {
            void list.refetch()
        }
    }

    return (
        <ListWorkspacePage
            title={title}
            headerActions={headerActions}
            loading={list.isPending}
            resultsHeadingId={`${prefix}-results-heading`}
            resultsLabel={
                listFailed || list.isFetching
                    ? "询价列表"
                    : `询价列表 · ${total} 条结果`
            }
            surface={{
                ariaLabel: "询价列表",
                toolbar: (
                    <ListWorkspaceFilterBar
                        idPrefix={`${prefix}-toolbar`}
                        formAriaLabel="询价列表查询"
                        morePresentation="popover"
                        onSubmit={() => void filters.form.handleSubmit()}
                        queryButtonId={`${prefix}-query`}
                        hasPendingChanges={filters.hasPendingChanges}
                        resultStatus={
                            listFailed
                                ? "查询未完成"
                                : list.isFetching
                                  ? "查询中…"
                                  : null
                        }
                        search={
                            <ListSearchField
                                id={`${prefix}-search`}
                                searchInputRef={searchInputRef}
                                aria-label="搜索询价单号或客户名称"
                                placeholder="搜索单号或客户名称"
                                value={filters.draft.q}
                                onChange={(value) =>
                                    filters.form.setFieldValue("q", value)
                                }
                            />
                        }
                        primaryFilters={
                            <>
                                {procurement && (
                                    <OptionCombobox
                                        id={`${prefix}-task-status`}
                                        aria-label="我的处理进度"
                                        filterLabel="我的进度"
                                        placeholder="全部"
                                        className="w-44 max-w-full min-w-0"
                                        value={filters.draft.taskStatus}
                                        options={TASK_STATUS_OPTIONS}
                                        onValueChange={(value) =>
                                            filters.form.setFieldValue(
                                                "taskStatus",
                                                value ?? "",
                                            )
                                        }
                                    />
                                )}
                                <OptionCombobox
                                    id={`${prefix}-status`}
                                    aria-label="处理状态"
                                    filterLabel="处理状态"
                                    placeholder="全部"
                                    className="w-44 max-w-full min-w-0"
                                    value={filters.draft.status}
                                    options={STATUS_OPTIONS}
                                    onValueChange={(value) =>
                                        filters.form.setFieldValue(
                                            "status",
                                            value ?? "",
                                        )
                                    }
                                />
                            </>
                        }
                        extraPrimary={
                            <Button
                                id={`${prefix}-reset`}
                                variant="ghost"
                                type="button"
                                className={listWorkspaceStyles.quietButton}
                                onClick={filters.clear}
                            >
                                {listFilterText.reset}
                            </Button>
                        }
                    />
                ),
                tableClassName: TABLE_CLASS_NAME,
                table: (
                    <ListDataTable
                        idPrefix={prefix}
                        entityName="询价"
                        caption={title}
                        data={list.data?.items ?? []}
                        columns={columns}
                        defaultColumnVisibility={DEFAULT_COLUMN_VISIBILITY}
                        defaultColumnOrder={defaultColumnOrder}
                        defaultColumnPinning={{
                            left: ["customer_name"],
                            right: canOpenDetail ? ["actions"] : [],
                        }}
                        getRowId={(row) => row.id}
                        rowLabel={(row) =>
                            `${row.inquiry_no} ${row.customer_name}`
                        }
                        showToolbarSummary={false}
                        showPagination={!listFailed}
                        rowCount={listFailed ? 0 : total}
                        pagination={pagination}
                        pageSizeOptions={[PAGE_SIZE]}
                        onPaginationChange={handlePaginationChange}
                        onRowOpen={
                            canOpenDetail
                                ? (row) => router.push(detailHref(row))
                                : undefined
                        }
                        loading={list.isFetching}
                        noScope={noScope}
                        hasActiveFilters={filters.hasActiveFilters}
                        onClearFilters={noScope ? undefined : filters.clear}
                        canCreate={canCreate}
                        onCreate={canCreate ? openCreate : undefined}
                        createLabel="新建询价"
                        emptyCopy={{
                            emptyTitle: "当前没有询价",
                            emptyDescription: procurement
                                ? "当前没有分配给您的询价，也没有已处理的记录。"
                                : canImport
                                  ? "可导入客户选品创建草稿，或新建询价后手动添加商品。"
                                  : canCreate
                                    ? "新建询价后可手动添加商品。"
                                    : "当前范围内还没有询价记录。",
                        }}
                        errorState={
                            listFailed ? (
                                <BusinessFailureState
                                    id={`${prefix}-retry`}
                                    title="询价列表加载失败"
                                    error={list.error ?? undefined}
                                    onRetry={retryList}
                                    action={
                                        scopeChanged ? (
                                            <Button
                                                id={`${prefix}-retry-retry`}
                                                type="button"
                                                variant="outline"
                                                size="sm"
                                                onClick={retryList}
                                            >
                                                {resetScope
                                                    ? "从第一页重新加载"
                                                    : "重试"}
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                    />
                ),
            }}
        />
    )
}

export function SalesInquiriesPage({
    procurement = false,
}: {
    procurement?: boolean
}) {
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const root = procurement ? "/procurement/inquiries" : "/sales/inquiries"
    const prefix = procurement ? "procurement-inquiries" : "sales-inquiries"
    const title = procurement ? "采购询价" : "销售询价"
    const canList = hasPermission(
        permissions,
        procurement ? "procurement_inquiry:list" : "sales_inquiry:list",
    )
    const canSwitch = hasPermission(
        permissions,
        procurement ? "sales_inquiry:list" : "procurement_inquiry:list",
    )
    const canOpenDetail = hasPermission(
        permissions,
        procurement ? "procurement_inquiry:detail" : "sales_inquiry:detail",
    )
    const canCreate =
        !procurement &&
        hasPermission(permissions, "sales_inquiry:create") &&
        hasPermission(permissions, "sellable_sku:list")
    const canImport =
        canCreate && hasPermission(permissions, "sales_inquiry:import")
    const headerActions = (canSwitch || canCreate) && (
        <>
            {canSwitch && (
                <Link
                    id={`${prefix}-switch`}
                    data-slot="button"
                    className={buttonVariants({
                        variant: "ghost",
                        size: "sm",
                        className: listWorkspaceStyles.quietButton,
                    })}
                    href={
                        procurement
                            ? "/sales/inquiries"
                            : "/procurement/inquiries"
                    }
                >
                    {procurement ? "销售询价" : "采购询价"}
                </Link>
            )}
            {canImport && profile.data && (
                <InquiryCreateImport userId={profile.data.userid} />
            )}
            {canCreate && (
                <Link
                    id="sales-inquiries-create"
                    data-slot="button"
                    className={buttonVariants({ size: "sm" })}
                    href="/sales/inquiries/new"
                >
                    <PlusIcon data-icon="inline-start" aria-hidden="true" />
                    新建询价
                </Link>
            )}
        </>
    )

    if (!profile.data) {
        return (
            <ListWorkspacePage
                title={title}
                loading={profile.isPending}
                alerts={
                    profile.isError ? (
                        <BusinessFailureState
                            idPrefix={`${prefix}-account`}
                            title="账号权限读取失败"
                            error={profile.error}
                            onRetry={() => void profile.refetch()}
                        />
                    ) : undefined
                }
            />
        )
    }
    if (!canList) {
        return (
            <ListWorkspacePage
                title={title}
                headerActions={headerActions || undefined}
                alerts={
                    <BusinessFailureState
                        kind="permission"
                        title={`当前账号无权查看${title}列表`}
                    />
                }
            />
        )
    }
    return (
        <InquiryListWorkspace
            key={prefix}
            procurement={procurement}
            root={root}
            prefix={prefix}
            title={title}
            headerActions={headerActions || undefined}
            canCreate={canCreate}
            canImport={canImport}
            canOpenDetail={canOpenDetail}
        />
    )
}
