"use client"

import { BatchWorkStatus } from "../batch/batch-work-status"
import { BatchPreparationStatus } from "../batch/batch-preparation"

import { catalogUnitLabel } from "../../api/catalog"

import * as React from "react"
import Link from "next/link"
import { TriangleAlertIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataTable,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    HoverCard,
    HoverCardContent,
    HoverCardTrigger,
} from "@/components/ui/hover-card"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { restoreListRowFocus } from "@/lib/list-row-focus"
import type {
    CatalogAvailability,
    CatalogFilters,
    CatalogItem,
} from "../../api/catalog"
import { useCatalogQuery } from "../../hooks/queries"
import {
    catalogFilterKey,
    useCatalogBatchSelection,
} from "../../hooks/use-catalog-batch-selection"
import { CatalogFilterForm } from "./catalog-filter-form"
import { CatalogPreviewSheet } from "./catalog-preview-sheet"
import { CatalogSourceImage } from "./catalog-source-image"
import { catalogMediaBlocker } from "../../lib/catalog-media"
import { CatalogIdentityReviewDialog } from "../dialogs/catalog-identity-review-dialog"

const availabilityLabels: Record<CatalogAvailability, string> = {
    AVAILABLE: "可供",
    UNAVAILABLE: "暂不可供",
    UNKNOWN: "待核验",
    WITHDRAWN: "已下架",
}

function CatalogMediaWarning({ item }: { item: CatalogItem }) {
    const message = catalogMediaBlocker(item)
    if (!message) return null

    return (
        <HoverCard>
            <HoverCardTrigger
                id={`supplier-catalog-media-warning-${toAutomationIdSegment(item.id)}`}
                delay={150}
                closeDelay={150}
                aria-label={`${item.name}的图片异常`}
                className="inline-flex size-6 shrink-0 items-center justify-center rounded text-warning-soft-foreground outline-none focus-visible:ring-2 focus-visible:ring-ring"
                onClick={(event) => event.stopPropagation()}
                render={
                    <button
                        type="button"
                        aria-label={`${item.name}的图片异常`}
                    />
                }
            >
                <TriangleAlertIcon className="size-4" aria-hidden="true" />
            </HoverCardTrigger>
            <HoverCardContent
                align="start"
                className="max-h-64 max-w-[min(20rem,calc(100vw-2rem))] overflow-y-auto rounded-xl text-xs leading-relaxed break-words"
            >
                <p>{message}</p>
            </HoverCardContent>
        </HoverCard>
    )
}

