"use client"

import { BusinessEmptyState, BusinessFailureState } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
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
    items: readonly SupplierOfferingView[]
    isPending: boolean
    isError: boolean
    error?: Error | null
    hasFilters: boolean
    noScope?: boolean
    ownerOptions?: ReadonlyArray<{ value: string; label: string }>
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
    items,
    isPending,
    isError,
    error,
    hasFilters,
    noScope = false,
    ownerOptions = [],
    taskMode,
    taskBusinessObjectId,
    onRetry,
    onClearFilters,
    onCreateOffering,
    onUpdateAvailability,
    onReviseOffering,
    onChangeStatus,
}: SupplierOfferingsTableProps) {
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
        <Table data-density="comfortable">
            <TableHeader>
                <TableRow>
                    <TableHead>SKU 名称</TableHead>
                    <TableHead>公司商品 / SKU</TableHead>
                    <TableHead>供应商 / 订货编码</TableHead>
                    <TableHead>维护人</TableHead>
                    <TableHead>供给价格</TableHead>
                    <TableHead>起订量 / 区域</TableHead>
                    <TableHead>当前可供</TableHead>
                    <TableHead>关系状态</TableHead>
                    <TableHead className="text-right">
                        {taskMode ? "任务关联" : "操作"}
                    </TableHead>
                </TableRow>
            </TableHeader>
            <TableBody>
                {items.map((item) => (
                    <TableRow
                        key={item.id}
                        className={
                            taskMode && item.id === taskBusinessObjectId
                                ? "bg-destructive/5"
                                : undefined
                        }
                    >
                        <TableCell>
                            <div className="font-medium">
                                {item.sku_name?.trim() || "—"}
                            </div>
                        </TableCell>
                        <TableCell>
                            <div className="font-medium">
                                {item.product_no ?? "公司商品"}
                            </div>
                            <div className="mt-1 text-xs text-muted-foreground">
                                {item.sku_no ?? item.sku_id}
                                {item.specification
                                    ? ` · ${item.specification}`
                                    : ""}
                            </div>
                        </TableCell>
                        <TableCell>
                            <div className="font-medium">
                                {item.supplier_name ??
                                    item.supplier_no ??
                                    item.supplier_id}
                            </div>
                            <div className="mt-1 text-xs text-muted-foreground">
                                {item.supplier_sku_code} ·{" "}
                                {SOURCE_TYPE_LABELS[item.source_type]}
                            </div>
                        </TableCell>
                        <TableCell>
                            <span className="text-sm">
                                {ownerOptions.find(
                                    (option) =>
                                        option.value ===
                                        item.maintainer_user_id,
                                )?.label ??
                                    item.maintainer_user_id ??
                                    "—"}
                            </span>
                        </TableCell>
                        <TableCell>
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
                        </TableCell>
                        <TableCell>
                            <div>{item.bulk_minimum_order_quantity ?? "—"}</div>
                            <div className="mt-1 max-w-48 truncate text-xs text-muted-foreground">
                                {item.supply_region.join("、") || "—"}
                            </div>
                        </TableCell>
                        <TableCell>
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
                        </TableCell>
                        <TableCell>
                            <Badge variant={statusVariant(item.status)}>
                                {OFFERING_STATUS_LABELS[item.status]}
                            </Badge>
                            <div className="mt-1 text-xs text-muted-foreground">
                                条款 v{item.current_revision_no ?? "—"}
                            </div>
                        </TableCell>
                        <TableCell>
                            {taskMode ? (
                                item.id === taskBusinessObjectId ? (
                                    <Badge variant="destructive">
                                        当前任务来源
                                    </Badge>
                                ) : (
                                    <span className="text-xs text-muted-foreground">
                                        核对模式只读
                                    </span>
                                )
                            ) : (
                                <SupplierOfferingRowActions
                                    offering={item}
                                    onUpdateAvailability={onUpdateAvailability}
                                    onReviseOffering={onReviseOffering}
                                    onChangeStatus={onChangeStatus}
                                />
                            )}
                        </TableCell>
                    </TableRow>
                ))}
            </TableBody>
        </Table>
    )
}
