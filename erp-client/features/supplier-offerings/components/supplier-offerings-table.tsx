"use client"

import { useMemo } from "react"
import type {
    ColumnDef,
    PaginationState,
    RowSelectionState,
} from "@tanstack/react-table"
import Link from "next/link"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { offeringDetailHref } from "../lib/detail"
import type { OfferingStatusIntent } from "../lib/offering-status"
import {
    AVAILABILITY_STATUS_LABELS,
    OFFERING_STATUS_LABELS,
    SOURCE_TYPE_LABELS,
    type AvailabilityStatus,
    type OfferingStatus,
    type SupplierOfferingView,
} from "../types"
import { SupplierOfferingRowActions } from "./supplier-offering-row-actions"

const TABLE_ID = "supplier-offerings-table"
const COLUMN_ORDER = [
    "__selection",
    "sku-name",
    "company-sku",
    "supplier",
    "prices",
    "delivery-template",
    "availability",
    "status",
    "maintainer",
    "conditions",
    "source-type",
    "revision",
    "actions",
]
const COLUMN_VISIBILITY = {
    "company-sku": false,
    maintainer: false,
    conditions: false,
    "source-type": false,
    revision: false,
}
const COLUMN_PINNING = { left: ["sku-name"], right: ["actions"] }
const OFFERING_TONES: Record<OfferingStatus, StatusTone> = {
    ACTIVE: "success",
    PAUSED: "warning",
    STOPPED: "destructive",
}
const AVAILABILITY_TONES: Record<AvailabilityStatus, StatusTone> = {
    AVAILABLE: "success",
    UNAVAILABLE: "warning",
    STALE: "warning",
    STOPPED: "destructive",
}

export function getSupplierOfferingRowId(item: SupplierOfferingView) {
    return item.id
}

function offeringLabel(item: SupplierOfferingView) {
    return item.sku_name?.trim() || item.sku_no || "供给资料"
}

function rowLabel(item: SupplierOfferingView) {
    return `${offeringLabel(item)} · ${item.supplier_name || item.supplier_sku_code}`
}

type SupplierOfferingsTableProps = {
    rowSelection: RowSelectionState
    onRowSelectionChange?: (selection: RowSelectionState) => void
    pagination: PaginationState
    onPaginationChange: (pagination: PaginationState) => void
    total: number
    items: readonly SupplierOfferingView[]
    loading: boolean
    isError: boolean
    error?: Error | null
    scopeChanged: boolean
    hasFilters: boolean
    noScope?: boolean
    onRowPreview: (offering: SupplierOfferingView) => void
    highlightedRowId?: string
    returnTo: string
    canViewCosts: boolean
    canRevise: boolean
    canAvailability: boolean
    taskMode: boolean
    taskBusinessObjectId?: string
    onRetry: () => void
    onClearFilters: () => void
    onCreateOffering?: () => void
    onUpdateAvailability: (offering: SupplierOfferingView) => void
    onReviseOffering: (offering: SupplierOfferingView) => void
    onChangeStatus: (
        offering: SupplierOfferingView,
        intent: OfferingStatusIntent,
    ) => void
}

