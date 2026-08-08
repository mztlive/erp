"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import Link from "next/link"
import { ArrowLeftIcon, ArrowRightIcon, SearchIcon, UploadIcon, PlusIcon } from "lucide-react"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MetricItem,
  MetricStrip,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  surfaceInsetClassName,
  surfacePanelClassName,
} from "@/components/business"
import { FilterChip } from "@/components/business/filter-chip"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { cn } from "@/lib/utils"
import type {
  SupplierCatalogItemView,
  SupplierCatalogQueueView,
  SupplierCatalogSourceType,
} from "@/features/supplier-catalog/types"

type SupplyRelationshipListViewProps = {
  items: SupplierCatalogItemView[]
  skuId?: string
  skuContext?: SupplierCatalogQueueView["skuContext"]
  returnTo?: string
  returnHref: string
  updatedAt?: string
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
  sort?: string
  onSortChange?: (sort: string | undefined) => void
  onOpenItem?: (item: SupplierCatalogItemView) => void
  searchInput: string
  onSearchInputChange: (value: string) => void
  onSearch: () => void
  sourceType: SupplierCatalogSourceType | "all"
  onSourceTypeChange: (value: SupplierCatalogSourceType | "all") => void
  /** D8：清除搜索/来源/SKU 锁定等全部筛选（页面级 URL 写入，含来源锁定 chip）。 */
  onClearFilters: () => void
  /** D8：仅移除 SKU 来源锁定（深链参数显性化 chip 的 ×）。 */
  onClearSku: () => void
  onOpenExcelImport: () => void
  /** 从 W14 固定 SKU 进入时仍用对话框一次登记供给；列表自由录入走全页同构表单。 */
  onOpenManualEntry?: () => void
  onPromote: (item: SupplierCatalogItemView) => void
}

type RelationshipStatus = {
  label: string
  tone: "success" | "warning" | "destructive" | "info" | "neutral"
  kind: "active" | "pending" | "unavailable"
}

function offeringStatusLabel(status: string) {
  if (status === "ACTIVE") return "正常供货"
  if (status === "PAUSED") return "已暂停"
  if (status === "STOPPED") return "已停供"
  return "待确认"
}

function relationshipStatus(
  item: SupplierCatalogItemView,
  skuId?: string
): RelationshipStatus {
  const mappedToCurrentSku =
    item.mapping?.mappingStatus === "ACTIVE" &&
    (!skuId || item.mapping.skuId === skuId)

  if (!mappedToCurrentSku) {
    return { label: "待确认关联", tone: "info", kind: "pending" }
  }

  const offering = item.offering?.currentRevision
  if (!offering) {
    return { label: "待设置供货条件", tone: "warning", kind: "pending" }
  }

  if (
    offering.status === "STOPPED" ||
    offering.availabilityStatus === "STOPPED"
  ) {
    return { label: "已停供", tone: "destructive", kind: "unavailable" }
  }
  if (offering.status === "PAUSED") {
    return { label: "已暂停", tone: "warning", kind: "unavailable" }
  }
  if (offering.availabilityStatus === "STALE") {
    return { label: "供货信息待更新", tone: "warning", kind: "unavailable" }
  }
  if (offering.availabilityStatus === "UNAVAILABLE") {
    return { label: "暂不可供", tone: "warning", kind: "unavailable" }
  }
  if (offering.status === "PENDING_CONFIRM") {
    return { label: "供货条件待确认", tone: "info", kind: "pending" }
  }

  return { label: "正常供货", tone: "success", kind: "active" }
}

function inferSkuContext(items: SupplierCatalogItemView[], skuId?: string) {
  if (!skuId) return undefined
  for (const item of items) {
    if (item.mapping?.skuId === skuId && item.mapping.skuCode) {
      return {
        productName: item.mapping.skuName ?? "商品规格",
        skuCode: item.mapping.skuCode,
        specification: item.mapping.specification,
        baseUnit: item.mapping.baseUnit,
      }
    }
    const candidate = item.skuCandidates.find((entry) => entry.skuId === skuId)
    if (candidate) {
      return {
        productName: candidate.skuName,
        skuCode: candidate.skuCode,
        specification: candidate.specification,
        baseUnit: candidate.baseUnit,
      }
    }
  }
  return undefined
}

