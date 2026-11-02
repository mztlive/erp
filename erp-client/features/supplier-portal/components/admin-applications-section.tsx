"use client"
import { useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { useRouter } from "@/lib/navigation"
import { OptionCombobox } from "@/components/business/option-combobox"
import { DataTable } from "@/components/business"
import { ListToolbar } from "@/components/business/list"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    kindLabels,
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
}: {
    supplierId: string
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
                cell: ({ row }) =>
                    statusLabels[row.original.status] ?? "待核对",
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
                cell: ({ row }) =>
                    timeLabel(
                        row.original.updated_at ?? row.original.created_at,
                    ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "default" },
                enableHiding: false,
                cell: ({ row }) => (
                    <Link
                        id={`supplier-portal-admin-request-${toAutomationIdSegment(row.original.id)}`}
                        href={`/procurement/supplier-portal/applications/${encodeURIComponent(row.original.id)}`}
                        className="text-primary"
                    >
                        查看与审核
                    </Link>
                ),
            },
        ],
        [],
    )
    return (
        <section
            className="space-y-4"
            aria-labelledby="supplier-applications-title"
        >
            <div className="space-y-1">
                <h2
                    id="supplier-applications-title"
                    className="text-base font-semibold"
                >
                    供应商申请
                </h2>
                <p className="text-sm text-muted-foreground">
                    查看当前供应商的报价、商品及合作资料申请；待处理事项也可从工作台进入。
                </p>
            </div>
            <ListWorkSurface
                ariaLabel="供应商申请列表"
                toolbar={
                    <ListToolbar
                        aria-label="供应商申请筛选"
                        filters={
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
                    />
                }
                selectionBar={
                    <span className="text-xs text-muted-foreground">
                        共 {query.data?.total ?? 0} 条申请
                    </span>
                }
                table={
                    <DataTable
                        id="supplier-portal-admin-applications-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        rowCount={query.data?.total ?? 0}
                        loading={query.isFetching}
                        onRowOpen={(row) =>
                            router.push(
                                `/procurement/supplier-portal/applications/${encodeURIComponent(row.id)}`,
                            )
                        }
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
                }
            />
        </section>
    )
}
