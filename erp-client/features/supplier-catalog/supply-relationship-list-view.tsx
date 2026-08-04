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
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
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
  costMasked: boolean
  searchInput: string
  onSearchInputChange: (value: string) => void
  onSearch: () => void
  sourceType: SupplierCatalogSourceType | "all"
  onSourceTypeChange: (value: SupplierCatalogSourceType | "all") => void
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

function displayAmount(value: string | null | undefined, masked: boolean) {
  if (masked) return "无查看权限"
  return value ? `¥${value}` : "—"
}

function SupplyRelationshipListView({
  items,
  skuId,
  skuContext,
  returnTo,
  returnHref,
  updatedAt,
  costMasked,
  searchInput,
  onSearchInputChange,
  onSearch,
  sourceType,
  onSourceTypeChange,
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
      item.poolEntry?.status === "ACTIVE" ? [item.poolEntry.poolEntryId] : []
    )
  ).size

  const columns = React.useMemo<ColumnDef<SupplierCatalogItemView, unknown>[]>(
    () => [
      {
        id: "source",
        accessorFn: (row) => row.supplierProduct.source.label,
        header: "来源",
        meta: { label: "来源", width: "status" },
        cell: ({ row }) => (
          <div>
            <Badge variant="outline">{row.original.supplierProduct.source.label}</Badge>
            <div className="mt-1 text-xs text-muted-foreground">
              {row.original.supplierProduct.source.fileName ??
                row.original.supplierProduct.source.connection?.code ??
                row.original.supplierProduct.source.recordedBy ??
                "—"}
            </div>
          </div>
        ),
      },
      {
        id: "supplierProduct",
        accessorFn: (row) => row.supplierProduct.supplier.name,
        header: "供应商商品",
        meta: { label: "供应商商品", width: "reference" },
        cell: ({ row }) => {
          const revision =
            row.original.supplierProduct.incomingRevision ??
            row.original.supplierProduct.currentRevision
          const mapping = row.original.mapping
          const candidate = skuId
            ? row.original.skuCandidates.find((entry) => entry.skuId === skuId)
            : undefined
          const relationTarget =
            mapping?.skuId && (!skuId || mapping.skuId === skuId)
              ? mapping.skuCode
              : candidate?.skuCode
          const relationPrefix =
            mapping?.mappingStatus === "ACTIVE" &&
            (!skuId || mapping.skuId === skuId)
              ? "关联 SKU"
              : "候选 SKU"
          return (
            <div className="min-w-0">
              <div className="font-medium text-foreground">
                {row.original.supplierProduct.supplier.name}
              </div>
              <div className="truncate text-sm text-muted-foreground">
                {revision.name}
              </div>
              <div className="mt-0.5 text-xs text-muted-foreground">
                编码 {row.original.supplierProduct.supplierSkuCode ?? row.original.supplierProduct.supplierSpuCode}
              </div>
              {relationTarget ? (
                <div className="mt-1 text-xs font-medium text-foreground">
                  {relationPrefix}：{relationTarget}
                </div>
              ) : null}
            </div>
          )
        },
      },
      {
        id: "sourceContent",
        accessorFn: (row) =>
          (row.supplierProduct.incomingRevision ??
            row.supplierProduct.currentRevision).media?.length ?? 0,
        header: "来源内容",
        meta: { label: "来源内容", width: "reference" },
        cell: ({ row }) => {
          const revision =
            row.original.supplierProduct.incomingRevision ??
            row.original.supplierProduct.currentRevision
          const media = revision.media ?? []
          const hasArchivedMain = media.some(
            (entry) =>
              entry.usage === "SKU_MAIN" &&
              entry.archiveStatus === "ARCHIVED",
          )
          return (
            <div>
              <div className={hasArchivedMain ? "text-foreground" : "text-warning"}>
                {hasArchivedMain ? "可预填 SKU 主图" : "缺 SKU 主图"}
              </div>
              <div className="text-xs text-muted-foreground">
                {media.length} 个媒体
                {revision.barcode ? ` · 条码 ${revision.barcode}` : " · 无条码"}
              </div>
            </div>
          )
        },
      },
      {
        id: "purchaseTerms",
        accessorFn: (row) =>
          row.offering?.currentRevision?.minimumOrderQuantity ?? "",
        header: "采购条件",
        meta: { label: "采购条件", width: "reference" },
        cell: ({ row }) => {
          const offering = row.original.offering?.currentRevision
          if (!offering) return <span className="text-muted-foreground">—</span>
          const unit = row.original.mapping?.baseUnit ?? currentSku?.baseUnit ?? "件"
          return (
            <div>
              <div>
                {offering.dropshipSupplyPriceGross ?? "—"}
                {" / "}
                {offering.bulkSupplyPriceGross ?? "—"}
              </div>
              <div className="text-xs text-muted-foreground">
                {offering.minimumOrderQuantity} {unit}起订
              </div>
            </div>
          )
        },
      },
      {
        id: "price",
        accessorFn: (row) =>
          row.offering?.currentRevision?.supplyPriceGross ??
          row.supplierProduct.currentRevision.bulkFloorPriceGross ??
          row.supplierProduct.currentRevision.dropshipFloorPriceGross ??
          "",
        header: "采购确认成本",
        meta: {
          align: "end",
          numeric: true,
          width: "amount",
        },
        cell: ({ row }) => {
          const confirmedCost =
            row.original.offering?.currentRevision?.supplyPriceGross
          if (confirmedCost) {
            return (
              <span className="num">
                {displayAmount(confirmedCost, costMasked)}
              </span>
            )
          }
          const quote =
            row.original.supplierProduct.currentRevision.bulkFloorPriceGross ??
            row.original.supplierProduct.currentRevision
              .dropshipFloorPriceGross
          return (
            <span className="text-xs text-muted-foreground">
              {costMasked
                ? "无查看权限"
                : quote
                  ? `报价 ¥${quote} · 待确认`
                  : "待确认"}
            </span>
          )
        },
      },
      {
        id: "coverage",
        accessorFn: (row) =>
          row.offering?.currentRevision?.supplyRegion.join("、") ?? "",
        header: "供货范围",
        meta: { label: "供货范围", width: "reference" },
        cell: ({ row }) => {
          const offering = row.original.offering?.currentRevision
          if (!offering) return <span className="text-muted-foreground">—</span>
          return (
            <div>
              <div>{offering.supplyRegion.join("、") || "区域待确认"}</div>
              <div className="num text-xs text-muted-foreground">
                {offering.validFrom} 至 {offering.validTo ?? "长期"}
              </div>
            </div>
          )
        },
      },
      {
        id: "status",
        accessorFn: (row) => relationshipStatus(row, skuId).label,
        header: "状态",
        meta: { label: "状态", width: "status" },
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
        id: "actions",
        header: "操作",
        meta: { label: "操作", align: "end", width: "status" },
        enableSorting: false,
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            {!row.original.poolEntry && row.original.changeType !== "ERROR" ? (
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => onPromote(row.original)}
              >
                加入公司商品池
              </Button>
            ) : null}
            <Button
              variant="ghost"
              size="sm"
              render={
                <Link
                  href={`/procurement/supplier-catalog/${row.original.supplierProduct.id}?section=overview&returnTo=${encodeURIComponent(returnHref)}`}
                />
              }
            >
              详情
              <ArrowRightIcon className="size-3.5" aria-hidden="true" />
            </Button>
          </div>
        ),
      },
    ],
    [costMasked, currentSku?.baseUnit, onPromote, returnHref, skuId]
  )

  const queueHref = `/procurement/supplier-catalog?mode=queue${skuId ? `&skuId=${encodeURIComponent(skuId)}` : ""}${returnTo ? `&returnTo=${encodeURIComponent(returnTo)}` : ""}`
  const pageTitle = currentSku
    ? `${currentSku.productName}的供给关系`
    : "供应商商品库"
  const pageDescription = currentSku
    ? `${currentSku.skuCode} · ${currentSku.specification ?? "默认规格"}${currentSku.baseUnit ? ` · 基本单位：${currentSku.baseUnit}` : ""}`
    : skuId
      ? `当前 SKU：${skuId}`
      : "统一管理 Excel、API 和手工录入的供应商 SPU/SKU，选择后加入公司商品池"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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
              导入 Excel
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
            <Button variant="outline" size="sm" render={<Link href={queueHref} />}>
              处理来源变化
            </Button>
            {returnTo ? (
              <Button variant="secondary" size="sm" render={<Link href={returnTo} />}>
                返回商品与 SKU
              </Button>
            ) : null}
          </div>
        }
      />

      {skuId ? (
        <Card size="sm">
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
            <div className="min-w-0 rounded-lg bg-muted/50 px-3 py-2.5">
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
        <MetricItem label="供应商 SKU" value={items.length} density="compact" />
        <MetricItem label="已入公司商品池" value={inPoolCount} density="compact" />
        <MetricItem label="正常供货" value={activeCount} density="compact" />
        <MetricItem label="暂停或停供" value={unavailableCount} density="compact" />
      </MetricStrip>

      <BusinessTableFrame
        title={skuId ? "当前 SKU 的供应商供给" : "供应商商品"}
        description="来源报价先保留为供应商事实；采购确认后，成本写入供给版本，销售可见价写入公司商品池版本。"
        toolbar={
          <ListToolbar
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
              />
            }
            search={
              <form
                className="flex items-center gap-2"
                onSubmit={(event) => {
                  event.preventDefault()
                  onSearch()
                }}
              >
                <div className="relative min-w-0 flex-1">
                  <SearchIcon
                    className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
                    aria-hidden="true"
                  />
                  <Input
                    value={searchInput}
                    onChange={(event) => onSearchInputChange(event.target.value)}
                    placeholder="搜索供应商、商品名称或商品编码"
                    className="pl-8"
                    aria-label="搜索供应商供给"
                  />
                </div>
                <Button type="submit" variant="secondary" size="sm">
                  搜索
                </Button>
              </form>
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
            defaultColumnPinning={{ left: ["supplierProduct"], right: ["actions"] }}
            emptyState={
              <BusinessEmptyState
                kind="no-data"
                title={skuId ? "当前 SKU 暂无供应商供给" : "暂无供应商商品"}
                description="可以导入供应商 Excel、运行 API 同步或手工录入。三种来源使用相同数据结构；手工录入使用与公司商品同构的全页表单。"
                action={
                  skuId && onOpenManualEntry ? (
                    <Button
                      variant="outline"
                      type="button"
                      onClick={onOpenManualEntry}
                    >
                      添加供应商并登记成本
                    </Button>
                  ) : (
                    <Button
                      variant="outline"
                      type="button"
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
            }
          />
        }
      />
    </div>
  )
}

export { SupplyRelationshipListView, offeringStatusLabel }
