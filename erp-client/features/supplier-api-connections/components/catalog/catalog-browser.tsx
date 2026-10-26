"use client"

import * as React from "react"
import Link from "next/link"
import type { ColumnDef, RowSelectionState } from "@tanstack/react-table"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    CatalogAvailability,
    CatalogFilters,
    CatalogItem,
} from "../../api/catalog"
import type { AdoptionPreview, AdoptionSelection } from "../../api/adoption"
import { useCatalogQuery } from "../../hooks/queries"
import { usePreviewAdoptionMutation } from "../../hooks/adoption"
import { CatalogFilterForm } from "./catalog-filter-form"
import { CatalogAdoptionDialog } from "../dialogs/catalog-adoption-dialog"
import { CatalogSourceImage } from "./catalog-source-image"
import { catalogMediaBlocker } from "../../lib/catalog-media"
import { CatalogIdentityReviewDialog } from "../dialogs/catalog-identity-review-dialog"

const availabilityLabels: Record<CatalogAvailability, string> = {
    AVAILABLE: "可供",
    UNAVAILABLE: "暂不可供",
    UNKNOWN: "待核验",
    WITHDRAWN: "已下架",
}

export function CatalogBrowser({
    connectionId,
    supplierName,
    supplierFilter,
    supplierId,
    canAdopt,
    canReadCost,
    canReview,
}: {
    connectionId: string
    supplierName: string
    supplierFilter?: React.ReactNode
    supplierId: string
    canAdopt: boolean
    canReadCost: boolean
    canReview: boolean
}) {
    const [filters, setFilters] = React.useState<CatalogFilters>({})
    const [page, setPage] = React.useState(1)
    const [selection, setSelection] = React.useState<RowSelectionState>({})
    const [previewRow, setPreviewRow] = React.useState<CatalogItem | null>(null)
    const [adoption, setAdoption] = React.useState<AdoptionPreview | null>(null)
    const [identityItemId, setIdentityItemId] = React.useState<string | null>(
        null,
    )
    const lastFocusedRowId = React.useRef<string | null>(null)
    const query = useCatalogQuery(connectionId, page, filters)
    const preview = usePreviewAdoptionMutation()
    const selectedIds = Object.entries(selection)
        .filter(([, selected]) => selected)
        .map(([id]) => id)
    const openAdoption = React.useCallback(
        async (choice: AdoptionSelection) => {
            try {
                const result = await preview.mutateAsync({
                    connectionId,
                    selection: choice,
                })
                setAdoption(result)
            } catch {
                // 显示服务端的预览错误，保留原筛选及勾选。
            }
        },
        [connectionId, preview],
    )
    const columns = React.useMemo<ColumnDef<CatalogItem>[]>(
        () => [
            {
                accessorKey: "name",
                header: "供应商商品",
                meta: { label: "商品" },
                cell: ({ row }) => (
                    <div className="flex min-w-48 items-center gap-3">
                        {row.original.images?.[0] ? (
                            <CatalogSourceImage
                                image={row.original.images[0]}
                                alt=""
                                width={44}
                                height={44}
                                className="size-11 shrink-0 rounded-md border object-cover"
                            />
                        ) : (
                            <div className="size-11 shrink-0 rounded-md bg-muted" />
                        )}
                        <div>
                            <p className="font-medium">{row.original.name}</p>
                            <p className="text-xs text-muted-foreground">
                                {row.original.specification || "未标注规格"}
                            </p>
                            {catalogMediaBlocker(row.original) ? (
                                <p className="text-xs text-warning-soft-foreground">
                                    {catalogMediaBlocker(row.original)}
                                </p>
                            ) : null}
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
                header: "单位",
                meta: { label: "单位" },
                cell: ({ row }) => row.original.unit || "待确认",
            },
            {
                accessorKey: "purchase_price",
                header: "接口采购报价",
                meta: { label: "接口采购报价" },
                cell: ({ row }) =>
                    !canReadCost ? (
                        "无查看权限"
                    ) : row.original.purchase_price == null ? (
                        "未取得"
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
                    row.original.company_product_id ? (
                        <div className="space-y-1">
                            <p className="text-xs text-muted-foreground">
                                {row.original.requires_identity_review
                                    ? "规格或单位有变，请核对供给"
                                    : catalogMediaBlocker(row.original)
                                      ? "图片待核验，请同步后选品"
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
                        loading={preview.isPending}
                        disabled={
                            !canAdopt ||
                            preview.isPending ||
                            Boolean(catalogMediaBlocker(row.original)) ||
                            row.original.availability === "WITHDRAWN"
                        }
                        onClick={() =>
                            void openAdoption({ item_ids: [row.original.id] })
                        }
                    >
                        选品入库
                    </LoadingButton>
                ),
            },
        ],
        [canAdopt, canReadCost, canReview, preview.isPending, openAdoption],
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
            <BusinessTableFrame
                title={`${supplierName} · 商品库`}
                toolbar={
                    <CatalogFilterForm
                        supplierFilter={supplierFilter}
                        value={filters}
                        options={query.data?.filters}
                        loading={query.isFetching}
                        canReadCost={canReadCost}
                        onApply={(next) => {
                            setFilters(next)
                            setPage(1)
                            setSelection({})
                        }}
                    />
                }
                selectionBar={
                    <div className="flex flex-wrap items-center gap-2 text-sm">
                        <span className="text-muted-foreground">
                            共 {query.data?.total ?? 0} 项
                            {selectedIds.length
                                ? `，已选 ${selectedIds.length} 项`
                                : ""}
                        </span>
                        {selectedIds.length ? (
                            <>
                                <Button
                                    id="supplier-catalog-clear-selection"
                                    variant="ghost"
                                    onClick={() => setSelection({})}
                                >
                                    清空勾选
                                </Button>
                                <LoadingButton
                                    id="supplier-catalog-adopt-selected"
                                    disabled={!canAdopt || preview.isPending}
                                    loading={preview.isPending}
                                    onClick={() =>
                                        void openAdoption({
                                            item_ids: selectedIds,
                                        })
                                    }
                                >
                                    勾选商品入库
                                </LoadingButton>
                            </>
                        ) : null}
                    </div>
                }
                tableActions={
                    <LoadingButton
                        id="supplier-catalog-adopt-filtered"
                        variant="outline"
                        disabled={
                            !canAdopt ||
                            !query.data?.total ||
                            query.isFetching ||
                            preview.isPending
                        }
                        loading={preview.isPending}
                        onClick={() => void openAdoption({ filter: filters })}
                    >
                        {filters.category_id
                            ? "当前分类及筛选结果入库"
                            : "当前筛选结果入库"}
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
                            row.original.availability !== "WITHDRAWN" &&
                            !catalogMediaBlocker(row.original)
                        }
                        rowSelection={selection}
                        showToolbarSummary={false}
                        onRowSelectionChange={setSelection}
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
                                    Object.values(filters).some(Boolean)
                                        ? "当前筛选没有商品"
                                        : "尚未同步商品"
                                }
                                description={
                                    Object.values(filters).some(Boolean)
                                        ? "调整分类、品牌或其它筛选条件后重试。"
                                        : "完成接口配置后同步供应商商品，再选择需要经营的商品入库。"
                                }
                            />
                        }
                    />
                }
                footer={
                    <p className="text-xs text-muted-foreground">
                        勾选跨页保留，变更筛选会清空勾选。当前筛选结果由系统确定完整范围后预览，最多
                        500 项；超过时请缩小分类或其它条件。
                    </p>
                }
            />
            {preview.error ? (
                <p role="alert" className="text-sm text-destructive">
                    {preview.error.message}
                </p>
            ) : null}
            {identityItemId ? (
                <CatalogIdentityReviewDialog
                    connectionId={connectionId}
                    itemId={identityItemId}
                    onClose={() => setIdentityItemId(null)}
                />
            ) : null}
            {adoption ? (
                <CatalogAdoptionDialog
                    connectionId={connectionId}
                    supplierId={supplierId}
                    preview={adoption}
                    onClose={(completed) => {
                        setAdoption(null)
                        if (completed) setSelection({})
                    }}
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
                                    !canReadCost
                                        ? "无采购报价查看权限"
                                        : previewRow.purchase_price == null
                                          ? "未取得"
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
                            <dt className="text-muted-foreground">单位</dt>
                            <dd>{previewRow.unit || "待确认"}</dd>
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
                            { title: "商品图片", images: previewRow.images },
                            {
                                title: "详情图片",
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
