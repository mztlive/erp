"use client"
import { useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"
import { useRouter } from "next/navigation"
import { OptionCombobox } from "@/components/business/option-combobox"
import { DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PortalError } from "@/features/supplier-portal/components/surface"
import {
    kindLabels,
    statusLabels,
    timeLabel,
} from "@/features/supplier-portal/lib/presentation"
import type { PortalApplication } from "@/features/supplier-portal/types"
import { usePortalAdminApplications, usePortalAdminAccess } from "../hooks"
import { PortalAdminFrame } from "../components/admin-frame"
export function PortalAdminApplicationsPage() {
    const router = useRouter()
    const access = usePortalAdminAccess()
    const [supplierId, setSupplierId] = useState("")
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
                accessorFn: (row) =>
                    row.title ??
                    (row.input.name as string) ??
                    kindLabels[row.kind],
            },
            {
                accessorKey: "kind",
                header: "类型",
                cell: ({ row }) =>
                    kindLabels[row.original.kind] ?? "供应合作申请",
            },
            {
                accessorKey: "status",
                header: "状态",
                cell: ({ row }) =>
                    statusLabels[row.original.status] ?? "待核对",
            },
            { accessorKey: "reason", header: "申请原因" },
            {
                accessorKey: "updated_at",
                header: "更新时间",
                cell: ({ row }) =>
                    timeLabel(
                        row.original.updated_at ?? row.original.created_at,
                    ),
            },
            {
                id: "actions",
                header: "操作",
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
    if (
        !access.can("supplier_portal_request:list") &&
        !access.profile.isPending
    )
        return (
            <PortalAdminFrame title="供应商申请">
                <p>
                    当前账号没有供应商申请列表权限，具体审核任务从工作台进入。
                </p>
            </PortalAdminFrame>
        )
    if (!access.can("supplier:list"))
        return (
            <PortalAdminFrame title="供应商申请">
                <p className="text-sm">
                    需供应商列表查看权限才能选择供应商；请联系管理员配置资格，具体审核可从工作台进入。
                </p>
            </PortalAdminFrame>
        )
    return (
        <PortalAdminFrame title="供应商申请">
            <ListWorkSurface
                ariaLabel="供应商申请列表"
                toolbar={
                    <div className="w-full max-w-lg space-y-2">
                        <label
                            htmlFor="supplier-portal-requests-filter-supplier"
                            className="text-sm"
                        >
                            查看供应商申请
                        </label>
                        <SupplierSearchCombobox
                            id="supplier-portal-requests-filter-supplier"
                            value={supplierId || undefined}
                            onValueChange={(value) => {
                                setSupplierId(value ?? "")
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                        />
                        <OptionCombobox
                            id="supplier-portal-admin-applications-status"
                            aria-label="申请状态"
                            value={status || null}
                            onValueChange={(value) => {
                                setStatus(value ?? "")
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                            options={Object.entries(statusLabels).map(
                                ([value, label]) => ({ value, label }),
                            )}
                            placeholder="全部状态"
                        />
                    </div>
                }
                selectionBar={
                    <span className="text-sm">
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
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id="supplier-portal-admin-requests-retry"
                            />
                        }
                        emptyState={
                            <p className="p-6 text-sm">
                                {supplierId
                                    ? "此供应商暂无可查看的申请。"
                                    : "请选择供应商查看申请。"}
                            </p>
                        }
                    />
                }
            />
        </PortalAdminFrame>
    )
}
