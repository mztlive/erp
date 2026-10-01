"use client"

import type { ColumnDef } from "@tanstack/react-table"
import Link from "next/link"
import { DataTable } from "@/components/business/data-table"
import { offeringDetailHref } from "../lib/detail"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SupplierOfferingRowActions } from "@/features/supplier-offerings/components/supplier-offering-row-actions"
import type { OfferingStatusIntent } from "@/features/supplier-offerings/lib/offering-status"
import {
    money,
    statusVariant,
} from "@/features/supplier-offerings/lib/presentation"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import {
    AVAILABILITY_STATUS_LABELS,
    OFFERING_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
} from "@/features/supplier-offerings/types"

export type SupplierOfferingsTableProps = {
    selectedIds?: readonly string[]
    onSelectionChange?: (ids: string[]) => void
    total: number
    items: readonly SupplierOfferingView[]
    isPending: boolean
    isError: boolean
    error?: Error | null
    hasFilters: boolean
    noScope?: boolean

    onRowPreview: (offering: SupplierOfferingView) => void
    highlightedRowId?: string
    returnTo: string
    canRevise: boolean
    canAvailability: boolean
    taskMode: boolean
    taskBusinessObjectId?: string
    onRetry: () => void
    onClearFilters: () => void
    onCreateOffering: () => void
    onUpdateAvailability: (offering: SupplierOfferingView) => void
    onReviseOffering: (offering: SupplierOfferingView) => void
    onChangeStatus: (
        offering: SupplierOfferingView,
        intent: OfferingStatusIntent,
    ) => void
}

