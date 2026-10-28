"use client"

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
    QuickPreviewSheet,
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
import { CatalogSourceImage } from "./catalog-source-image"
import { catalogMediaBlocker } from "../../lib/catalog-media"
import { CatalogIdentityReviewDialog } from "../dialogs/catalog-identity-review-dialog"
import { CatalogBatchHistory } from "./catalog-batch-history"

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
                            {row.original.requires_identity_review ? (
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
                cell: ({ row }) => (
                    <LoadingButton
                        id={`supplier-catalog-adopt-${toAutomationIdSegment(row.original.id)}`}
                        size="sm"
                        variant="outline"
                        aria-describedby={
                            selectionBlocker
                                ? "supplier-catalog-adoption-blocker"
                                : undefined
                        }
                        loading={choice.creating}
                        disabled={!canConfigure || choice.busy}
                        onClick={() => void choice.configure([row.original.id])}
                    >
                        配置入库
                    </LoadingButton>
                ),
            },
        ],
        [
            canConfigure,
            selectionBlocker,
            canReadCost,
            canReview,
            query.data?.can_read_cost,
            choice,
        ],
    )
    const closePreview = () => {
        setPreviewRow(null)
        if (lastFocusedRowId.current) {
            document
                .querySelector<HTMLElement>(
                    `[data-row-id="${CSS.escape(lastFocusedRowId.current)}"]`,
                )
                ?.focus()
        }
    }
    return (
        <>
            {selectionBlocker ? (
                <div
                    role="alert"
                    className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warning-border bg-warning-soft p-4 text-sm text-warning-soft-foreground"
                >
                    <p id="supplier-catalog-adoption-blocker">
                        {selectionBlocker}
                    </p>
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
                                choice.clear(
                                    choice.selectedIds.length || choice.frozen
                                        ? "筛选条件已变更，原选择与排除项已清空，请重新选择。"
                                        : undefined,
                                )
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
                        {choice.frozen ? (
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
                        {!choice.frozen && query.data?.total ? (
                            <LoadingButton
                                id="supplier-catalog-batch-select-filtered"
                                variant="outline"
                                disabled={
                                    !canConfigure ||
                                    query.isFetching ||
                                    choice.busy
                                }
                                loading={choice.creating}
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
                    <LoadingButton
                        aria-describedby={
                            selectionBlocker
                                ? "supplier-catalog-adoption-blocker"
                                : undefined
                        }
                        id="supplier-catalog-batch-configure"
                        disabled={
                            !canConfigure ||
                            !choice.selectedIds.length ||
                            choice.needsReload ||
                            choice.busy
                        }
                        loading={choice.creating}
                        onClick={() => void choice.configure()}
                    >
                        配置入库（{choice.selectedIds.length}）
                    </LoadingButton>
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
                footer={
                    <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                        <span>
                            {choice.productCount} 个商品 ·{" "}
                            {choice.selectedIds.length} 个 SKU 已选
                        </span>
                        <span>
                            表头只选择当前页；翻页保留选择，变更筛选将清空选择与排除项。
                        </span>
                    </div>
                }
            />
            {canConfigure ? (
                <CatalogBatchHistory connectionId={connectionId} />
            ) : null}
            {choice.notice ? (
                <p role="status" className="text-sm text-muted-foreground">
                    {choice.notice}
                </p>
            ) : null}
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
            <QuickPreviewSheet
                idPrefix="supplier-catalog-preview"
                open={Boolean(previewRow)}
                onOpenChange={(open) => {
                    if (!open) closePreview()
                }}
                size="preview"
                title={previewRow?.name ?? "供应商商品"}
                description={previewRow?.specification || undefined}
                identity={
                    previewRow ? (
                        <span className="num">
                            供应商订货编码：{previewRow.spec_id}
                        </span>
                    ) : null
                }
                summary={
                    previewRow ? (
                        <Badge
                            variant={
                                previewRow.availability === "AVAILABLE"
                                    ? "success"
                                    : "secondary"
                            }
                        >
                            {availabilityLabels[previewRow.availability] ??
                                "待核验"}
                        </Badge>
                    ) : null
                }
                footer={
                    <>
                        <Button
                            id="supplier-catalog-preview-close"
                            variant="outline"
                            onClick={closePreview}
                        >
                            关闭
                        </Button>
                        {previewRow?.company_product_id ? (
                            <Button
                                id="supplier-catalog-preview-open-company"
                                render={
                                    <Link
                                        href={`/master-data/products/${encodeURIComponent(previewRow.company_product_id)}`}
                                    />
                                }
                            >
                                打开公司商品
                            </Button>
                        ) : null}
                    </>
                }
            >
                {previewRow ? (
                    <div className="space-y-5">
                        <div>
                            <p className="text-xs text-muted-foreground">
                                接口采购报价
                            </p>
                            <MoneyValue
                                value={previewRow.purchase_price}
                                unavailableReason={
                                    !canReadCost ||
                                    query.data?.can_read_cost === false
                                        ? "无采购报价查看权限"
                                        : previewRow.purchase_price == null
                                          ? "暂无接口报价"
                                          : undefined
                                }
                                size="section"
                            />
                        </div>
                        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
                            <dt className="text-muted-foreground">分类</dt>
                            <dd>{previewRow.category_name || "未提供"}</dd>
                            <dt className="text-muted-foreground">品牌</dt>
                            <dd>{previewRow.brand_name || "未提供"}</dd>
                            <dt className="text-muted-foreground">商品单位</dt>
                            <dd>{catalogUnitLabel(previewRow)}</dd>
                            <dt className="text-muted-foreground">可供数量</dt>
                            <dd>
                                {previewRow.quantity_unbounded
                                    ? "供应商未设数量上限"
                                    : (previewRow.available_quantity ??
                                      "未提供")}
                            </dd>
                        </dl>
                        {previewRow.description ? (
                            <section className="space-y-2">
                                <h3 className="text-sm font-medium">
                                    商品介绍
                                </h3>
                                <p className="whitespace-pre-wrap break-words text-sm leading-6 text-muted-foreground">
                                    {previewRow.description}
                                </p>
                            </section>
                        ) : null}
                        {[
                            { title: "商品轮播图", images: previewRow.images },
                            {
                                title: "商品详情图",
                                images: previewRow.detail_images,
                            },
                        ].map(({ title, images }) =>
                            images?.length ? (
                                <section key={title} className="space-y-2">
                                    <h3 className="text-sm font-medium">
                                        {title}
                                    </h3>
                                    <div className="space-y-2">
                                        {images.map((image, index) => (
                                            <CatalogSourceImage
                                                key={`${image.url}-${index}`}
                                                image={image}
                                                alt={`${title} ${index + 1}`}
                                                width={800}
                                                height={800}
                                                className="h-auto w-full rounded-md border object-contain"
                                            />
                                        ))}
                                    </div>
                                </section>
                            ) : null,
                        )}
                    </div>
                ) : null}
            </QuickPreviewSheet>
        </>
    )
}
