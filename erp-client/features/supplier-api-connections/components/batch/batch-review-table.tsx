"use client"

import * as React from "react"
import Link from "next/link"
import { PackageIcon, TruckIcon } from "lucide-react"
import { useStore } from "@tanstack/react-form"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
    BusinessTableFrame,
    DataTable,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
import { useAppFilterForm } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { Button, buttonVariants } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { compactFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { BatchRow, BatchRowStatus, BatchView } from "../../api/batch"
import { catalogUnitLabel } from "../../api/catalog"
import { CatalogSourceImage } from "../catalog/catalog-source-image"

export type BatchReviewMode = "REVIEW" | "RESULT" | "RECOVERY"

type BatchReviewTableProps = {
    view: BatchView
    mode: BatchReviewMode
    needsRecheck?: boolean
    selectedIds: string[]
    onSelectionChange: (ids: string[]) => void
    onEdit: (row: BatchRow) => void
    onReconcile: () => void
    busy: boolean
    canWrite: boolean
    canReadCost: boolean
}

const statusMeta: Record<
    BatchRowStatus,
    {
        label: string
        variant: "secondary" | "warning" | "success" | "destructive" | "info"
    }
> = {
    DRAFT: { label: "待补齐", variant: "warning" },
    READY: { label: "可入库", variant: "success" },
    PROCESSING: { label: "处理中", variant: "info" },
    SUCCEEDED: { label: "入库成功", variant: "success" },
    FAILED: { label: "明确失败", variant: "destructive" },
    UNKNOWN: { label: "结果待核实", variant: "warning" },
    SKIPPED: { label: "已完成跳过", variant: "secondary" },
    EXCLUDED: { label: "已排除", variant: "secondary" },
}

/** 提交只接受本次已就绪且未执行的确认项；待核实项只查原结果。 */
export function isBatchRowSelectable(
    row: BatchRow,
    mode: BatchReviewMode,
): boolean {
    if (mode === "RESULT" || row.operation_id) return false
    if (mode === "REVIEW") {
        return (
            row.status === "READY" &&
            (!row.warnings.length || row.warnings_confirmed)
        )
    }
    return (
        row.status === "DRAFT" ||
        row.status === "FAILED" ||
        row.status === "READY"
    )
}

function rowMessages(row: BatchRow): string[] {
    return Array.from(new Set([...row.messages, ...row.warnings]))
}

function manuallyAdjusted(row: BatchRow): boolean {
    return Object.values(row.sales_prices).some(
        (value) =>
            typeof value === "object" &&
            value !== null &&
            value.origin === "MANUAL",
    )
}

function RowStatus({
    row,
    needsRecheck,
}: {
    row: BatchRow
    needsRecheck: boolean
}) {
    const state = statusMeta[row.status]
    const stale = needsRecheck && row.status === "READY" && !row.operation_id
    return (
        <div className="space-y-1">
            <Badge variant={stale ? "secondary" : state.variant}>
                {row.status === "READY" && row.operation_id
                    ? "等待入库"
                    : stale
                      ? "待重新核对"
                      : state.label}
            </Badge>
            {row.status === "UNKNOWN" ? (
                <p className="text-xs text-warning-soft-foreground">
                    先查询原操作结果，不能直接重提。
                </p>
            ) : null}
            {row.status === "READY" &&
            row.warnings.length > 0 &&
            !row.warnings_confirmed ? (
                <p className="text-xs text-warning-soft-foreground">
                    价格提示待确认
                </p>
            ) : null}
            {rowMessages(row).map((message) => (
                <p
                    key={message}
                    className="max-w-80 text-xs break-words text-muted-foreground"
                >
                    {message}
                </p>
            ))}
        </div>
    )
}

function MatchKind({ row }: { row: BatchRow }) {
    if (row.status === "SKIPPED")
        return <span className="text-sm text-muted-foreground">已完成跳过</span>
    const kind = row.resolution?.kind
    return (
        <div className="space-y-1 text-sm">
            <span>
                {kind === "EXISTING"
                    ? "关联已有 SKU"
                    : kind === "CREATE_SKU"
                      ? "已有商品新增规格"
                      : kind === "CREATE_PRODUCT"
                        ? "新建商品与 SKU"
                        : "待自动核对"}
            </span>
        </div>
    )
}

/** 报价格式差异不算来源变化，只有真实变化才进入对比列。 */
function sourceChangeFlags(row: BatchRow, canReadCost: boolean) {
    const previous = row.previous_source
    if (!previous) return null
    const flags = {
        price:
            canReadCost &&
            (previous.purchase_price == null
                ? null
                : compactFixed(previous.purchase_price)) !==
                (row.source.purchase_price == null
                    ? null
                    : compactFixed(row.source.purchase_price)),
        specification: previous.specification !== row.source.specification,
        unit: catalogUnitLabel(previous) !== catalogUnitLabel(row.source),
    }
    return Object.values(flags).some(Boolean) ? flags : null
}

function SourceChanges({
    row,
    canReadCost,
}: {
    row: BatchRow
    canReadCost: boolean
}) {
    const previous = row.previous_source
    const changes = sourceChangeFlags(row, canReadCost)
    if (!previous || !changes)
        return <span className="text-muted-foreground">—</span>
    const previousUnit = catalogUnitLabel(previous)
    const currentUnit = catalogUnitLabel(row.source)
    return (
        <div className="min-w-48 space-y-1 text-xs">
            {changes.price ? (
                <>
                    <p className="text-muted-foreground">接口采购报价</p>
                    <p className="flex flex-wrap items-baseline gap-1">
                        {previous.purchase_price !== null ? (
                            <MoneyValue value={previous.purchase_price} />
                        ) : (
                            "未提供"
                        )}
                        <span aria-label="变更为">→</span>
                        {row.source.purchase_price !== null ? (
                            <MoneyValue value={row.source.purchase_price} />
                        ) : (
                            "未提供"
                        )}
                    </p>
                </>
            ) : null}
            {changes.specification ? (
                <p className="break-words">
                    <span className="text-muted-foreground">规格：</span>
                    {previous.specification || "未标注"} →{" "}
                    {row.source.specification || "未标注"}
                </p>
            ) : null}
            {changes.unit ? (
                <p className="break-words">
                    <span className="text-muted-foreground">单位：</span>
                    {previousUnit} → {currentUnit}
                </p>
            ) : null}
        </div>
    )
}

/** 核对、结果与恢复共用逐项事实表；翻页和筛选不会扩展提交范围。 */
export function BatchReviewTable({
    view,
    mode,
    needsRecheck = false,
    selectedIds,
    onSelectionChange,
    onEdit,
    onReconcile,
    busy,
    canWrite,
    canReadCost,
}: BatchReviewTableProps) {
    const profile = useAccountProfileQuery()
    const canOpenProduct = hasPermission(
        profile.data?.permissions,
        "product:detail",
    )
    const canOpenOffering = hasPermission(
        profile.data?.permissions,
        "supplier_offering:detail",
    )
    const prefix = `supplier-catalog-batch-${mode.toLowerCase()}`
    const form = useAppFilterForm({
        defaultValues: { status: "ALL", reason: "ALL" },
    })
    const filters = useStore(form.store, (state) => state.values)
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: 0,
        pageSize: 20,
    })
    const baseRows = React.useMemo(
        () =>
            mode === "RECOVERY"
                ? view.rows.filter((row) =>
                      [
                          "DRAFT",
                          "FAILED",
                          "UNKNOWN",
                          "READY",
                          "PROCESSING",
                      ].includes(row.status),
                  )
                : view.rows,
        [mode, view.rows],
    )
    const reasons = React.useMemo(
        () => Array.from(new Set(baseRows.flatMap(rowMessages))),
        [baseRows],
    )
    const allDropshipOnly =
        baseRows.length > 0 &&
        baseRows.every((row) => !row.terms?.bulk_supply_price_gross)
    const hasActiveFilters =
        filters.status !== "ALL" || filters.reason !== "ALL"
    const showFilters =
        mode === "RECOVERY" ||
        new Set(baseRows.map((row) => row.status)).size > 1 ||
        reasons.length > 0 ||
        hasActiveFilters
    const filteredRows = React.useMemo(
        () =>
            baseRows.filter(
                (row) =>
                    (filters.status === "ALL" ||
                        row.status === filters.status) &&
                    (filters.reason === "ALL" ||
                        rowMessages(row).includes(filters.reason)),
            ),
        [baseRows, filters],
    )
    React.useEffect(() => {
        setPagination((current) => ({ ...current, pageIndex: 0 }))
    }, [filters.status, filters.reason, mode])
    const pagedRows = filteredRows.slice(
        pagination.pageIndex * pagination.pageSize,
        (pagination.pageIndex + 1) * pagination.pageSize,
    )
    const selectableIds = React.useMemo(
        () =>
            new Set(
                view.rows
                    .filter(
                        (row) =>
                            isBatchRowSelectable(row, mode) &&
                            (mode !== "REVIEW" ||
                                row.preview_version ===
                                    view.batch.preview_version),
                    )
                    .map((row) => row.source.id),
            ),
        [view.rows, view.batch.preview_version, mode],
    )
    const rowSelection = React.useMemo(
        () =>
            Object.fromEntries(
                selectedIds
                    .filter((id) => selectableIds.has(id))
                    .map((id) => [id, true]),
            ),
        [selectedIds, selectableIds],
    )
    const selectedCount = selectedIds.filter((id) =>
        selectableIds.has(id),
    ).length
    const columns = React.useMemo<ColumnDef<BatchRow>[]>(() => {
        const columns: ColumnDef<BatchRow>[] = [
            {
                id: "product",
                header: "商品与规格",
                enableHiding: false,
                meta: {
                    label: "商品与规格",
                    width: mode === "RESULT" ? "flex" : "reference",
                },
                cell: ({ row }) => (
                    <div className="flex min-w-52 items-start gap-3">
                        {mode !== "RESULT" ? (
                            <span className="size-11 shrink-0 overflow-hidden rounded-md bg-muted">
                                {row.original.source.images?.[0] ? (
                                    <CatalogSourceImage
                                        image={row.original.source.images[0]}
                                        alt=""
                                        width={44}
                                        height={44}
                                        compact
                                        className="size-full object-cover"
                                    />
                                ) : null}
                            </span>
                        ) : null}
                        <div className="min-w-0 space-y-1">
                            <p className="font-medium">
                                {row.original.source.specification ||
                                    row.original.source.name}
                            </p>
                            {row.original.source.specification ? (
                                <p className="text-xs text-muted-foreground">
                                    {row.original.source.name}
                                </p>
                            ) : null}
                            {manuallyAdjusted(row.original) ? (
                                <Badge variant="warning">已单独调整</Badge>
                            ) : null}
                            {mode !== "RESULT" &&
                            !allDropshipOnly &&
                            !row.original.terms?.bulk_supply_price_gross ? (
                                <p className="text-xs text-muted-foreground">
                                    供货：仅代发
                                </p>
                            ) : null}
                        </div>
                    </div>
                ),
            },
            ...(view.rows.some((row) => sourceChangeFlags(row, canReadCost))
                ? [
                      {
                          id: "sourceChanges",
                          header: "来源变化",
                          meta: {
                              label: "来源变化",
                              width: "reference",
                          },
                          cell: ({ row }) => (
                              <SourceChanges
                                  row={row.original}
                                  canReadCost={canReadCost}
                              />
                          ),
                      } satisfies ColumnDef<BatchRow>,
                  ]
                : []),
            {
                id: "match",
                header: mode === "RESULT" ? "入库方式" : "自动匹配",
                meta: {
                    label: mode === "RESULT" ? "入库方式" : "自动匹配",
                },
                cell: ({ row }) => <MatchKind row={row.original} />,
            },
            {
                id: "purchase",
                header: "接口采购报价",
                meta: {
                    label: "接口采购报价",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue
                        className="font-normal"
                        value={row.original.source.purchase_price}
                    />
                ),
            },
            {
                id: "dropship",
                header: "公司代发价（含税）",
                meta: {
                    label: "公司代发价",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div>
                        <MoneyValue
                            className="font-semibold"
                            value={row.original.sales_prices.dropship.value}
                        />
                        {row.original.sales_prices.dropship.origin ===
                        "PRESERVED" ? (
                            <p className="text-xs text-muted-foreground">
                                保留原价
                            </p>
                        ) : row.original.sales_prices.dropship.origin ===
                          "CLEARED" ? (
                            <p className="text-xs text-muted-foreground">
                                明确清空
                            </p>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "bulk",
                header: "公司集采价 / 起订量",
                meta: {
                    label: "公司集采价与起订量",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="space-y-1">
                        <MoneyValue
                            className="font-semibold"
                            value={row.original.sales_prices.bulk.value}
                        />
                        {row.original.sales_prices.minimum_quantity.value !==
                        null ? (
                            <p className="text-xs text-muted-foreground">
                                起订{" "}
                                <QuantityValue
                                    value={
                                        row.original.sales_prices
                                            .minimum_quantity.value
                                    }
                                    unit={catalogUnitLabel({
                                        ...row.original.source,
                                        suggested_unit:
                                            row.original.suggested_unit,
                                        unit_inferred:
                                            row.original.unit_inferred,
                                    })}
                                />
                            </p>
                        ) : null}
                        {row.original.sales_prices.bulk.origin ===
                        "PRESERVED" ? (
                            <p className="text-xs text-muted-foreground">
                                保留原价
                            </p>
                        ) : row.original.sales_prices.bulk.origin ===
                          "CLEARED" ? (
                            <p className="text-xs text-muted-foreground">
                                明确清空
                            </p>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "status",
                header: mode === "RESULT" ? "入库结果" : "状态与待处理原因",
                meta: {
                    label: mode === "RESULT" ? "入库结果" : "状态与待处理原因",
                },
                cell: ({ row }) => (
                    <RowStatus row={row.original} needsRecheck={needsRecheck} />
                ),
            },
            {
                id: "sourceCategory",
                header: "来源分类",
                meta: { label: "来源分类" },
                cell: ({ row }) =>
                    row.original.source.category_name || "未提供",
            },
            {
                id: "market",
                header: "公司市场价",
                meta: {
                    label: "公司市场价",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue
                        value={row.original.sales_prices.market_price.value}
                    />
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: {
                    width: mode === "RESULT" ? "reference" : "default",
                    align: "end",
                },
                enableHiding: false,
                enableSorting: false,
                cell: ({ row: tableRow }) => {
                    const row = tableRow.original
                    const key = toAutomationIdSegment(row.source.id)
                    if (row.status === "UNKNOWN")
                        return canWrite ? (
                            <LoadingButton
                                id={`${prefix}-query-result-${key}`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={busy}
                                onClick={onReconcile}
                            >
                                查询原操作结果
                            </LoadingButton>
                        ) : null
                    if (
                        row.status === "SUCCEEDED" ||
                        row.status === "SKIPPED"
                    ) {
                        const productId =
                            row.result?.product_id ||
                            row.source.company_product_id
                        const offeringId =
                            row.result?.offering_id || row.source.offering_id
                        return (
                            <div className="flex flex-wrap justify-end gap-1">
                                {canOpenProduct && productId ? (
                                    <Link
                                        id={`${prefix}-product-${key}`}
                                        href={`/master-data/products/${encodeURIComponent(productId)}`}
                                        className={buttonVariants({
                                            variant: "ghost",
                                            size: "sm",
                                        })}
                                    >
                                        <PackageIcon aria-hidden />
                                        查看商品
                                    </Link>
                                ) : null}
                                {canOpenOffering && offeringId ? (
                                    <Link
                                        id={`${prefix}-offering-${key}`}
                                        href={`/procurement/supplier-offerings/${encodeURIComponent(offeringId)}`}
                                        className={buttonVariants({
                                            variant: "ghost",
                                            size: "sm",
                                        })}
                                    >
                                        <TruckIcon aria-hidden />
                                        查看供给
                                    </Link>
                                ) : null}
                            </div>
                        )
                    }
                    if (row.status === "PROCESSING" || row.operation_id)
                        return (
                            <span className="text-xs text-muted-foreground">
                                后台持续处理
                            </span>
                        )
                    if (row.status === "EXCLUDED")
                        return (
                            <span className="text-xs text-muted-foreground">
                                未提交
                            </span>
                        )
                    return canWrite ? (
                        <Button
                            id={`${prefix}-edit-${key}`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={busy}
                            onClick={() => onEdit(row)}
                        >
                            {row.warnings.length && !row.warnings_confirmed
                                ? "调整与确认提示"
                                : "调整"}
                        </Button>
                    ) : null
                },
            },
        ]
        return canReadCost
            ? columns
            : columns.filter((column) => column.id !== "purchase")
    }, [
        canReadCost,
        prefix,
        mode,
        busy,
        canWrite,
        canOpenProduct,
        canOpenOffering,
        onEdit,
        onReconcile,
        view.rows,
        needsRecheck,
        allDropshipOnly,
    ])
    return (
        <div className="min-w-0 space-y-3">
            {mode === "RECOVERY" && view.counts.unknown > 0 ? (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning-border bg-warning-soft px-4 py-3 text-sm text-warning-soft-foreground">
                    <p>
                        {view.counts.unknown} 个 SKU
                        的原操作结果待核实；查明前不能选择重试。
                    </p>
                    {canWrite && (
                        <LoadingButton
                            id={`${prefix}-query-all-unknown`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={busy}
                            onClick={onReconcile}
                        >
                            查询原操作结果
                        </LoadingButton>
                    )}
                </div>
            ) : null}
            <BusinessTableFrame
                title={
                    mode === "RECOVERY"
                        ? "待处理商品"
                        : mode === "RESULT"
                          ? "入库结果明细"
                          : "逐项核对入库"
                }
                className={mode !== "RECOVERY" ? "gap-2" : undefined}
                tableActions={
                    allDropshipOnly && mode !== "RESULT" ? (
                        <span className="text-xs text-muted-foreground">
                            供应商供货：仅代发
                        </span>
                    ) : undefined
                }
                toolbar={
                    showFilters ? (
                        <form
                            className="flex min-w-0 flex-wrap items-end gap-3"
                            onSubmit={(event) => event.preventDefault()}
                        >
                            <form.AppField name="status">
                                {(field) => (
                                    <field.SelectField
                                        id={`${prefix}-status-filter`}
                                        label="状态"
                                        options={[
                                            {
                                                value: "ALL",
                                                label: `全部（${baseRows.length}）`,
                                            },
                                            ...Object.entries(statusMeta)
                                                .filter(([status]) =>
                                                    baseRows.some(
                                                        (row) =>
                                                            row.status ===
                                                            status,
                                                    ),
                                                )
                                                .map(([value, state]) => ({
                                                    value,
                                                    label: `${needsRecheck && value === "READY" ? "上次可入库" : state.label}（${baseRows.filter((row) => row.status === value).length}）`,
                                                })),
                                        ]}
                                        allowClear={false}
                                        className="w-44"
                                    />
                                )}
                            </form.AppField>
                            {reasons.length > 0 || filters.reason !== "ALL" ? (
                                <form.AppField name="reason">
                                    {(field) => (
                                        <field.SelectField
                                            id={`${prefix}-reason-filter`}
                                            label="待处理原因"
                                            options={[
                                                {
                                                    value: "ALL",
                                                    label: "全部原因",
                                                },
                                                ...reasons.map((reason) => ({
                                                    value: reason,
                                                    label: reason,
                                                })),
                                            ]}
                                            allowClear={false}
                                            className="w-64 max-w-full"
                                        />
                                    )}
                                </form.AppField>
                            ) : null}
                            {hasActiveFilters ? (
                                <Button
                                    id={`${prefix}-reset-filters`}
                                    type="button"
                                    variant="ghost"
                                    onClick={() =>
                                        form.reset({
                                            status: "ALL",
                                            reason: "ALL",
                                        })
                                    }
                                >
                                    重置筛选
                                </Button>
                            ) : null}
                        </form>
                    ) : undefined
                }
                selectionBar={
                    <div className="flex flex-wrap items-center gap-3 text-sm">
                        {mode === "RESULT" ? (
                            <h2 className="font-semibold">入库明细</h2>
                        ) : null}
                        <span className="num">
                            当前 {filteredRows.length} 个 SKU
                            {canWrite && mode !== "RESULT"
                                ? ` · 已选 ${selectedCount} 个 SKU`
                                : ""}
                        </span>
                        {canWrite && mode !== "RESULT" ? (
                            <Button
                                id={`${prefix}-clear-selection`}
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={busy || selectedIds.length === 0}
                                onClick={() => onSelectionChange([])}
                            >
                                清空选择
                            </Button>
                        ) : null}
                    </div>
                }
                table={
                    <DataTable
                        id={prefix}
                        data={pagedRows}
                        columns={columns}
                        getRowId={(row) => row.source.id}
                        rowLabel={(row) =>
                            `${row.source.name} ${row.source.specification}`
                        }
                        rowCount={filteredRows.length}
                        caption="批量入库逐项核对，接口采购报价为供应商报价，公司售价为含税销售参考价"
                        manualPagination
                        manualSorting={false}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        pageSizeOptions={[20]}
                        defaultColumnVisibility={{
                            sourceCategory: false,
                            market: false,
                            purchase: mode !== "RESULT",
                            match: mode !== "RESULT",
                        }}
                        showToolbarSummary={false}
                        rowSelection={rowSelection}
                        onRowSelectionChange={(selection) =>
                            onSelectionChange(
                                Object.entries(selection)
                                    .filter(
                                        ([id, selected]) =>
                                            selected && selectableIds.has(id),
                                    )
                                    .map(([id]) => id),
                            )
                        }
                        enableRowSelection={
                            !canWrite || mode === "RESULT"
                                ? false
                                : (row) =>
                                      !busy &&
                                      isBatchRowSelectable(
                                          row.original,
                                          mode,
                                      ) &&
                                      (mode !== "REVIEW" ||
                                          row.original.preview_version ===
                                              view.batch.preview_version)
                        }
                        emptyTitle="当前筛选没有商品"
                        emptyDescription="调整状态或待处理原因后继续查看。"
                    />
                }
            />
        </div>
    )
}
