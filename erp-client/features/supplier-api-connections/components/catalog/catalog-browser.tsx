"use client"

import { BatchWorkStatus } from "../batch/batch-work-status"
import { BatchPreparationStatus } from "../batch/batch-preparation"

import { catalogUnitLabel } from "../../api/catalog"

import * as React from "react"
import Link from "next/link"
import { TriangleAlertIcon } from "lucide-react"
import type { ColumnDef } from "@tanstack/react-table"
import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    ListWorkSurface,
    listWorkspaceStyles,
    MoneyValue,
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { Alert } from "@/components/ui/alert"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
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
                className="max-h-64 max-w-[min(20rem,calc(100vw-2rem))] overflow-y-auto rounded-lg text-xs break-words"
            >
                <p>{message}</p>
            </HoverCardContent>
        </HoverCard>
    )
}

export function CatalogBrowser({
    connectionId,
    title,
    canAdopt,
    adoptionBlocker,
    settingsHref,
    canReadCost,
    canReview,
}: {
    connectionId: string
    title: string
    canAdopt: boolean
    adoptionBlocker?: string
    settingsHref: string
    canReadCost: boolean
    canReview: boolean
}) {
    const [appliedFilters, setFilters] = React.useState<CatalogFilters>({})
    const [page, setPage] = React.useState(1)
    const [previewRow, setPreviewRow] = React.useState<CatalogItem | null>(null)
    const [identityItemId, setIdentityItemId] = React.useState<string | null>(
        null,
    )
    const lastFocusedRowId = React.useRef<string | null>(null)
    const profile = useAccountProfileQuery()
    const canOpenProduct = hasPermission(
        profile.data?.permissions,
        "product:detail",
    )
    const hasHiddenPriceFilter =
        !canReadCost &&
        Boolean(appliedFilters.price_min || appliedFilters.price_max)
    const filters = React.useMemo(
        () =>
            hasHiddenPriceFilter
                ? {
                      ...appliedFilters,
                      price_min: undefined,
                      price_max: undefined,
                  }
                : appliedFilters,
        [appliedFilters, hasHiddenPriceFilter],
    )
    const query = useCatalogQuery(
        connectionId,
        hasHiddenPriceFilter ? 1 : page,
        filters,
    )
    const choice = useCatalogBatchSelection(connectionId)
    const showCosts = canReadCost && query.data?.can_read_cost !== false
    const canSelect = canAdopt && showCosts
    const selectionBlocker = canSelect ? adoptionBlocker : undefined
    const canConfigure = canSelect && !selectionBlocker && query.isSuccess
    const canConfigureBatch =
        canConfigure &&
        !choice.preparing &&
        choice.selectedIds.length > 0 &&
        !choice.needsReload &&
        !choice.busy
    const hasActiveFilters = catalogFilterKey(filters) !== catalogFilterKey({})
    const applyFilters = (next: CatalogFilters) => {
        if (catalogFilterKey(filters) === catalogFilterKey(next)) return
        choice.clear()
        setFilters(next)
        setPage(1)
    }
    React.useEffect(() => {
        if (
            showCosts ||
            (!appliedFilters.price_min && !appliedFilters.price_max)
        )
            return
        setFilters((current) => ({
            ...current,
            price_min: undefined,
            price_max: undefined,
        }))
        setPage(1)
    }, [appliedFilters.price_min, appliedFilters.price_max, showCosts])
    const columns = React.useMemo<ColumnDef<CatalogItem>[]>(
        () => [
            {
                accessorKey: "name",
                header: "供应商商品",
                meta: { label: "商品", width: "flex" },
                cell: ({ row: { original: item } }) => {
                    const reference = `订货编码：${item.spec_id}`
                    const specification = item.specification || "未标注规格"
                    const secondary = `${reference} · ${specification}`
                    return (
                        <div className="flex min-w-0 items-center gap-2">
                            <div className="size-9 shrink-0 overflow-hidden rounded-md bg-muted">
                                {item.images?.[0] ? (
                                    <CatalogSourceImage
                                        image={item.images[0]}
                                        alt=""
                                        width={36}
                                        height={36}
                                        compact
                                        className="size-full object-cover"
                                    />
                                ) : null}
                            </div>
                            <TableIdentityCell
                                className="flex-1"
                                primary={item.name}
                                primaryTitle={item.name}
                                secondary={
                                    <>
                                        <span className="num">{reference}</span>{" "}
                                        · {specification}
                                    </>
                                }
                                secondaryTitle={secondary}
                            />
                            <CatalogMediaWarning item={item} />
                        </div>
                    )
                },
            },
            {
                accessorKey: "category_name",
                header: "供应商分类",
                meta: { label: "供应商分类", width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={
                            row.original.category_name ||
                            row.original.category_id
                        }
                        clamp={1}
                    />
                ),
            },
            {
                accessorKey: "brand_name",
                header: "品牌",
                meta: { label: "品牌", width: "default" },
                cell: ({ row }) => (
                    <TableTextCell value={row.original.brand_name} clamp={1} />
                ),
            },
            {
                accessorKey: "unit",
                header: "商品单位",
                meta: { label: "商品单位", width: "status" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={catalogUnitLabel(row.original)}
                        clamp={1}
                    />
                ),
            },
            ...(showCosts
                ? ([
                      {
                          accessorKey: "purchase_price",
                          header: "接口采购报价",
                          meta: {
                              label: "接口采购报价",
                              align: "end",
                              numeric: true,
                              width: "amount",
                          },
                          cell: ({ row }) =>
                              row.original.purchase_price == null ? (
                                  <TableEmptyCell
                                      variant="label"
                                      label="暂无接口报价"
                                  />
                              ) : (
                                  <MoneyValue
                                      value={row.original.purchase_price}
                                      display="exact"
                                  />
                              ),
                      },
                  ] satisfies ColumnDef<CatalogItem>[])
                : []),
            {
                id: "purchase-terms",
                header: "采购要求",
                meta: { label: "采购要求", width: "default" },
                cell: ({ row }) => {
                    const terms = row.original.source_purchase_terms
                    if (!terms)
                        return <TableEmptyCell variant="label" label="未取得" />
                    return (
                        <div className="flex flex-col items-start gap-1">
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
                                <BusinessStatusBadge
                                    context="list"
                                    tone="warning"
                                    label="含附加采购条件"
                                />
                            ) : null}
                        </div>
                    )
                },
            },
            {
                accessorKey: "availability",
                header: "可供状态",
                meta: { label: "可供状态", width: "status" },
                cell: ({ row }) => {
                    const availability = row.original.availability
                    return (
                        <BusinessStatusBadge
                            context="list"
                            appearance={
                                availability === "AVAILABLE" ? "quiet" : "badge"
                            }
                            tone={
                                availability === "AVAILABLE"
                                    ? "success"
                                    : availability === "UNKNOWN"
                                      ? "warning"
                                      : "neutral"
                            }
                            label={availabilityLabels[availability] ?? "待核验"}
                        />
                    )
                },
            },
            {
                id: "selected",
                header: "公司商品",
                meta: { label: "公司商品", width: "default" },
                cell: ({ row }) =>
                    choice.isExcluded(row.original.id) ? (
                        <BusinessStatusBadge
                            context="list"
                            tone="neutral"
                            label="已排除"
                        />
                    ) : row.original.company_product_id ? (
                        <div className="flex min-w-0 flex-col items-start gap-1">
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
                                    variant="ghost"
                                    className={listWorkspaceStyles.quietButton}
                                    size="xs"
                                    onClick={(event) => {
                                        event.stopPropagation()
                                        lastFocusedRowId.current =
                                            row.original.id
                                        setIdentityItemId(row.original.id)
                                    }}
                                >
                                    核对规格
                                </Button>
                            ) : null}
                            {canOpenProduct ? (
                                <Link
                                    id={`supplier-catalog-product-${toAutomationIdSegment(row.original.id)}`}
                                    href={`/master-data/products/${encodeURIComponent(row.original.company_product_id)}`}
                                    className="num max-w-full truncate text-primary underline"
                                    onClick={(event) => event.stopPropagation()}
                                >
                                    {row.original.sku_no || "打开公司商品"}
                                </Link>
                            ) : row.original.sku_no ? (
                                <span className="num">
                                    {row.original.sku_no}
                                </span>
                            ) : null}
                        </div>
                    ) : (
                        <BusinessStatusBadge
                            context="list"
                            tone="neutral"
                            appearance="quiet"
                            label="未入库"
                        />
                    ),
            },
            ...(canSelect
                ? ([
                      {
                          id: "actions",
                          header: "操作",
                          enableHiding: false,
                          meta: {
                              label: "操作",
                              align: "end",
                              width: "status",
                          },
                          cell: ({ row }) => {
                              const segment = toAutomationIdSegment(
                                  row.original.id,
                              )
                              return (
                                  <TableRowActions
                                      moreId={`supplier-catalog-${segment}-more`}
                                      moreLabel={`${row.original.name}更多操作`}
                                      actions={[
                                          {
                                              id: `supplier-catalog-adopt-${segment}`,
                                              label: "配置入库",
                                              loading: choice.configuring,
                                              disabled:
                                                  !canConfigure ||
                                                  choice.busy ||
                                                  choice.preparing,
                                              disabledReason: selectionBlocker,
                                              onClick: () =>
                                                  void choice.configure([
                                                      row.original.id,
                                                  ]),
                                          },
                                      ]}
                                  />
                              )
                          },
                      },
                  ] satisfies ColumnDef<CatalogItem>[])
                : []),
        ],
        [
            canConfigure,
            canSelect,
            selectionBlocker,
            canReview,
            canOpenProduct,
            showCosts,
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
                <Alert
                    variant="warning"
                    role="alert"
                    className="flex flex-wrap items-center justify-between gap-3"
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
                        前往平台设置
                    </Button>
                </Alert>
            ) : null}
            {choice.error ? (
                <BusinessFailureState
                    title="选择保存或读取失败"
                    description={`${choice.error.message}。页面保留上次确认的选择，${choice.frozen ? "请重新读取后核对。" : "请重试刚才的选择。"}`}
                    action={
                        choice.frozen ? (
                            <Button
                                id="supplier-catalog-batch-selection-reload"
                                variant="outline"
                                size="sm"
                                disabled={choice.busy}
                                onClick={() => void choice.refreshFrozen()}
                            >
                                重新读取选择
                            </Button>
                        ) : null
                    }
                />
            ) : null}
            {choice.showExcluded ? (
                <section
                    aria-labelledby="supplier-catalog-batch-excluded-title"
                    className="flex flex-col gap-3 rounded-lg border p-4"
                >
                    <h3
                        id="supplier-catalog-batch-excluded-title"
                        className="font-medium"
                    >
                        已排除 {choice.excludedRows.length} 个 SKU
                    </h3>
                    <div className="flex max-h-80 flex-col gap-2 overflow-y-auto">
                        {choice.excludedRows.map((row) => (
                            <div
                                key={row.source.id}
                                className="flex flex-wrap items-center justify-between gap-3 border-b pb-2 text-body-compact"
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
            {canReview && identityItemId ? (
                <CatalogIdentityReviewDialog
                    connectionId={connectionId}
                    itemId={identityItemId}
                    onClose={() => {
                        setIdentityItemId(null)
                        restoreListRowFocus(lastFocusedRowId.current)
                    }}
                />
            ) : null}
            {previewRow ? (
                <CatalogPreviewSheet
                    key={previewRow.id}
                    item={previewRow}
                    onClose={closePreview}
                    canReadCost={showCosts}
                    canSelect={canSelect}
                    canOpenProduct={canOpenProduct}
                    canConfigure={canConfigure}
                    selectionBlocker={selectionBlocker}
                    choice={choice}
                />
            ) : null}
            <ListWorkSurface
                ariaLabel={title}
                tableClassName="max-sm:[&_[data-column-id=name]]:w-table-column-default max-sm:[&_[data-column-id=name]]:min-w-table-column-default-min"
                toolbar={
                    <CatalogFilterForm
                        key={`${showCosts}:${catalogFilterKey(filters)}`}
                        value={filters}
                        options={query.data?.filters}
                        loading={query.isFetching || choice.busy}
                        resultStatus={
                            query.isFetching
                                ? "查询中…"
                                : query.isError
                                  ? "查询未完成"
                                  : null
                        }
                        canReadCost={showCosts}
                        onApply={applyFilters}
                    />
                }
                selectionBar={
                    <div className="flex flex-wrap items-center gap-2 text-body-compact">
                        <span className="num font-medium">
                            共{" "}
                            {query.data?.product_total != null
                                ? `${query.data.product_total} 个商品 · `
                                : ""}
                            {query.data?.total ?? "—"} 个 SKU
                        </span>
                        <span className="num text-muted-foreground">
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
                            <span className="num text-success-soft-foreground">
                                已选择全部筛选结果 {choice.frozen.rows.length}{" "}
                                个 SKU · 已排除 {choice.excludedRows.length} 个
                                · 最终选中 {choice.selectedIds.length} 个
                            </span>
                        ) : choice.selectedIds.length ? (
                            <span className="num">
                                已选 {choice.productCount} 个商品 ·{" "}
                                {choice.selectedIds.length} 个 SKU
                            </span>
                        ) : null}
                        {!choice.frozen &&
                        Boolean(query.data?.total) &&
                        canSelect ? (
                            <LoadingButton
                                id="supplier-catalog-batch-select-filtered"
                                variant="outline"
                                size="sm"
                                disabled={
                                    !canConfigure ||
                                    query.isFetching ||
                                    choice.busy
                                }
                                loading={choice.selecting}
                                onClick={() => void choice.freezeAll(filters)}
                            >
                                选择全部 {query.data?.total} 个 SKU
                            </LoadingButton>
                        ) : null}
                        {choice.excludedRows.length ? (
                            <Button
                                id="supplier-catalog-batch-view-excluded"
                                variant="ghost"
                                size="sm"
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
                                size="sm"
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
                        {canSelect ? (
                            <LoadingButton
                                aria-describedby={
                                    selectionBlocker
                                        ? "supplier-catalog-adoption-blocker"
                                        : undefined
                                }
                                id="supplier-catalog-batch-configure"
                                size="sm"
                                className="ml-auto"
                                variant={
                                    canConfigureBatch ? "default" : "secondary"
                                }
                                disabled={!canConfigureBatch}
                                loading={choice.configuring}
                                onClick={() => void choice.configure()}
                            >
                                配置入库（{choice.selectedIds.length}）
                            </LoadingButton>
                        ) : null}
                    </div>
                }
                table={
                    <ListDataTable
                        id="supplier-api-connections-catalog-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => row.name}
                        loading={query.isFetching}
                        defaultColumnPinning={{
                            left: ["name"],
                            right: ["actions"],
                        }}
                        pageSizeOptions={[20]}
                        rowCount={query.data?.total ?? 0}
                        pagination={{ pageIndex: page - 1, pageSize: 20 }}
                        onPaginationChange={(next) =>
                            setPage(next.pageIndex + 1)
                        }
                        enableRowSelection={
                            canSelect
                                ? (row) =>
                                      canConfigure &&
                                      !choice.busy &&
                                      !choice.preparing &&
                                      !choice.needsReload &&
                                      choice.isFrozenMember(row.original.id)
                                : false
                        }
                        rowSelection={choice.rowSelection}
                        onRowSelectionChange={(next) =>
                            choice.change(next, query.data?.items ?? [])
                        }
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.id
                            setPreviewRow(row)
                        }}
                        highlightedRowId={previewRow?.id}
                        caption="供应商品库"
                        entityName="供应商商品"
                        errorState={
                            query.isError ? (
                                <BusinessFailureState
                                    idPrefix="supplier-api-connections-catalog-table"
                                    title="供应商品库加载失败"
                                    error={query.error}
                                    action={
                                        <Button
                                            id="supplier-api-connections-catalog-table-retry"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => void query.refetch()}
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={() => applyFilters({})}
                        emptyCopy={{
                            filterTitle: "当前筛选没有商品",
                            emptyTitle: "尚未同步商品",
                            emptyDescription:
                                "完成接口配置后同步供应商商品，再选择需要经营的商品入库。",
                        }}
                    />
                }
            />
        </>
    )
}
