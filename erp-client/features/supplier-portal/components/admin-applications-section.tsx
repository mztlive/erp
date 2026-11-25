"use client"
import { useCallback, useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { ArrowUpRightIcon } from "lucide-react"
import { useRouter } from "@/lib/navigation"
import {
    DataTable,
    DetailInlineTable,
    DetailWorkspaceSection,
    OptionCombobox,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    kindLabels,
    portalApplicationStatus,
    statusLabels,
    timeLabel,
} from "@/features/supplier-portal/lib/presentation"
import type { PortalApplication } from "@/features/supplier-portal/types"
import {
    usePortalAdminApplications,
    usePortalAdminAccess,
} from "../hooks/admin"
export function SupplierApplicationsSection({
    supplierId,
    onNavigateAway,
}: {
    supplierId: string
    onNavigateAway?: (href: string) => void
}) {
    const router = useRouter()
    const access = usePortalAdminAccess()
    const [status, setStatus] = useState("")
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const query = usePortalAdminApplications(
        {
            supplier_id: supplierId,
            status,
            page: pagination.pageIndex + 1,
            page_size: pagination.pageSize,
        },
        access.can("supplier_portal_request:list") && !!supplierId,
    )
    const openApplication = useCallback(
        (id: string) => {
            const href = `/procurement/supplier-portal/applications/${encodeURIComponent(id)}`
            if (onNavigateAway) {
                onNavigateAway(href)
                return
            }
            router.push(href)
        },
        [onNavigateAway, router],
    )
    const columns = useMemo<ColumnDef<PortalApplication>[]>(
        () => [
            {
                id: "title",
                header: "申请内容",
                meta: { label: "申请内容", width: "flex" },
                accessorFn: (row) =>
                    row.title ??
                    (row.input.name as string) ??
                    kindLabels[row.kind],
            },
            {
                accessorKey: "kind",
                header: "类型",
                meta: { label: "类型", width: "default" },
                cell: ({ row }) =>
                    kindLabels[row.original.kind] ?? "供应合作申请",
            },
            {
                accessorKey: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <StatusBadge
                        {...portalApplicationStatus(row.original.status)}
                    />
                ),
            },
            {
                accessorKey: "reason",
                header: "申请原因",
                meta: { label: "申请原因", width: "reference" },
            },
            {
                accessorKey: "updated_at",
                header: "更新时间",
                meta: { label: "更新时间", width: "reference" },
                cell: ({ row }) => (
                    <span className="num">
                        {timeLabel(
                            row.original.updated_at ?? row.original.created_at,
                        )}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default" },
                enableHiding: false,
                cell: ({ row }) => {
                    const href = `/procurement/supplier-portal/applications/${encodeURIComponent(row.original.id)}`
                    return (
                        <Button
                            id={`supplier-portal-admin-request-${toAutomationIdSegment(row.original.id)}`}
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.inlineLinkButton}
                            render={
                                <Link
                                    href={href}
                                    onClick={(event) => {
                                        if (
                                            !onNavigateAway ||
                                            event.metaKey ||
                                            event.ctrlKey ||
                                            event.shiftKey ||
                                            event.altKey
                                        ) {
                                            return
                                        }
                                        event.preventDefault()
                                        onNavigateAway(href)
                                    }}
                                />
                            }
                        >
                            查看与审核
                            <ArrowUpRightIcon
                                data-icon="inline-end"
                                className="size-3.5"
                                aria-hidden="true"
                            />
                        </Button>
                    )
                },
            },
        ],
        [onNavigateAway],
    )
    return (
        <DetailWorkspaceSection
            headingId="supplier-applications-title"
            title="供应商申请"
            width="full"
            divider={false}
            badge={
                <span className="num rounded-sm bg-muted/80 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                    共 {query.data?.total ?? 0} 条申请
                </span>
            }
            actions={
                <OptionCombobox
                    filterLabel="申请状态"
                    className="w-48 max-w-full"
                    id="supplier-portal-admin-applications-status"
                    aria-label="申请状态"
                    value={status || null}
                    onValueChange={(value) => {
                        setStatus(value ?? "")
                        setPagination((p) => ({
                            ...p,
                            pageIndex: 0,
                        }))
                    }}
                    options={Object.entries(statusLabels).map(
                        ([value, label]) => ({ value, label }),
                    )}
                    placeholder="全部状态"
                />
            }
        >
            <DetailInlineTable frame="scroll">
                <DataTable
                    id="supplier-portal-admin-applications-table"
                    data={query.data?.items ?? []}
                    columns={columns}
                    getRowId={(row) => row.id}
                    pagination={pagination}
                    onPaginationChange={setPagination}
                    rowCount={query.data?.total ?? 0}
                    loading={query.isFetching}
                    onRowOpen={(row) => openApplication(row.id)}
                    errorState={
                        query.isError ? (
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id="supplier-portal-admin-requests-retry"
                            />
                        ) : undefined
                    }
                    emptyTitle="此供应商暂无可查看的申请。"
                    showPagination={
                        (query.data?.total ?? 0) > pagination.pageSize ||
                        pagination.pageIndex > 0
                    }
                />
            </DetailInlineTable>
        </DetailWorkspaceSection>
    )
}