function availabilityLabel(value: string) {
  if (value === "AVAILABLE") return "可供"
  if (value === "UNAVAILABLE") return "暂不可供"
  if (value === "STOPPED") return "已停供"
  if (value === "STALE") return "信息待更新"
  return value || "—"
}

function formatDateOnly(iso: string | undefined) {
  if (!iso) return "—"
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return "—"
  return date.toLocaleDateString("zh-CN", { hour12: false })
}

function SupplyRelationshipListView({
  items,
  skuId,
  skuContext,
  returnTo,
  returnHref,
  updatedAt,
  emptyReason,
  sort,
  onSortChange,
  onOpenItem,
  searchInput,
  onSearchInputChange,
  onSearch,
  sourceType,
  onSourceTypeChange,
  onClearFilters,
  onClearSku,
  onOpenExcelImport,
  onOpenManualEntry,
  onPromote,
}: SupplyRelationshipListViewProps) {
  const currentSku = skuContext ?? inferSkuContext(items, skuId)
  const statusSummary = items.map((item) => relationshipStatus(item, skuId))
  const supplierCount = new Set(
    items.map((item) => item.supplierProduct.supplier.id)
  ).size
  const activeCount = statusSummary.filter((status) => status.kind === "active").length
  const unavailableCount = statusSummary.filter(
    (status) => status.kind === "unavailable"
  ).length
  const inPoolCount = new Set(
    items.flatMap((item) =>
      item.sellableSku?.status === "ACTIVE" ? [item.sellableSku.skuId] : []
    )
  ).size

  const detailHrefFor = React.useCallback(
    (item: SupplierCatalogItemView) =>
      `/procurement/supplier-catalog/${item.supplierProduct.id}?section=overview&returnTo=${encodeURIComponent(returnHref)}`,
    [returnHref]
  )

  const [sortBy, sortDir] = React.useMemo(() => {
    const [id, dir] = (sort ?? "").split(":")
    if (!id || (dir !== "asc" && dir !== "desc")) {
      return [undefined, undefined] as const
    }
    return [id, dir] as const
  }, [sort])

  const sorting = React.useMemo(
    () => (sortBy && sortDir ? [{ id: sortBy, desc: sortDir === "desc" }] : []),
    [sortBy, sortDir]
  )

  const columns = React.useMemo<ColumnDef<SupplierCatalogItemView, unknown>[]>(
    () => [
      {
        id: "supplier",
        accessorFn: (row) => row.supplierProduct.supplier.name,
        header: "供应商",
        meta: { label: "供应商", width: "reference" },
        enableSorting: true,
        cell: ({ row }) => (
          <div className="min-w-0">
            <div className="font-medium text-foreground">
              {row.original.supplierProduct.supplier.name}
            </div>
            <Badge variant="outline" className="mt-1">
              {row.original.supplierProduct.source.label}
            </Badge>
          </div>
        ),
      },
      {
        id: "supplierProduct",
        accessorFn: (row) =>
          (row.supplierProduct.incomingRevision ??
            row.supplierProduct.currentRevision).name,
        header: "商品",
        meta: { label: "商品", width: "reference" },
        enableSorting: true,
        cell: ({ row }) => {
          const revision =
            row.original.supplierProduct.incomingRevision ??
            row.original.supplierProduct.currentRevision
          const hasMainImage = (revision.media ?? []).some(
            (entry) =>
              entry.usage === "SKU_MAIN" && Boolean(entry.sourceUrl),
          )
          return (
            <div className="min-w-0">
              <div className="truncate text-sm font-medium text-foreground">
                {revision.name}
              </div>
              {hasMainImage ? (
                <div className="mt-0.5 text-xs text-muted-foreground">
                  有主图
                </div>
              ) : null}
              <div className="mt-0.5 truncate text-xs text-muted-foreground">
                {[revision.brand, revision.category]
                  .filter(Boolean)
                  .join(" · ") || "—"}
                {revision.specification
                  ? ` · ${revision.specification}`
                  : ""}
              </div>
            </div>
          )
        },
      },
      {
        id: "skuCode",
        accessorFn: (row) => row.supplierProduct.supplierSkuCode,
        header: "SKU 编码",
        meta: { label: "SKU 编码", width: "status" },
        cell: ({ row }) => (
          <div className="min-w-0">
            <div className="truncate text-sm">
              {row.original.supplierProduct.supplierSkuCode ?? "—"}
            </div>
            <div className="mt-0.5 truncate text-xs text-muted-foreground">
              {row.original.supplierProduct.supplierSpuCode
                ? `SPU ${row.original.supplierProduct.supplierSpuCode}`
                : ""}
            </div>
          </div>
        ),
      },
      {
        id: "source",
        accessorFn: (row) =>
          row.supplierProduct.source.fileName ??
          row.supplierProduct.source.connection?.code ??
          row.supplierProduct.source.recordedBy ??
          "",
        header: "来源",
        meta: { label: "来源", width: "status" },
        cell: ({ row }) => (
          <div className="min-w-0">
            <div className="truncate text-xs text-muted-foreground">
              {row.original.supplierProduct.source.fileName ??
                row.original.supplierProduct.source.connection?.code ??
                row.original.supplierProduct.source.recordedBy ??
                "—"}
            </div>
          </div>
        ),
      },
      {
        id: "price",
        accessorFn: (row) =>
          row.offering?.currentRevision?.supplyPriceGross ??
          row.supplierProduct.currentRevision.bulkFloorPriceGross ??
          row.supplierProduct.currentRevision.dropshipFloorPriceGross ??
          "",
        header: "价格",
        meta: { align: "end", numeric: true, width: "amount" },
        enableSorting: true,
        cell: ({ row }) => {
          const revision =
            row.original.supplierProduct.incomingRevision ??
            row.original.supplierProduct.currentRevision
          const offering = row.original.offering?.currentRevision
          const quote =
            revision.dropshipFloorPriceGross || revision.bulkFloorPriceGross
          return (
            <div className="text-right">
              {quote ? (
                <div className="text-xs text-muted-foreground">
                  报价 ¥{quote}
                </div>
              ) : null}
              {offering ? (
                <>
                  <div className="num font-medium">
                    ¥{offering.supplyPriceGross}
                  </div>
                  <div className="text-xs text-muted-foreground">
                    含税
                    {offering.inputTaxRate
                      ? ` · ${offering.inputTaxRate}`
                      : ""}
                    {offering.minimumOrderQuantity
                      ? ` · ${offering.minimumOrderQuantity}起订`
                      : ""}
                  </div>
                </>
              ) : quote ? (
                <div className="text-xs text-muted-foreground">待确认成本</div>
              ) : (
                <div className="text-muted-foreground">—</div>
              )}
            </div>
          )
        },
      },
      {
        id: "availability",
        accessorFn: (row) =>
          row.offering?.currentRevision?.availabilityStatus ??
          row.supplierProduct.currentRevision.availabilityStatus ??
          "",
        header: "可供",
        meta: { label: "可供", width: "status" },
        cell: ({ row }) => {
          const revision =
            row.original.supplierProduct.incomingRevision ??
            row.original.supplierProduct.currentRevision
          const offering = row.original.offering?.currentRevision
          const status = offering?.availabilityStatus ?? revision.availabilityStatus
          const quantity = offering?.availableQuantity ?? revision.availableQuantity
          return (
            <div>
              <div>{availabilityLabel(status)}</div>
              {quantity && quantity !== "0" ? (
                <div className="num text-xs text-muted-foreground">
                  可售 {quantity}
                </div>
              ) : null}
            </div>
          )
        },
      },
      {
        id: "mapping",
        accessorFn: (row) =>
          row.mapping?.mappingStatus === "ACTIVE"
            ? row.mapping.skuCode ?? ""
            : "",
        header: "关联公司 SKU",
        meta: { label: "关联公司 SKU", width: "reference" },
        cell: ({ row }) => {
          const mapping = row.original.mapping
          const sellableSku = row.original.sellableSku
          const mapped = mapping?.mappingStatus === "ACTIVE"
          return (
            <div>
              <div>
                {mapped ? (
                  <div className="font-medium text-foreground">
                    {mapping?.skuCode ?? "已映射"}
                  </div>
                ) : (
                  <span className="text-muted-foreground">待确认关联</span>
                )}
              </div>
              {sellableSku ? (
                <div className="text-xs text-muted-foreground">
                  商品池价 ¥{sellableSku.salesVisiblePriceGross}
                </div>
              ) : null}
            </div>
          )
        },
      },
      {
        id: "status",
        accessorFn: (row) => relationshipStatus(row, skuId).label,
        header: "状态",
        meta: { label: "状态", width: "status" },
        enableSorting: true,
        cell: ({ row }) => {
          const status = relationshipStatus(row.original, skuId)
          return (
            <BusinessStatusBadge
              context="list"
              label={status.label}
              tone={status.tone}
            />
          )
        },
      },
      {
        id: "updatedAt",
        accessorFn: (row) =>
          row.supplierProduct.incomingRevision?.sourceUpdatedAt ??
          row.supplierProduct.currentRevision.sourceUpdatedAt,
        header: "更新时间",
        meta: { label: "更新时间", width: "status" },
        cell: ({ row }) => {
          const updatedAt =
            row.original.supplierProduct.incomingRevision?.sourceUpdatedAt ??
            row.original.supplierProduct.currentRevision.sourceUpdatedAt
          return (
            <div className="text-xs text-muted-foreground">
              {formatDateOnly(updatedAt)}
            </div>
          )
        },
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", align: "end", width: "status" },
        enableSorting: false,
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            {!row.original.sellableSku && row.original.changeType !== "ERROR" ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => onPromote(row.original)}
              >
                入池
              </Button>
            ) : null}
            <Button
              variant="ghost"
              size="sm"
              render={<Link href={detailHrefFor(row.original)} />}
            >
              详情
              <ArrowRightIcon className="size-3.5" aria-hidden="true" />
            </Button>
          </div>
        ),
      },
    ],
    [detailHrefFor, onPromote, skuId]
  )

  const queueHref = `/procurement/supplier-catalog?mode=queue${skuId ? `&skuId=${encodeURIComponent(skuId)}` : ""}${returnTo ? `&returnTo=${encodeURIComponent(returnTo)}` : ""}`
  const pageTitle = currentSku
    ? `${currentSku.productName}的供给关系`
    : "供应商商品库"
  const pageDescription = currentSku
    ? `${currentSku.skuCode} · ${currentSku.specification ?? "默认规格"}${currentSku.baseUnit ? ` · 基本单位：${currentSku.baseUnit}` : ""}`
    : skuId
      ? "当前商品规格筛选：商品规格待补充"
      : "统一管理 Excel、API 和手工录入的供应商商品，确认后加入公司商品池"

  return (
    <PageScaffold density="compact">
      <PageHeader
        title={pageTitle}
        description={pageDescription}
        breadcrumbs={[
          { id: "procurement", label: "采购", href: "/procurement/orders" },
          { id: "catalog", label: "供应商商品库", current: true },
        ]}
        metadata={
          <DataFreshness
            state="fresh"
            label="供给信息更新时间"
            updatedAt={updatedAt ? new Date(updatedAt).toLocaleString("zh-CN", { hour12: false }) : "—"}
            dateTime={updatedAt}
          />
        }
        actions={
          <div className="flex flex-wrap gap-2">
            <Button type="button" variant="outline" size="sm" onClick={onOpenExcelImport}>
              <UploadIcon className="size-3.5" />
              按 Excel 模板录入
            </Button>
            {skuId && onOpenManualEntry ? (
              <Button type="button" size="sm" onClick={onOpenManualEntry}>
                <PlusIcon className="size-3.5" />
                添加供应商并登记成本
              </Button>
            ) : (
              <Button
                type="button"
                size="sm"
                render={
                  <Link
                    href={`/procurement/supplier-catalog/new?returnTo=${encodeURIComponent(returnHref)}`}
                  />
                }
              >
                <PlusIcon className="size-3.5" />
                手工录入
              </Button>
            )}
            <Button variant="ghost" size="sm" render={<Link href={queueHref} />}>
              处理来源变化
            </Button>
            {returnTo ? (
              <Button variant="secondary" size="sm" className="rounded-lg shadow-none" render={<Link href={returnTo} />}>
                返回商品与 SKU
              </Button>
            ) : null}
          </div>
        }
      />

      {skuId ? (
        <Card size="sm" className={surfacePanelClassName}>
          <CardContent className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] md:items-center">
            <div className="min-w-0">
              <div className="text-xs font-medium text-muted-foreground">
                当前 ERP 商品 SKU
              </div>
              <div className="mt-1 font-medium text-foreground">
                {currentSku?.productName ?? "商品规格"}
              </div>
              <div className="mt-0.5 text-sm text-muted-foreground">
                {currentSku?.skuCode ?? skuId} · {currentSku?.specification ?? "规格待补充"}
              </div>
            </div>
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground md:flex-col md:gap-1">
              <ArrowLeftIcon className="size-4 -rotate-90 md:rotate-0" aria-hidden="true" />
              <span>供给到当前 SKU</span>
            </div>
            <div className={cn(surfaceInsetClassName, "min-w-0 px-3 py-2.5")}>
              <div className="text-xs font-medium text-muted-foreground">
                供应商商品
              </div>
              <div className="mt-1 font-medium text-foreground">
                {items.length} 条供给关系 · {supplierCount} 家供应商
              </div>
              <div className="mt-0.5 text-xs text-muted-foreground">
                下方每一行，都是一个供应商商品与当前 SKU 的关系
              </div>
            </div>
          </CardContent>
        </Card>
      ) : (
        <Badge variant="outline">全部 ERP SKU</Badge>
      )}

      <MetricStrip columns={4} density="compact" aria-label="供给关系概览">
        <MetricItem label="供给关系" value={items.length} density="compact" />
        <MetricItem label="已入公司商品池" value={inPoolCount} density="compact" />
        <MetricItem label="正常供货" value={activeCount} density="compact" />
        <MetricItem label="暂停或停供" value={unavailableCount} density="compact" />
      </MetricStrip>

      <BusinessTableFrame
        title={skuId ? "当前 SKU 的供应商供给" : "供应商商品"}
        description="来源报价先保留为供应商事实；采购确认后，成本写入供给版本，销售可见价写入公司商品池版本。"
        toolbar={
          <ListToolbar
            search={
              // P3：list 模式搜索为「防抖即时 + Enter 兜底」，页面级写入 q；
              // / 聚焦快捷键由页面监听 data-slot="catalog-list-search"。
              <InputGroup className="w-full max-w-sm">
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  data-slot="catalog-list-search"
                  value={searchInput}
                  onChange={(event) => onSearchInputChange(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") onSearch()
                  }}
                  placeholder="搜索供应商、商品名称或商品编码"
                  aria-label="搜索供应商供给"
                />
              </InputGroup>
            }
            filters={
              <OptionCombobox
                value={sourceType}
                onValueChange={(value) =>
                  onSourceTypeChange(
                    (value ?? "all") as SupplierCatalogSourceType | "all"
                  )
                }
                options={[
                  { value: "all", label: "全部来源" },
                  { value: "EXCEL", label: "Excel 导入" },
                  { value: "API", label: "API 同步" },
                  { value: "MANUAL", label: "手工录入" },
                ]}
                allowClear={false}
                className="w-40"
                aria-label="来源类型"
              />
            }
            secondary={
              skuId ? (
                // D8：SKU 来源锁定显性化为可移除 chip（深链参数）
                <FilterChip
                  label={`${currentSku?.productName ?? "商品规格"}（${currentSku?.skuCode ?? skuId}）`}
                  onClear={onClearSku}
                  clearLabel="清除当前 SKU 锁定"
                />
              ) : undefined
            }
            actions={
              <div className="flex items-center gap-2">
                <span
                  className="text-xs text-muted-foreground"
                  aria-live="polite"
                >
                  共 {items.length.toLocaleString("zh-CN")} 条
                </span>
                {Boolean(searchInput.trim()) ||
                sourceType !== "all" ||
                Boolean(skuId) ? (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    onClick={onClearFilters}
                  >
                    清除筛选
                  </Button>
                ) : null}
              </div>
            }
          />
        }
        table={
          <DataTable
            data={items}
            columns={columns}
            getRowId={(row) => row.supplierProduct.id}
            rowCount={items.length}
            rowLabel={(row) =>
              `${row.supplierProduct.supplier.name} ${row.supplierProduct.currentRevision.name}`
            }
            caption="商品供给关系列表"
            density="compact"
            layout="flush"
            showPagination={false}
            enableColumnPinning
            sorting={sorting}
            onSortingChange={(next) => {
              const nextSort = next[0]
              onSortChange?.(
                nextSort
                  ? `${nextSort.id}:${nextSort.desc ? "desc" : "asc"}`
                  : undefined
              )
            }}
            defaultColumnPinning={{ left: ["supplier", "skuCode"], right: ["actions"] }}
            onRowPreview={onOpenItem ? (row) => onOpenItem(row) : undefined}
            onRowOpen={onOpenItem ? (row) => onOpenItem(row) : undefined}
            emptyState={
              emptyReason === "FILTER_NO_RESULT" ? (
                <BusinessEmptyState
                  kind="filter"
                  title="没有符合条件的供应商商品"
                  description="可调整搜索、来源或清除筛选后重试。"
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                  action={
                    // D8：清除范围=搜索+来源+SKU 锁定（原实现只清 q，漏 sourceType）
                    <Button
                      variant="secondary"
                      type="button"
                      className="rounded-lg shadow-none"
                      onClick={onClearFilters}
                    >
                      清除筛选
                    </Button>
                  }
                />
              ) : (
                <BusinessEmptyState
                  kind="no-data"
                  title={skuId ? "当前 SKU 暂无供应商供给" : "暂无供应商商品"}
                  description="可以导入供应商 Excel、运行 API 同步或手工录入。三种来源均可录入；手工录入使用与公司商品相同的表单。"
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                  action={
                    skuId && onOpenManualEntry ? (
                      <Button
                        variant="secondary"
                        type="button"
                        className="rounded-lg shadow-none"
                        onClick={onOpenManualEntry}
                      >
                        添加供应商并登记成本
                      </Button>
                    ) : (
                      <Button
                        variant="secondary"
                        type="button"
                        className="rounded-lg shadow-none"
                        render={
                          <Link
                            href={`/procurement/supplier-catalog/new?returnTo=${encodeURIComponent(returnHref)}`}
                          />
                        }
                      >
                        手工录入商品
                      </Button>
                    )
                  }
                />
              )
            }
          />
        }
      />
    </PageScaffold>
  )
}

export { SupplyRelationshipListView, offeringStatusLabel }
