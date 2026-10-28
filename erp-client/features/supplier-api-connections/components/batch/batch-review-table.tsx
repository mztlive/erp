"use client"

import * as React from "react"
import Link from "next/link"
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
import { Button, buttonVariants } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { BatchRow, BatchRowStatus, BatchView } from "../../api/batch"
import { catalogUnitLabel } from "../../api/catalog"
import { CatalogSourceImage } from "../catalog/catalog-source-image"

export type BatchReviewMode = "REVIEW" | "RESULT" | "RECOVERY"

type BatchReviewTableProps = {
    view: BatchView
    mode: BatchReviewMode
    selectedIds: string[]
    onSelectionChange: (ids: string[]) => void
    onEdit: (row: BatchRow) => void
    onReconcile: () => void
    busy: boolean
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

function RowStatus({ row }: { row: BatchRow }) {
    const state = statusMeta[row.status]
    return (
        <div className="space-y-1">
            <Badge variant={state.variant}>
                {row.status === "READY" && row.operation_id
                    ? "等待入库"
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
            {kind === "EXISTING" ? (
                <p className="text-xs text-muted-foreground">
                    保留原价、上架状态与资料
                </p>
            ) : row.status === "SUCCEEDED" && kind ? (
                <p className="text-xs text-muted-foreground">
                    新建 SKU 保持未上架
                </p>
            ) : null}
        </div>
    )
}

function SourceChanges({ row }: { row: BatchRow }) {
    const previous = row.previous_source
    if (!previous) return <span className="text-muted-foreground">—</span>
    const previousUnit = catalogUnitLabel(previous)
    const currentUnit = catalogUnitLabel(row.source)
    return (
        <div className="min-w-48 space-y-1 text-xs">
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
            {previous.specification !== row.source.specification ? (
                <p className="break-words">
                    <span className="text-muted-foreground">规格：</span>
                    {previous.specification || "未标注"} →{" "}
                    {row.source.specification || "未标注"}
                </p>
            ) : null}
            {previousUnit !== currentUnit ? (
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
    selectedIds,
    onSelectionChange,
    onEdit,
    onReconcile,
    busy,
}: BatchReviewTableProps) {
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
    const columns = React.useMemo<ColumnDef<BatchRow>[]>(
        () => [
            {
                id: "product",
                header: "商品与规格",
                enableHiding: false,
                meta: { label: "商品与规格", width: "flex" },
                cell: ({ row }) => (
                    <div className="flex min-w-52 items-start gap-3">
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
                        <div className="min-w-0 space-y-1">
                            <p className="font-medium">
                                {row.original.source.name}
                            </p>
                            <p className="text-xs text-muted-foreground">
                                {row.original.source.specification ||
                                    "未标注规格"}
                            </p>
                            {manuallyAdjusted(row.original) ? (
                                <Badge variant="warning">已单独调整</Badge>
                            ) : null}
                            {!row.original.terms?.bulk_supply_price_gross ? (
                                <p className="text-xs text-muted-foreground">
                                    本供给仅支持代发
                                </p>
                            ) : null}
                        </div>
                    </div>
                ),
            },
            ...(mode === "RECOVERY" ||
            view.rows.some((row) => row.previous_source)
                ? [
                      {
                          id: "sourceChanges",
                          header: "来源变化",
                          meta: { label: "来源变化", width: "reference" },
                          cell: ({ row }) => (
                              <SourceChanges row={row.original} />
                          ),
                      } satisfies ColumnDef<BatchRow>,
                  ]
                : []),
            {
                id: "match",
                header: "自动匹配",
                meta: { label: "自动匹配" },
                cell: ({ row }) => <MatchKind row={row.original} />,
            },
            {
                id: "purchase",
                header: "接口采购报价",
                meta: { label: "接口采购报价", align: "end", numeric: true },
                cell: ({ row }) => (
                    <MoneyValue value={row.original.source.purchase_price} />
                ),
            },
            {
                id: "dropship",
                header: "公司代发价（含税）",
                meta: { label: "公司代发价", align: "end", numeric: true },
                cell: ({ row }) => (
                    <div>
                        <MoneyValue
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
                header: "状态与待处理原因",
                meta: { label: "状态与待处理原因" },
                cell: ({ row }) => <RowStatus row={row.original} />,
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
                meta: { label: "公司市场价", align: "end", numeric: true },
                cell: ({ row }) => (
                    <MoneyValue
                        value={row.original.sales_prices.market_price.value}
                    />
                ),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                enableSorting: false,
                cell: ({ row: tableRow }) => {
                    const row = tableRow.original
                    const key = toAutomationIdSegment(row.source.id)
                    if (row.status === "UNKNOWN")
                        return (
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
                        )
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
                            <div className="flex flex-wrap gap-1">
                                {productId ? (
                                    <Link
                                        id={`${prefix}-product-${key}`}
                                        href={`/master-data/products/${encodeURIComponent(productId)}`}
                                        className={buttonVariants({
                                            variant: "ghost",
                                            size: "sm",
                                        })}
                                    >
                                        打开公司商品
                                    </Link>
                                ) : null}
                                {offeringId ? (
                                    <Link
                                        id={`${prefix}-offering-${key}`}
                                        href={`/procurement/supplier-offerings/${encodeURIComponent(offeringId)}`}
                                        className={buttonVariants({
                                            variant: "ghost",
                                            size: "sm",
                                        })}
                                    >
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
                    return (
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
                    )
                },
            },
        ],
        [prefix, mode, busy, onEdit, onReconcile, view.rows],
    )
    return (
        <div className="min-w-0 space-y-3">
            {mode === "RECOVERY" && view.counts.unknown > 0 ? (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning-border bg-warning-soft px-4 py-3 text-sm text-warning-soft-foreground">
                    <p>
                        {view.counts.unknown} 个 SKU
                        的原操作结果待核实；查明前不能选择重试。
                    </p>
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
                toolbar={
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
                                                        row.status === status,
                                                ),
                                            )
                                            .map(([value, state]) => ({
                                                value,
                                                label: `${state.label}（${baseRows.filter((row) => row.status === value).length}）`,
                                            })),
                                    ]}
                                    allowClear={false}
                                    className="w-44"
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="reason">
                            {(field) => (
                                <field.SelectField
                                    id={`${prefix}-reason-filter`}
                                    label="待处理原因"
                                    options={[
                                        { value: "ALL", label: "全部原因" },
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
                        <Button
                            id={`${prefix}-reset-filters`}
                            type="button"
                            variant="ghost"
                            onClick={() =>
                                form.reset({ status: "ALL", reason: "ALL" })
                            }
                        >
                            重置筛选
                        </Button>
                    </form>
                }
                selectionBar={
                    <div className="flex flex-wrap items-center gap-3 text-sm">
                        <span className="num">
                            当前 {filteredRows.length} 个 SKU
                            {mode !== "RESULT"
                                ? ` · 已选 ${selectedCount} 个 SKU`
                                : ""}
                        </span>
                        {mode !== "RESULT" ? (
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
                        caption="批量入库逐项核对，金额为公司销售参考价（含税）"
                        manualPagination
                        manualSorting={false}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        pageSizeOptions={[20]}
                        defaultColumnVisibility={{
                            sourceCategory: false,
                            market: false,
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
                            mode === "RESULT"
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