/** 供给列表的加载失败、空态与数据表格三态展示。 */
export function SupplierOfferingsTable({
    selectedIds = [],
    onSelectionChange,
    items,
    total,
    isPending,
    isError,
    error,
    hasFilters,
    noScope = false,

    onRowPreview,
    highlightedRowId,
    returnTo,
    canRevise,
    canAvailability,
    taskMode,
    taskBusinessObjectId,
    onRetry,
    onClearFilters,
    onCreateOffering,
    onUpdateAvailability,
    onReviseOffering,
    onChangeStatus,
}: SupplierOfferingsTableProps) {
    const columns: ColumnDef<SupplierOfferingView, unknown>[] = [
        {
            id: "sku-name",
            header: "SKU 名称",
            meta: { label: "SKU 名称" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <Link
                        id={`supplier-offerings-open-${toAutomationIdSegment(item.id)}`}
                        href={offeringDetailHref(item.id, returnTo)}
                        className="font-medium hover:underline"
                    >
                        {item.sku_name?.trim() || item.sku_no || "供给资料"}
                    </Link>
                </>
            ),
        },
        {
            id: "company-sku",
            header: "公司商品 / SKU",
            meta: { label: "公司商品 / SKU" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <div className="font-medium">
                        {item.product_no ?? "公司商品"}
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">
                        {item.sku_no ?? "—"}
                        {item.specification ? ` · ${item.specification}` : ""}
                    </div>
                </>
            ),
        },
        {
            id: "supplier",
            header: "供应商 / 订货编码",
            meta: { label: "供应商 / 订货编码" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <div className="font-medium">
                        {item.supplier_name ?? item.supplier_no ?? "未提供"}
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">
                        {item.supplier_sku_code} ·{" "}
                        {SOURCE_TYPE_LABELS[item.source_type]}
                    </div>
                </>
            ),
        },
        {
            id: "maintainer",
            header: "维护人",
            meta: { label: "维护人" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <span className="text-sm">
                        {item.maintainer_user_name || "—"}
                    </span>
                </>
            ),
        },
        {
            id: "prices",
            header: "供给价格",
            meta: { label: "供给价格" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <div className="text-sm">
                        <span className="text-xs text-muted-foreground mr-1.5">
                            代发
                        </span>
                        <span className="tabular-nums font-semibold text-foreground">
                            {money(item.dropship_supply_price_gross)}
                        </span>
                    </div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                        <span className="mr-1.5">集采</span>
                        <span className="tabular-nums font-medium text-foreground/80">
                            {money(item.bulk_supply_price_gross)}
                        </span>
                    </div>
                </>
            ),
        },
        {
            id: "conditions",
            header: "起订量 / 区域",
            meta: { label: "起订量 / 区域" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <div>{item.bulk_minimum_order_quantity ?? "—"}</div>
                    <div className="mt-1 max-w-48 truncate text-xs text-muted-foreground">
                        {item.supply_region.join("、") || "—"}
                    </div>
                </>
            ),
        },
        {
            id: "availability",
            header: "当前可供",
            meta: { label: "当前可供" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <Badge
                        variant={
                            item.availability_status === "AVAILABLE"
                                ? "success"
                                : "outline"
                        }
                    >
                        {item.availability_status
                            ? AVAILABILITY_STATUS_LABELS[
                                  item.availability_status
                              ]
                            : "未更新"}
                    </Badge>
                    <div className="mt-1 text-xs text-muted-foreground">
                        数量 {item.available_quantity ?? "未提供"}
                    </div>
                </>
            ),
        },
        {
            id: "status",
            header: "关系状态",
            meta: { label: "关系状态" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    <Badge variant={statusVariant(item.status)}>
                        {OFFERING_STATUS_LABELS[item.status]}
                    </Badge>
                    <div className="mt-1 text-xs text-muted-foreground">
                        条款 v{item.current_revision_no ?? "—"}
                    </div>
                </>
            ),
        },
        {
            id: "actions",
            header: taskMode ? "任务关联" : "操作",
            meta: { label: "操作" },
            enableSorting: false,
            cell: ({ row: { original: item } }) => (
                <>
                    {taskMode ? (
                        item.id === taskBusinessObjectId ? (
                            <Badge variant="destructive">当前任务来源</Badge>
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                核对模式只读
                            </span>
                        )
                    ) : (
                        <SupplierOfferingRowActions
                            offering={item}
                            canRevise={canRevise}
                            canAvailability={canAvailability}
                            onUpdateAvailability={onUpdateAvailability}
                            onReviseOffering={onReviseOffering}
                            onChangeStatus={onChangeStatus}
                        />
                    )}
                </>
            ),
        },
    ]

    if (isError) {
        return (
            <BusinessFailureState
                title="供给列表加载失败"
                error={error}
                onRetry={onRetry}
            />
        )
    }

    if (items.length === 0 && !isPending) {
        return (
            <BusinessEmptyState
                kind={noScope ? "no-data" : hasFilters ? "filter" : "no-data"}
                title={
                    noScope
                        ? "当前没有可查看的供给范围"
                        : hasFilters
                          ? undefined
                          : "还没有供应商供给"
                }
                description={
                    noScope
                        ? "已授权动作但没有可见供给。请联系管理员配置数据范围，或清除筛选后重试。"
                        : hasFilters
                          ? "没有符合当前筛选的供给关系。"
                          : taskMode
                            ? "当前列表没有加载任务来源的供给行；来源身份以上方任务记录为准。"
                            : "先添加公司商品，再为具体 SKU 添加第一条供给。"
                }
                action={
                    hasFilters ? (
                        <Button
                            id="supplier-offerings-table-clear-filters"
                            type="button"
                            variant="secondary"
                            size="sm"
                            className="rounded-lg shadow-none"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : !taskMode ? (
                        <Button
                            id="supplier-offerings-table-create"
                            type="button"
                            size="sm"
                            onClick={onCreateOffering}
                        >
                            添加供给
                        </Button>
                    ) : undefined
                }
            />
        )
    }

    return (
        <DataTable
            idPrefix="supplier-offerings-table"
            data={[...items]}
            columns={columns}
            getRowId={(item) => item.id}
            rowLabel={(item) =>
                `${item.sku_name || item.sku_no || "供给"} · ${item.supplier_name || item.supplier_sku_code}`
            }
            rowCount={total}
            loading={isPending}
            layout="flush"
            showPagination={false}
            enableRowSelection={Boolean(onSelectionChange)}
            rowSelection={Object.fromEntries(
                selectedIds.map((id) => [id, true]),
            )}
            onRowSelectionChange={(selection) =>
                onSelectionChange?.(
                    Object.keys(selection).filter((id) => selection[id]),
                )
            }
            onRowPreview={onRowPreview}
            highlightedRowId={
                highlightedRowId ??
                (taskMode ? taskBusinessObjectId : undefined)
            }
        />
    )
}