export function SupplierOfferingsTable({
    rowSelection,
    onRowSelectionChange,
    pagination,
    onPaginationChange,
    total,
    items,
    loading,
    isError,
    error,
    scopeChanged,
    hasFilters,
    noScope,
    onRowPreview,
    highlightedRowId,
    returnTo,
    canViewCosts,
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
    const data = useMemo(() => [...items], [items])
    const columns = useMemo(() => {
        const columns: ColumnDef<SupplierOfferingView, unknown>[] = [
            {
                id: "sku-name",
                header: "SKU 名称",
                meta: { width: "flex" },
                cell: ({ row: { original: item } }) => {
                    const name = offeringLabel(item)
                    const secondary = [item.sku_no, item.specification]
                        .filter(Boolean)
                        .join(" · ")
                    return (
                        <TableIdentityCell
                            primary={
                                <Link
                                    id={`supplier-offerings-open-${toAutomationIdSegment(item.id)}`}
                                    href={offeringDetailHref(item.id, returnTo)}
                                    className="hover:underline"
                                >
                                    {name}
                                </Link>
                            }
                            primaryTitle={name}
                            secondary={secondary}
                            secondaryTitle={secondary}
                            numericSecondary
                        />
                    )
                },
            },
            {
                id: "company-sku",
                header: "SPU 编号",
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.product_no}
                        clamp={1}
                        numeric
                    />
                ),
            },
            {
                id: "supplier",
                header: "供应商 / 订货编码",
                cell: ({ row: { original: item } }) => {
                    const name = item.supplier_name || item.supplier_no
                    return (
                        <TableIdentityCell
                            primary={name || <TableEmptyCell />}
                            primaryTitle={name ?? undefined}
                            secondary={item.supplier_sku_code}
                            secondaryTitle={item.supplier_sku_code}
                            numericSecondary
                        />
                    )
                },
            },
            {
                id: "delivery-template",
                header: "配送模板",
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.delivery_template?.display_name}
                        clamp={1}
                    />
                ),
            },
            {
                id: "availability",
                header: "当前可供",
                meta: { width: "status" },
                cell: ({ row: { original: item } }) => {
                    const status = item.availability_status
                    return (
                        <div className="flex flex-col items-start gap-0.5">
                            <BusinessStatusBadge
                                context="list"
                                tone={
                                    status
                                        ? AVAILABILITY_TONES[status]
                                        : "neutral"
                                }
                                appearance={
                                    status === "AVAILABLE" ? "quiet" : "badge"
                                }
                                label={
                                    status
                                        ? AVAILABILITY_STATUS_LABELS[status]
                                        : "未更新"
                                }
                            />
                            {item.available_quantity != null ? (
                                <span className="text-xs text-muted-foreground">
                                    数量{" "}
                                    <span className="num">
                                        {item.available_quantity}
                                    </span>
                                </span>
                            ) : null}
                        </div>
                    )
                },
            },
            {
                id: "status",
                header: "关系状态",
                meta: { width: "status" },
                cell: ({ row: { original: item } }) => (
                    <BusinessStatusBadge
                        context="list"
                        tone={OFFERING_TONES[item.status]}
                        appearance={
                            item.status === "ACTIVE" ? "quiet" : "badge"
                        }
                        label={OFFERING_STATUS_LABELS[item.status]}
                    />
                ),
            },
            {
                id: "maintainer",
                header: "维护人",
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.maintainer_user_name}
                        clamp={1}
                    />
                ),
            },
            {
                id: "conditions",
                header: "集采起订量",
                meta: { align: "end", numeric: true, width: "quantity" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.bulk_minimum_order_quantity}
                    />
                ),
            },
            {
                id: "source-type",
                header: "登记来源",
                cell: ({ row }) => (
                    <TableTextCell
                        value={SOURCE_TYPE_LABELS[row.original.source_type]}
                    />
                ),
            },
            {
                id: "revision",
                header: "条款版本",
                meta: { align: "end", numeric: true, width: "quantity" },
                cell: ({ row }) => {
                    const revision = row.original.current_revision_no
                    return (
                        <TableTextCell
                            value={revision == null ? null : `v${revision}`}
                        />
                    )
                },
            },
        ]
        if (canViewCosts) {
            columns.push({
                id: "prices",
                header: "含税供给价",
                meta: { align: "end", numeric: true, width: "amount" },
                cell: ({ row: { original: item } }) => (
                    <div className="flex flex-col items-end">
                        <OfferingPrice
                            label="代发"
                            value={item.dropship_supply_price_gross}
                        />
                        <OfferingPrice
                            label="集采"
                            value={item.bulk_supply_price_gross}
                        />
                    </div>
                ),
            })
        }
        if (taskMode || canRevise || canAvailability) {
            columns.push({
                id: "actions",
                header: taskMode ? "任务关联" : "操作",
                meta: {
                    align: "end",
                    width: "status",
                    label: taskMode ? "任务关联" : "操作",
                },
                cell: ({ row: { original: item } }) =>
                    taskMode ? (
                        item.id === taskBusinessObjectId ? (
                            <BusinessStatusBadge
                                context="list"
                                tone="destructive"
                                label="当前任务来源"
                            />
                        ) : (
                            <TableTextCell value="核对模式只读" clamp={1} />
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
                    ),
            })
        }
        return columns
    }, [
        canAvailability,
        canRevise,
        canViewCosts,
        onChangeStatus,
        onReviseOffering,
        onUpdateAvailability,
        returnTo,
        taskBusinessObjectId,
        taskMode,
    ])

    return (
        <ListDataTable
            id={TABLE_ID}
            entityName="供应商供给"
            data={data}
            columns={columns}
            defaultColumnVisibility={COLUMN_VISIBILITY}
            defaultColumnOrder={COLUMN_ORDER}
            defaultColumnPinning={COLUMN_PINNING}
            getRowId={getSupplierOfferingRowId}
            rowLabel={rowLabel}
            caption="供应商供给"
            rowCount={total}
            pagination={pagination}
            onPaginationChange={onPaginationChange}
            showPagination={false}
            enableRowSelection={Boolean(onRowSelectionChange)}
            rowSelection={rowSelection}
            onRowSelectionChange={onRowSelectionChange}
            onRowPreview={onRowPreview}
            highlightedRowId={
                highlightedRowId ??
                (taskMode ? taskBusinessObjectId : undefined)
            }
            loading={loading}
            errorState={
                isError ? (
                    <BusinessFailureState
                        idPrefix={TABLE_ID}
                        title="供给列表加载失败"
                        error={error ?? undefined}
                        onRetry={scopeChanged ? undefined : onRetry}
                        action={
                            scopeChanged ? (
                                <Button
                                    id={`${TABLE_ID}-retry`}
                                    variant="outline"
                                    size="sm"
                                    onClick={onRetry}
                                >
                                    返回第一页重新查询
                                </Button>
                            ) : undefined
                        }
                    />
                ) : undefined
            }
            noScope={noScope}
            hasActiveFilters={hasFilters}
            onClearFilters={onClearFilters}
            clearFiltersButtonId={`${TABLE_ID}-clear-filters`}
            canCreate={!taskMode && Boolean(onCreateOffering)}
            onCreate={onCreateOffering}
            createButtonId={`${TABLE_ID}-create`}
            createLabel="添加供给"
            emptyCopy={{
                noScopeTitle: "当前没有可查看的供给范围",
                noScopeDescription: "请联系管理员开通供给数据范围后重试。",
                filterDescription: "没有符合当前筛选的供给关系。",
                emptyDescription: taskMode
                    ? "当前列表没有加载任务来源的供给行；来源身份以上方任务记录为准。"
                    : onCreateOffering
                      ? "为公司商品的具体 SKU 添加第一条供给。"
                      : undefined,
            }}
        />
    )
}

function OfferingPrice({
    label,
    value,
}: {
    label: string
    value?: string | null
}) {
    return (
        <span className="inline-flex items-baseline gap-2">
            <span className="text-xs text-muted-foreground">{label}</span>
            {value ? (
                <MoneyValue value={value} display="compact" showHint={false} />
            ) : (
                <TableEmptyCell />
            )}
        </span>
    )
}
