"use client"

import { useMemo, useState } from "react"
import Link from "next/link"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PO_STATUS_LABEL } from "@/features/purchase-orders/types"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { usePortalOfferingImpacts } from "../hooks"
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
const defaultPagination: PaginationState = { pageIndex: 0, pageSize: 25 }

/** 当前供给情况只产生履约提示，不据此改写历史采购价格、付款或状态。 */
export function PortalOfferingImpacts({
    offeringId,
    enabled,
}: {
    offeringId: string
    enabled: boolean
}) {
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
                cell: ({ row }) => (
                    <span className="num font-medium">
                        {row.original.purchase_no || "单号待核对"}
                    </span>
                ),
            },
            {
                accessorKey: "status",
                header: "采购状态",
                cell: ({ row }) =>
                    purchaseStatusLabels[row.original.status] ?? "待核对",
            },
            {
                accessorKey: "owner_name",
                header: "当前采购负责人",
                cell: ({ row }) => row.original.owner_name?.trim() || "待核对",
            },
            {
                id: "source-line-count",
                header: "真实选源行数",
                accessorFn: (row) => row.lines.length,
                meta: { numeric: true, align: "end" },
            },
            {
                id: "open-task-count",
                header: "当前开放任务数",
                accessorFn: (row) => row.tasks.length,
                meta: { numeric: true, align: "end" },
            },
            {
                id: "tasks",
                header: "开放履约任务",
                enableHiding: false,
                cell: ({ row }) =>
                    row.original.tasks.length ? (
                        <div className="flex flex-wrap gap-x-3 gap-y-2">
                            {row.original.tasks.map((task, index) => (
                                <Link
                                    key={task.id}
                                    id={`${prefix}-task-${toAutomationIdSegment(task.id)}`}
                                    href={`/workspace?view=managed&purchaseOrderIds=${encodeURIComponent(row.original.purchase_order_id)}&currentWorkItemId=${encodeURIComponent(task.id)}`}
                                    className="rounded-sm text-primary underline underline-offset-4"
                                >
                                    在工作台核对相关任务（{index + 1}）
                                </Link>
                            ))}
                        </div>
                    ) : (
                        <span className="text-muted-foreground">
                            暂无当前可查看的开放任务
                        </span>
                    ),
            },
        ],
        [prefix],
    )
    if (!enabled || !offeringId) return null
    return (
        <section className="min-w-0 space-y-4 rounded-xl border bg-card p-5">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                    <h2 className="font-semibold">当前采购与履约影响</h2>
                    <p className="mt-1 text-sm text-muted-foreground">
                        表内仅显示当前账号可查看的采购单与开放任务。核对当前负责人及未完成履约；旧采购单的价格、付款条件和已冻结选源保持原值。
                    </p>
                </div>
                <Button
                    id={`${prefix}-refresh`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={query.isFetching}
                    onClick={() => void query.refetch()}
                >
                    重新核对影响
                </Button>
            </div>
            {query.data && !query.error && (
                <>
                    {query.data.warning ? (
                        <div
                            role="status"
                            className="space-y-1 rounded-lg border border-warning-soft-foreground/30 bg-warning-soft/30 p-3 text-sm"
                        >
                            <p className="font-medium">
                                {warningLabels[query.data.warning.code] ??
                                    "当前供给情况需要核对"}
                            </p>
                            <p>
                                请由当前采购负责人逐单核验未完成履约，另行处理缺货或停供影响；此提示不会调整旧单金额、付款条件或采购状态。
                            </p>
                        </div>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            当前可供情况未触发缺货或停供提示，仍须按各采购单的真实履约进度核对。
                        </p>
                    )}
                    <p className="rounded-lg border bg-muted/30 p-3 text-sm text-muted-foreground">
                        {query.data.association_notice ||
                            "采购关联范围说明暂缺，请重新核对后确认。"}
                    </p>
                </>
            )}
            <ListWorkSurface
                ariaLabel="供给关联的采购与履约影响"
                selectionBar={
                    query.data ? (
                        <span className="text-sm">
                            共{query.data.total}张当前可查看的关联采购单
                        </span>
                    ) : undefined
                }
                table={
                    <DataTable
                        id={`${prefix}-table`}
                        caption="当前供给实际关联的采购单及开放履约任务"
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
                            <p className="p-6 text-sm text-muted-foreground">
                                当前权限及已记录选源范围内，没有关联的未完成采购。历史未记录选源的关联仍需人工核验。
                            </p>
                        }
                    />
                }
            />
        </section>
    )
}
