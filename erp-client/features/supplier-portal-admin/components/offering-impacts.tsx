"use client"

import {
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business/detail-workspace"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { DataTable } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePathname, useSearchParams } from "@/lib/navigation"
import { PO_STATUS_LABEL } from "@/features/purchase-orders/types"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { usePortalAdminAccess, usePortalOfferingImpacts } from "../hooks"
import type { PortalPurchaseImpact } from "../api"

const warningLabels: Record<string, string> = {
    SUPPLY_AVAILABILITY_UNKNOWN: "当前可供情况待核对",
    SUPPLIER_STOPPED: "供应商已停止供应",
    SUPPLY_UNAVAILABLE: "供应商当前缺货或不可供",
    SUPPLY_AVAILABILITY_STALE: "当前可供情况已过期",
    SUPPLY_ZERO_INVENTORY: "供应商当前库存为零",
}
const purchaseStatusLabels: Record<string, string> = {
    ...PO_STATUS_LABEL,
    IN_APPROVAL: PO_STATUS_LABEL.PENDING_REVIEW,
    PENDING_FINANCE_REVIEW: PO_STATUS_LABEL.PENDING_REVIEW,
    PARTIALLY_EXECUTED: PO_STATUS_LABEL.PARTIAL,
    VOIDED: PO_STATUS_LABEL.VOID,
}

function purchaseStatusTone(
    status: string,
): "neutral" | "info" | "success" | "warning" {
    switch (status) {
        case "PENDING_REVIEW":
        case "IN_APPROVAL":
        case "PENDING_FINANCE_REVIEW":
            return "warning"
        case "APPROVED":
        case "SENT":
        case "CONFIRMED":
        case "PARTIAL":
        case "PARTIALLY_EXECUTED":
            return "info"
        case "COMPLETED":
            return "success"
        default:
            return "neutral"
    }
}

const defaultPagination: PaginationState = { pageIndex: 0, pageSize: 25 }

/** 当前供给情况只产生履约提示，不据此改写历史采购价格、付款或状态。 */
export function PortalOfferingImpacts({
    offeringId,
    enabled,
}: {
    offeringId: string
    enabled: boolean
}) {
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const access = usePortalAdminAccess()
    const canViewOrder = access.can("purchase_order:detail")
    const currentHref = searchParams.toString()
        ? `${pathname}?${searchParams.toString()}`
        : pathname
    const [selection, setSelection] = useState({
        offeringId,
        pagination: defaultPagination,
    })
    const pagination =
        selection.offeringId === offeringId
            ? selection.pagination
            : defaultPagination
    const prefix = `supplier-portal-offering-impacts-${toAutomationIdSegment(offeringId)}`
    const params = {
        page: pagination.pageIndex + 1,
        page_size: pagination.pageSize,
    }
    const query = usePortalOfferingImpacts(offeringId, params, enabled)
    const columns = useMemo<ColumnDef<PortalPurchaseImpact>[]>(
        () => [
            {
                accessorKey: "purchase_no",
                header: "采购单号",
                enableHiding: false,
                cell: ({ row }) => {
                    const label = row.original.purchase_no || "单号待核对"
                    if (!canViewOrder || !row.original.purchase_order_id) {
                        return <span className="num font-medium">{label}</span>
                    }
                    const href = `/procurement/orders/${encodeURIComponent(row.original.purchase_order_id)}?${new URLSearchParams({ returnTo: currentHref }).toString()}`
                    return (
                        <Link
                            id={`${prefix}-po-${toAutomationIdSegment(row.original.purchase_order_id)}`}
                            href={href}
                            className="num font-medium text-primary underline-offset-4 hover:underline"
                        >
                            {label}
                        </Link>
                    )
                },
            },
            {
                accessorKey: "status",
                header: "采购状态",
                cell: ({ row }) => (
                    <StatusBadge
                        tone={purchaseStatusTone(row.original.status)}
                        label={
                            purchaseStatusLabels[row.original.status] ??
                            "待核对"
                        }
                    />
                ),
            },
            {
                accessorKey: "owner_name",
                header: "当前采购负责人",
                cell: ({ row }) => row.original.owner_name?.trim() || "待核对",
            },
            {
                id: "source-line-count",
                header: "关联采购明细行数",
                accessorFn: (row) => row.lines.length,
                meta: { numeric: true, align: "end" },
            },
            {
                id: "open-task-count",
                header: "待处理履约任务数",
                accessorFn: (row) => row.tasks.length,
                meta: { numeric: true, align: "end" },
            },
            {
                id: "tasks",
                header: "待处理履约任务",
                enableHiding: false,
                cell: ({ row }) =>
                    row.original.tasks.length ? (
                        <div className="flex flex-wrap gap-x-3 gap-y-2">
                            {row.original.tasks.map((task, index) => (
                                <Link
                                    key={task.id}
                                    id={`${prefix}-task-${toAutomationIdSegment(task.id)}`}
                                    href={buildWorkspaceDetailHref(
                                        "tasks",
                                        task.id,
                                        {
                                            returnTo: `/workspace/tasks?view=managed&purchaseOrderIds=${encodeURIComponent(row.original.purchase_order_id)}`,
                                        },
                                    )}
                                    className="rounded-sm text-primary underline underline-offset-4"
                                >
                                    核对相关任务（
                                    <span className="num">{index + 1}</span>）
                                </Link>
                            ))}
                        </div>
                    ) : (
                        <span className="text-muted-foreground">
                            暂无待处理履约任务
                        </span>
                    ),
            },
        ],
        [canViewOrder, currentHref, prefix],
    )
    if (!enabled || !offeringId) return null
    return (
        <DetailWorkspaceSection
            title="当前采购与履约影响"
            subtitle={
                query.data ? (
                    <span className="num">
                        共 {query.data.total} 张当前可查看的关联采购单
                    </span>
                ) : undefined
            }
            actions={
                <Button
                    id={`${prefix}-refresh`}
                    type="button"
                    variant="ghost"
                    className={detailWorkspaceStyles.subtleButton}
                    size="sm"
                    disabled={query.isFetching}
                    onClick={() => void query.refetch()}
                >
                    重新核对影响
                </Button>
            }
        >
            <p className="text-xs text-muted-foreground">
                表内仅显示当前账号可查看的关联采购单与待处理任务。请核对采购负责人及未完成履约情况；已生效的历史采购单价格、付款条件及供货关系保持原值。
            </p>
            {query.data && !query.error && (
                <div className="space-y-2">
                    {query.data.warning ? (
                        <div
                            role="status"
                            className="space-y-1 border-l-2 border-warning-soft-foreground/60 bg-warning-soft/25 px-3.5 py-2.5 text-body-compact"
                        >
                            <p className="font-medium">
                                {warningLabels[query.data.warning.code] ??
                                    "当前供给情况需要核对"}
                            </p>
                            <p className="text-xs text-muted-foreground">
                                请由当前采购负责人逐单核验未完成履约，另行处理缺货或停供影响；此提示不会调整旧单金额、付款条件或采购状态。
                            </p>
                        </div>
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            当前可供情况未触发缺货或停供提示，仍须按各采购单的真实履约进度核对。
                        </p>
                    )}
                    <p className="text-xs text-muted-foreground">
                        {query.data.association_notice ||
                            "采购关联范围说明暂缺，请重新核对后确认。"}
                    </p>
                </div>
            )}
            <div className={detailWorkspaceStyles.dataTableWrapper}>
                <DataTable
                    id={`${prefix}-table`}
                    caption="当前供货商品关联的采购单及待处理履约任务"
                    data={query.data?.items ?? []}
                    columns={columns}
                    getRowId={(row) => row.purchase_order_id}
                    rowLabel={(row) => row.purchase_no}
                    pagination={pagination}
                    onPaginationChange={(next) =>
                        setSelection({ offeringId, pagination: next })
                    }
                    rowCount={query.data?.total ?? 0}
                    loading={query.isFetching}
                    errorState={
                        query.error ? (
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id={`${prefix}-retry`}
                            />
                        ) : undefined
                    }
                    emptyState={
                        <p className="p-6 text-xs text-muted-foreground">
                            当前权限范围内暂未查到受影响的未完成采购单；如存在早期历史采购单，仍需人工核对。
                        </p>
                    }
                />
            </div>
        </DetailWorkspaceSection>
    )
}
