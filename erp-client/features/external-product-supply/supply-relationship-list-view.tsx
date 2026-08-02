"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"
import Link from "next/link"
import { ArrowLeftIcon, ArrowRightIcon, SearchIcon } from "lucide-react"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MetricItem,
  MetricStrip,
  PageHeader,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import type {
  ExternalCatalogItemView,
  ExternalCatalogQueueView,
} from "@/features/external-product-supply/types"

type SupplyRelationshipListViewProps = {
  items: ExternalCatalogItemView[]
  skuId?: string
  skuContext?: ExternalCatalogQueueView["skuContext"]
  returnTo?: string
  returnHref: string
  updatedAt?: string
  costMasked: boolean
  searchInput: string
  onSearchInputChange: (value: string) => void
  onSearch: () => void
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
  item: ExternalCatalogItemView,
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

function inferSkuContext(items: ExternalCatalogItemView[], skuId?: string) {
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
}: SupplyRelationshipListViewProps) {
  const currentSku = skuContext ?? inferSkuContext(items, skuId)
  const statusSummary = items.map((item) => relationshipStatus(item, skuId))
  const supplierCount = new Set(
    items.map((item) => item.externalProduct.supplier.id)
  ).size
  const activeCount = statusSummary.filter((status) => status.kind === "active").length
  const pendingCount = statusSummary.filter(
    (status) => status.kind === "pending"
  ).length
  const unavailableCount = statusSummary.filter(
    (status) => status.kind === "unavailable"
  ).length

  const columns = React.useMemo<ColumnDef<ExternalCatalogItemView, unknown>[]>(
    () => [
      {
        id: "supplierProduct",
        accessorFn: (row) => row.externalProduct.supplier.name,
        header: "供应商商品",
        meta: { label: "供应商商品", width: "reference" },
        cell: ({ row }) => {
          const revision =
            row.original.externalProduct.incomingRevision ??
            row.original.externalProduct.currentRevision
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
                {row.original.externalProduct.supplier.name}
              </div>
              <div className="truncate text-sm text-muted-foreground">
                {revision.name}
              </div>
              <div className="mt-0.5 text-xs text-muted-foreground">
                编码 {row.original.externalProduct.externalSkuId ?? row.original.externalProduct.externalProductId}
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
        id: "purchaseTerms",
        accessorFn: (row) =>
          row.offering?.currentRevision?.minimumOrderQuantity ?? "",
        header: "采购条件",
        meta: { label: "采购条件", width: "reference" },
        cell: ({ row }) => {
          const offering = row.original.offering?.currentRevision
          if (!offering) return <span className="text-muted-foreground">—</span>
          const mode = offering.supplyMode === "DROPSHIP" ? "一件代发" : "集采"
          const unit = row.original.mapping?.baseUnit ?? currentSku?.baseUnit ?? "件"
          return (
            <div>
              <div>{mode}</div>
              <div className="text-xs text-muted-foreground">
                {offering.minimumOrderQuantity} {unit}起订
              </div>
            </div>
          )
        },
      },
      {
        id: "price",
        accessorFn: (row) => row.offering?.currentRevision?.supplyPriceGross ?? "",
        header: "含税供货价",
        meta: {
          align: "end",
          numeric: true,
          width: "amount",
        },
        cell: ({ row }) => (
          <span className="num">
            {displayAmount(
              row.original.offering?.currentRevision?.supplyPriceGross,
              costMasked
            )}
          </span>
        ),
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
          <Button
            variant="ghost"
            size="sm"
            render={
              <Link
                href={`/supplier-api/catalog/${row.original.externalProduct.id}?section=overview&returnTo=${encodeURIComponent(returnHref)}`}
              />
            }
          >
            查看供货详情
            <ArrowRightIcon className="size-3.5" aria-hidden="true" />
          </Button>
        ),
      },
    ],
    [costMasked, currentSku?.baseUnit, returnHref, skuId]
  )

  const queueHref = `/supplier-api/catalog?mode=queue${skuId ? `&skuId=${encodeURIComponent(skuId)}` : ""}${returnTo ? `&returnTo=${encodeURIComponent(returnTo)}` : ""}`
  const pageTitle = currentSku
    ? `${currentSku.productName}的供给关系`
    : "商品供给关系"
  const pageDescription = currentSku
    ? `${currentSku.skuCode} · ${currentSku.specification ?? "默认规格"}${currentSku.baseUnit ? ` · 基本单位：${currentSku.baseUnit}` : ""}`
    : skuId
      ? `当前 SKU：${skuId}`
      : "查看每个 ERP SKU 已关联的供应商与供货条件"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title={pageTitle}
        description={pageDescription}
        breadcrumbs={[
          { id: "master", label: "商品与 SKU", href: "/master-data/products" },
          { id: "supply", label: "供给关系", current: true },
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
            <Button variant="outline" size="sm" render={<Link href={queueHref} />}>
              处理供应商变更
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
        <MetricItem label="供给关系" value={items.length} density="compact" />
        <MetricItem label="正常供货" value={activeCount} density="compact" />
        <MetricItem label="待确认" value={pendingCount} density="compact" />
        <MetricItem label="暂停或停供" value={unavailableCount} density="compact" />
      </MetricStrip>

      <BusinessTableFrame
        title="供应商供给"
        description="先看供应商商品，再看它关联到哪个 ERP SKU；价格、起订量和有效期属于这条供给关系。"
        toolbar={
          <ListToolbar
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
            getRowId={(row) => row.externalProduct.id}
            rowCount={items.length}
            rowLabel={(row) =>
              `${row.externalProduct.supplier.name} ${row.externalProduct.currentRevision.name}`
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
                title="当前 SKU 暂无供应商供给"
                description="尚未关联供应商商品，或没有符合当前搜索条件的记录。"
                action={
                  <Button variant="outline" render={<Link href={queueHref} />}>
                    查看供应商商品
                  </Button>
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
