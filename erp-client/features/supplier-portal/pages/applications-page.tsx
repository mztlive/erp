"use client"

import { useState } from "react"
import type { ColumnDef } from "@tanstack/react-table"
import { useRouter } from "next/navigation"

import {
    BusinessStatusBadge,
    ListDataTable,
    ListWorkSurface,
    OptionCombobox,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import type { StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { PortalListSearchForm } from "../components/portal-list-search-form"
import { PortalSurface } from "../components/surface"
import { usePortalApplications } from "../hooks/queries"
import { kindLabels, statusLabels, timeLabel } from "../lib/presentation"
import type { PortalApplication } from "../types"

const PREFIX = "supplier-portal-applications"
const SEARCH_ID = `${PREFIX}-search`
const statusOptions = Object.entries(statusLabels).map(([value, label]) => ({
    value,
    label,
}))
const statusTones: Record<PortalApplication["status"], StatusTone> = {
    draft: "neutral",
    pending: "warning",
    returned: "warning",
    withdrawn: "neutral",
    effective: "success",
}

function applicationTitle(row: PortalApplication) {
    return (
        row.title?.trim() ||
        (typeof row.input.name === "string" && row.input.name.trim()) ||
        kindLabels[row.kind] ||
        "供应合作申请"
    )
}

function applicationHref(id: string) {
    return `/supplier-portal/applications/${encodeURIComponent(id)}`
}

const columns: ColumnDef<PortalApplication>[] = [
    {
        id: "title",
        header: "申请内容",
        accessorFn: applicationTitle,
        meta: { width: "flex" },
        cell: ({ row }) => {
            const title = applicationTitle(row.original)
            const applicationNo = row.original.application_no
            return (
                <TableIdentityCell
                    primary={title}
                    primaryTitle={title}
                    secondary={applicationNo}
                    secondaryTitle={applicationNo}
                    numericSecondary
                />
            )
        },
    },
    {
        accessorKey: "kind",
        header: "申请类型",
        meta: { width: "default" },
        cell: ({ row }) => (
            <TableTextCell
                value={kindLabels[row.original.kind] ?? "供应合作申请"}
            />
        ),
    },
    {
        accessorKey: "status",
        header: "当前状态",
        meta: { width: "status" },
        cell: ({ row }) => {
            const { status } = row.original
            return (
                <BusinessStatusBadge
                    context="list"
                    label={statusLabels[status] ?? "待核对"}
                    tone={statusTones[status] ?? "neutral"}
                    appearance={status === "effective" ? "quiet" : "badge"}
                />
            )
        },
    },
    {
        accessorKey: "reason",
        header: "申请原因",
        meta: { width: "reference" },
        cell: ({ row }) => (
            <TableTextCell
                value={row.original.reason}
                clamp={2}
                className="wrap-anywhere whitespace-normal"
            />
        ),
    },
    {
        accessorKey: "updated_at",
        header: "更新时间",
        meta: { width: "default" },
        cell: ({ row }) => (
            <TableTextCell
                value={timeLabel(
                    row.original.updated_at ?? row.original.created_at,
                )}
                numeric
            />
        ),
    },
    {
        id: "actions",
        header: "操作",
        enableHiding: false,
        meta: { width: "status", align: "end" },
        cell: ({ row }) => {
            const rowId = toAutomationIdSegment(row.original.id)
            return (
                <TableRowActions
                    moreId={`supplier-portal-application-more-${rowId}`}
                    moreLabel={`${applicationTitle(row.original)}的更多操作`}
                    actions={[
                        {
                            id: `supplier-portal-application-open-${rowId}`,
                            href: applicationHref(row.original.id),
                            label: "查看与处理",
                        },
                    ]}
                />
            )
        },
    },
]

export function PortalApplicationsPage() {
    const router = useRouter()
    const [search, setSearch] = useState("")
    const [q, setQ] = useState("")
    const [status, setStatus] = useState("")
    const [statusDraft, setStatusDraft] = useState("")
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const query = usePortalApplications({
        q,
        status,
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    })
    const resetPage = () => setPagination((page) => ({ ...page, pageIndex: 0 }))

    return (
        <PortalSurface variant="list" title="我的申请">
            <ListWorkSurface
                ariaLabel="我的申请"
                toolbar={
                    <PortalListSearchForm
                        searchId={SEARCH_ID}
                        searchLabel="搜索申请"
                        placeholder="搜索申请"
                        value={search}
                        onChange={setSearch}
                        submitId={`${PREFIX}-search-submit`}
                        hasPendingChanges={
                            search.trim() !== q || statusDraft !== status
                        }
                        resultStatus={
                            query.isError
                                ? "查询未完成"
                                : query.isFetching
                                  ? "查询中…"
                                  : null
                        }
                        onSubmit={() => {
                            setQ(search.trim())
                            setStatus(statusDraft)
                            resetPage()
                        }}
                    >
                        <OptionCombobox
                            id={`${PREFIX}-status`}
                            aria-label="申请状态"
                            className="w-40 max-w-full min-w-0"
                            filterLabel="状态"
                            placeholder="全部"
                            value={statusDraft || null}
                            onValueChange={(value) =>
                                setStatusDraft(value ?? "")
                            }
                            options={statusOptions}
                        />
                    </PortalListSearchForm>
                }
                tableClassName="max-md:[&_[data-column-pinned]]:inset-x-auto! max-md:[&_[data-pinned-edge]]:shadow-none!"
                table={
                    <ListDataTable
                        id={`${PREFIX}-table`}
                        entityName="申请"
                        caption="我的申请"
                        data={query.data?.items ?? []}
                        columns={columns}
                        defaultColumnOrder={[
                            "title",
                            "kind",
                            "status",
                            "reason",
                            "updated_at",
                            "actions",
                        ]}
                        defaultColumnPinning={{
                            left: ["title"],
                            right: ["actions"],
                        }}
                        getRowId={(row) => row.id}
                        rowLabel={applicationTitle}
                        rowCount={query.data?.total ?? 0}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        showToolbarSummary={false}
                        loading={query.isFetching}
                        onRowOpen={(row) =>
                            router.push(applicationHref(row.id))
                        }
                        isError={query.isError}
                        error={query.error}
                        onRetry={() => void query.refetch()}
                        retryButtonId={`${PREFIX}-retry`}
                        hasActiveFilters={q !== "" || status !== ""}
                        onClearFilters={() => {
                            setSearch("")
                            setQ("")
                            setStatus("")
                            setStatusDraft("")
                            resetPage()
                            document.getElementById(SEARCH_ID)?.focus()
                        }}
                        clearFiltersButtonId={`${PREFIX}-empty-clear-filters`}
                        emptyCopy={{
                            emptyTitle: "暂无申请",
                            emptyDescription:
                                "已有商品报价和新品提报均可创建草稿。",
                        }}
                    />
                }
            />
        </PortalSurface>
    )
}