export function CatalogBrowser({
    connectionId,
    supplierName,
    canAdopt,
    adoptionBlocker,
    settingsHref,
    canReadCost,
    canReview,
}: {
    connectionId: string
    supplierName: string
    supplierId: string
    canAdopt: boolean
    adoptionBlocker?: string
    settingsHref: string
    canReadCost: boolean
    canReview: boolean
}) {
    const [filters, setFilters] = React.useState<CatalogFilters>({})
    const [page, setPage] = React.useState(1)
    const [previewRow, setPreviewRow] = React.useState<CatalogItem | null>(null)
    const [identityItemId, setIdentityItemId] = React.useState<string | null>(
        null,
    )
    const lastFocusedRowId = React.useRef<string | null>(null)
    const query = useCatalogQuery(connectionId, page, filters)
    const choice = useCatalogBatchSelection(connectionId)
    const selectionBlocker =
        adoptionBlocker ??
        (!canReadCost || query.data?.can_read_cost === false
            ? "当前账号没有接口采购报价权限，不能配置入库。"
            : !canAdopt
              ? "当前账号没有选品入库权限，请联系管理员。"
              : undefined)
    const canSelect =
        canAdopt && canReadCost && query.data?.can_read_cost !== false
    const canConfigure =
        !selectionBlocker &&
        canAdopt &&
        canReadCost &&
        query.isSuccess &&
        query.data.can_read_cost !== false
    const columns = React.useMemo<ColumnDef<CatalogItem>[]>(
        () => [
            {
                accessorKey: "name",
                header: "供应商商品",
                meta: { label: "商品" },
                cell: ({ row }) => (
                    <div className="flex min-w-48 items-center gap-3">
                        <div className="size-11 shrink-0 overflow-hidden rounded-md bg-muted">
                            {row.original.images?.[0] ? (
                                <CatalogSourceImage
                                    image={row.original.images[0]}
                                    alt=""
                                    width={44}
                                    height={44}
                                    compact
                                    className="size-full rounded-md border object-cover"
                                />
                            ) : null}
                        </div>
                        <div>
                            <div className="flex items-center gap-1">
                                <p className="font-medium">
                                    {row.original.name}
                                </p>
                                <CatalogMediaWarning item={row.original} />
                            </div>
                            <p className="text-xs text-muted-foreground">
                                {row.original.specification || "未标注规格"}
                            </p>
                            <p className="num mt-1 text-xs text-muted-foreground">
                                供应商订货编码：{row.original.spec_id}
                            </p>
                        </div>
                    </div>
                ),
            },
            {
                accessorKey: "category_name",
                header: "供应商分类",
                meta: { label: "供应商分类" },
                cell: ({ row }) =>
                    row.original.category_name ||
                    row.original.category_id ||
                    "未提供",
            },
            {
                accessorKey: "brand_name",
                header: "品牌",
                meta: { label: "品牌" },
                cell: ({ row }) => row.original.brand_name || "未提供",
            },
            {
                accessorKey: "unit",
                header: "商品单位",
                meta: { label: "商品单位" },
                cell: ({ row }) => catalogUnitLabel(row.original),
            },
            {
                accessorKey: "purchase_price",
                header: "接口采购报价",
                meta: { label: "接口采购报价", align: "end", numeric: true },
                cell: ({ row }) =>
                    !canReadCost || query.data?.can_read_cost === false ? (
                        "无查看权限"
                    ) : row.original.purchase_price == null ? (
                        "暂无接口报价"
                    ) : (
                        <MoneyValue
                            value={row.original.purchase_price}
                            display="exact"
                        />
                    ),
            },
            {
                id: "purchase-terms",
                header: "采购要求",
                meta: { label: "采购要求" },
                cell: ({ row }) => {
                    const terms = row.original.source_purchase_terms
                    if (!terms) return "未取得"
                    return (
                        <div className="space-y-1 text-xs">
                            <p>
                                起购：
                                {terms.minimum_quantity ? (
                                    <QuantityValue
                                        value={terms.minimum_quantity}
                                        unit={row.original.unit ?? ""}
                                    />
                                ) : (
                                    "未取得"
                                )}
                            </p>
                            <p>
                                倍数：
                                {terms.quantity_multiple ? (
                                    <QuantityValue
                                        value={terms.quantity_multiple}
                                        unit={row.original.unit ?? ""}
                                    />
                                ) : (
                                    "未取得"
                                )}
                            </p>
                            {terms.restrictions.length > 0 ? (
                                <Badge variant="warning">含附加采购条件</Badge>
                            ) : null}
                        </div>
                    )
                },
            },
            {
                accessorKey: "availability",
                header: "可供状态",
                meta: { label: "可供状态" },
                cell: ({ row }) => (
                    <Badge
                        variant={
                            row.original.availability === "AVAILABLE"
                                ? "success"
                                : row.original.availability === "UNKNOWN"
                                  ? "warning"
                                  : "secondary"
                        }
                    >
                        {availabilityLabels[row.original.availability] ??
                            "待核验"}
                    </Badge>
                ),
            },
            {
                id: "selected",
                header: "公司商品",
                meta: { label: "公司商品" },
                cell: ({ row }) =>
                    choice.isExcluded(row.original.id) ? (
                        <Badge variant="secondary">已排除</Badge>
                    ) : row.original.company_product_id ? (
                        <div className="space-y-1">
                            <p className="text-xs text-muted-foreground">
                                {row.original.requires_identity_review
                                    ? "规格或单位有变，请核对供给"
                                    : catalogMediaBlocker(row.original)
                                      ? "部分图片尚未同步"
                                      : row.original.offering_id
                                        ? "已入库"
                                        : "已匹配，待入库"}
                            </p>
                            {row.original.requires_identity_review &&
                            canReview ? (
                                <Button
                                    id={`supplier-catalog-identity-${toAutomationIdSegment(row.original.id)}`}
                                    variant="outline"
                                    size="sm"
                                    disabled={!canReview}
                                    onClick={(event) => {
                                        event.stopPropagation()
                                        setIdentityItemId(row.original.id)
                                    }}
                                >
                                    核对规格
                                </Button>
                            ) : null}
                            <Link
                                id={`supplier-catalog-product-${toAutomationIdSegment(row.original.id)}`}
                                href={`/master-data/products/${encodeURIComponent(row.original.company_product_id)}`}
                                className="text-primary underline"
                            >
                                {row.original.sku_no || "打开公司商品"}
                            </Link>
                        </div>
                    ) : (
                        "未入库"
                    ),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                cell: ({ row }) =>
                    canSelect ? (
                        <LoadingButton
                            id={`supplier-catalog-adopt-${toAutomationIdSegment(row.original.id)}`}
                            size="sm"
                            variant="outline"
                            aria-describedby={
                                selectionBlocker
                                    ? "supplier-catalog-adoption-blocker"
                                    : undefined
                            }
                            loading={choice.configuring}
                            disabled={
                                !canConfigure || choice.busy || choice.preparing
                            }
                            onClick={() =>
                                void choice.configure([row.original.id])
                            }
                        >
                            配置入库
                        </LoadingButton>
                    ) : null,
            },
        ],
        [
            canConfigure,
            canSelect,
            selectionBlocker,
            canReadCost,
            canReview,
            query.data?.can_read_cost,
            choice,
        ],
    )
    const closePreview = () => {
        setPreviewRow(null)
        restoreListRowFocus(lastFocusedRowId.current)
    }
    return (
        <>
            {selectionBlocker ? (
                <div
                    role="alert"
                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning-border bg-warning-soft p-4 text-sm text-warning-soft-foreground"
                >
                    {canSelect && (
                        <p id="supplier-catalog-adoption-blocker">
                            {selectionBlocker}
                        </p>
                    )}
                    <Button
                        id="supplier-catalog-adoption-settings"
                        variant="outline"
                        size="sm"
                        render={<Link href={settingsHref} />}
                    >
                        前往 API 设置
                    </Button>
                </div>
            ) : null}
            <BusinessTableFrame
                title={`${supplierName} · 商品库`}
                toolbar={
                    <CatalogFilterForm
                        key={catalogFilterKey(filters)}
                        value={filters}
                        options={query.data?.filters}
                        loading={query.isFetching || choice.busy}
                        canReadCost={
                            canReadCost && query.data?.can_read_cost !== false
                        }
                        onApply={(next) => {
                            if (
                                catalogFilterKey(filters) !==
                                catalogFilterKey(next)
                            ) {
                                choice.clear()
                                setFilters(next)
                                setPage(1)
                            }
                        }}
                    />
                }
                selectionBar={
                    <div className="flex flex-wrap items-center gap-2 text-sm">
                        <span className="font-medium">
                            共{" "}
                            {query.data?.product_total != null
                                ? `${query.data.product_total} 个商品 · `
                                : ""}
                            {query.data?.total ?? "—"} 个 SKU
                        </span>
                        <span className="text-muted-foreground">
                            本页 {query.data?.items.length ?? 0} 个 SKU
                        </span>
                        {choice.preparing && choice.frozen ? (
                            choice.frozen.batch.work &&
                            choice.frozen.batch.work.status !== "COMPLETED" ? (
                                <BatchWorkStatus
                                    batch={choice.frozen.batch}
                                    inCatalog
                                />
                            ) : (
                                <BatchPreparationStatus
                                    batch={choice.frozen.batch}
                                />
                            )
                        ) : choice.frozen ? (
                            <span className="rounded-md bg-success-soft px-3 py-2 text-success-soft-foreground">
                                已选择全部筛选结果 {choice.frozen.rows.length}{" "}
                                个 SKU · 已排除 {choice.excludedRows.length} 个
                                · 最终选中 {choice.selectedIds.length} 个
                            </span>
                        ) : choice.selectedIds.length ? (
                            <span>
                                已选 {choice.productCount} 个商品 ·{" "}
                                {choice.selectedIds.length} 个 SKU
                            </span>
                        ) : null}
                        {!choice.frozen && query.data?.total && canSelect ? (
                            <LoadingButton
                                id="supplier-catalog-batch-select-filtered"
                                variant="outline"
                                disabled={
                                    !canConfigure ||
                                    query.isFetching ||
                                    choice.busy
                                }
                                loading={choice.selecting}
                                onClick={() => void choice.freezeAll(filters)}
                            >
                                选择全部 {query.data.total} 个 SKU
                            </LoadingButton>
                        ) : null}
                        {choice.excludedRows.length ? (
                            <Button
                                id="supplier-catalog-batch-view-excluded"
                                variant="ghost"
                                onClick={() =>
                                    choice.setShowExcluded(!choice.showExcluded)
                                }
                            >
                                {choice.showExcluded
                                    ? "收起排除项"
                                    : `查看排除项（${choice.excludedRows.length}）`}
                            </Button>
                        ) : null}
                        {choice.selectedIds.length || choice.frozen ? (
                            <Button
                                id="supplier-catalog-clear-selection"
                                variant="ghost"
                                disabled={choice.busy}
                                onClick={() => choice.clear()}
                            >
                                清空选择
                            </Button>
                        ) : null}
                        {choice.saving ? (
                            <span
                                role="status"
                                className="text-muted-foreground"
                            >
                                正在保存选择…
                            </span>
                        ) : null}
                    </div>
                }
                tableActions={
                    canSelect ? (
                        <LoadingButton
                            aria-describedby={
                                selectionBlocker
                                    ? "supplier-catalog-adoption-blocker"
                                    : undefined
                            }
                            id="supplier-catalog-batch-configure"
                            disabled={
                                !canConfigure ||
                                choice.preparing ||
                                !choice.selectedIds.length ||
                                choice.needsReload ||
                                choice.busy
                            }
                            loading={choice.configuring}
                            onClick={() => void choice.configure()}
                        >
                            配置入库（{choice.selectedIds.length}）
                        </LoadingButton>
                    ) : null
                }
                table={
                    <DataTable
                        id="supplier-api-connections-catalog-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => row.name}
                        loading={query.isPending}
                        rowCount={query.data?.total ?? 0}
                        pagination={{ pageIndex: page - 1, pageSize: 20 }}
                        onPaginationChange={(next) =>
                            setPage(next.pageIndex + 1)
                        }
                        enableRowSelection={(row) =>
                            canConfigure &&
                            !choice.busy &&
                            !choice.preparing &&
                            !choice.needsReload &&
                            choice.isFrozenMember(row.original.id)
                        }
                        rowSelection={choice.rowSelection}
                        showToolbarSummary={false}
                        onRowSelectionChange={(next) =>
                            choice.change(next, query.data?.items ?? [])
                        }
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.id
                            setPreviewRow(row)
                        }}
                        highlightedRowId={previewRow?.id}
                        caption="供应商品库"
                        layout="flush"
                        errorState={
                            query.isError ? (
                                <BusinessFailureState
                                    title="供应商品库加载失败"
                                    description={query.error.message}
                                    onRetry={() => void query.refetch()}
                                />
                            ) : undefined
                        }
                        emptyState={
                            <BusinessEmptyState
                                kind="no-data"
                                title={
                                    Object.values(filters).some((value) =>
                                        Array.isArray(value)
                                            ? value.length > 0
                                            : Boolean(value),
                                    )
                                        ? "当前筛选没有商品"
                                        : "尚未同步商品"
                                }
                                description={
                                    Object.values(filters).some((value) =>
                                        Array.isArray(value)
                                            ? value.length > 0
                                            : Boolean(value),
                                    )
                                        ? "调整分类、品牌或其它筛选条件后重试。"
                                        : "完成接口配置后同步供应商商品，再选择需要经营的商品入库。"
                                }
                            />
                        }
                    />
                }
            />
            {choice.error ? (
                <div
                    role="alert"
                    className="flex flex-wrap items-center justify-between gap-3 text-sm"
                >
                    <p className="text-destructive">
                        选择保存或读取失败：{choice.error.message}
                        。页面保留上次确认的选择，
                        {choice.frozen
                            ? "请重新读取后核对。"
                            : "请重试刚才的选择。"}
                    </p>
                    {choice.frozen ? (
                        <Button
                            id="supplier-catalog-batch-selection-reload"
                            variant="outline"
                            size="sm"
                            disabled={choice.busy}
                            onClick={() => void choice.refreshFrozen()}
                        >
                            重新读取选择
                        </Button>
                    ) : null}
                </div>
            ) : null}
            {choice.showExcluded ? (
                <section
                    aria-labelledby="supplier-catalog-batch-excluded-title"
                    className="space-y-3 rounded-lg border p-4"
                >
                    <h3
                        id="supplier-catalog-batch-excluded-title"
                        className="text-sm font-medium"
                    >
                        已排除 {choice.excludedRows.length} 个 SKU
                    </h3>
                    <div className="max-h-80 space-y-2 overflow-y-auto">
                        {choice.excludedRows.map((row) => (
                            <div
                                key={row.source.id}
                                className="flex flex-wrap items-center justify-between gap-3 border-b pb-2 text-sm"
                            >
                                <div>
                                    <p>{row.source.name}</p>
                                    <p className="text-xs text-muted-foreground">
                                        {row.source.specification ||
                                            "未标注规格"}{" "}
                                        · 订货编码 {row.source.spec_id}
                                    </p>
                                </div>
                                <Button
                                    id={`supplier-catalog-batch-restore-${toAutomationIdSegment(row.source.id)}`}
                                    size="sm"
                                    variant="outline"
                                    disabled={choice.busy}
                                    onClick={() =>
                                        void choice.restore(row.source.id)
                                    }
                                >
                                    恢复选择
                                </Button>
                            </div>
                        ))}
                    </div>
                </section>
            ) : null}
            {identityItemId ? (
                <CatalogIdentityReviewDialog
                    connectionId={connectionId}
                    itemId={identityItemId}
                    onClose={() => setIdentityItemId(null)}
                />
            ) : null}
            {previewRow ? (
                <CatalogPreviewSheet
                    key={previewRow.id}
                    item={previewRow}
                    onClose={closePreview}
                    canReadCost={
                        canReadCost && query.data?.can_read_cost !== false
                    }
                    canConfigure={canConfigure}
                    selectionBlocker={selectionBlocker}
                    choice={choice}
                />
            ) : null}
        </>
    )
}
