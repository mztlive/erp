"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, ColumnPinningState, PaginationState } from "@tanstack/react-table"
import {
  BanIcon,
  PlusIcon,
  RefreshCwIcon,
  SearchIcon,
} from "lucide-react"

import {
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  QuickPreviewSheet,
} from "@/components/business"
import { FilterChip } from "@/components/business/filter-chip"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Separator } from "@/components/ui/separator"
import { MALLS } from "@/features/product-publications/api"
import {
  usePublicationDetailQuery,
  usePublicationListQuery,
} from "@/features/product-publications/queries"
import { SafetyPausePanel } from "@/features/product-publications/safety-pause-panel"
import type {
  ProductPublicationListQuery,
  ProductPublicationRow,
} from "@/features/product-publications/types"
import { PUBLICATION_STATUS_LABEL } from "@/features/product-publications/types"
import { formatDateTime } from "@/lib/datetime"

function parseMetric(raw: string | null): string {
  if (
    raw === "pending_confirm" ||
    raw === "failed_handoff" ||
    raw === "mall_live" ||
    raw === "paused" ||
    raw === "pending_publish"
  ) {
    return raw
  }
  return "all"
}

export function ProductPublicationsListPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const qParam = searchParams.get("q") ?? ""
  const skuId = searchParams.get("skuId") ?? undefined
  const supplierOfferingRevisionId =
    searchParams.get("supplierOfferingRevisionId") ?? undefined
  const mallId = searchParams.get("mall") ?? undefined
  const publicationStatus = searchParams.get("publicationStatus") ?? "all"
  const deliveryStatus = searchParams.get("deliveryStatus") ?? "all"
  const metric = parseMetric(searchParams.get("metric"))

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const pageFromUrl = Math.max(1, Number(searchParams.get("page") ?? "1") || 1)
  // 本地记录：pageSize 仅影响查询页大小（沿用页面默认 20，不写 URL）
  const [pageSize, setPageSize] = React.useState(20)
  // 预览 Sheet 由本地 state 管理（导航上下文，不写 URL、不随清除筛选变化）
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [columnPinning] = React.useState<ColumnPinningState>({
    left: ["sku"],
    right: ["actions"],
  })

  React.useEffect(() => {
    // URL is source of truth when filters/metrics change outside the search box；
    // 用户正在输入（焦点在搜索框）时不得用 URL 旧值覆盖草稿
    const el = searchInputRef.current
    if (el && document.activeElement === el) return
    setSearchInput(qParam)
  }, [qParam])

  // P3：搜索 300ms 防抖自动写 URL（replace），Enter 兜底，`/` 聚焦
  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchInput.trim() === qParam) return
      replaceParams({ q: searchInput.trim() || undefined })
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- replaceParams 以当前 URL 快照为准
  }, [searchInput])

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey)
        return
      const target = event.target as HTMLElement | null
      const tag = target?.tagName
      if (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        target?.isContentEditable
      ) {
        return
      }
      event.preventDefault()
      searchInputRef.current?.focus()
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  const query: ProductPublicationListQuery = {
    q: qParam || undefined,
    skuId,
    supplierOfferingRevisionId,
    mallId,
    publicationStatus:
      publicationStatus === "all" ? undefined : publicationStatus,
    deliveryStatus: deliveryStatus === "all" ? undefined : deliveryStatus,
    metric: metric === "all" ? undefined : metric,
    page: pageFromUrl,
    pageSize,
  }

  const listQuery = usePublicationListQuery(query)
  const data = listQuery.data
  const items = data?.items ?? []
  const pagination: PaginationState = {
    pageIndex: pageFromUrl - 1,
    pageSize,
  }
  const previewQuery = usePublicationDetailQuery(previewId)
  const previewRow = previewQuery.data

  const replaceParams = React.useCallback(
    (patch: Record<string, string | undefined>) => {
      const sp = new URLSearchParams(searchParams.toString())
      for (const [k, v] of Object.entries(patch)) {
        if (!v || v === "all") sp.delete(k)
        else sp.set(k, v)
      }
      sp.delete("page")
      const qs = sp.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname)
    },
    [pathname, router, searchParams]
  )

  const commitSearch = () => {
    replaceParams({ q: searchInput.trim() || undefined })
  }

  // P4：清搜索词 + 全部筛选参数 + page 回 1；保留排序/视图/导航上下文等（本页无此类参数，语义等价全清）
  const clearFilters = React.useCallback(() => {
    setSearchInput("")
    const sp = new URLSearchParams(searchParams.toString())
    for (const k of [
      "q",
      "skuId",
      "supplierOfferingRevisionId",
      "mall",
      "publicationStatus",
      "deliveryStatus",
      "metric",
      "page",
    ]) {
      sp.delete(k)
    }
    const qs = sp.toString()
    router.replace(qs ? `${pathname}?${qs}` : pathname)
  }, [pathname, router, searchParams])

  const handlePaginationChange = React.useCallback(
    (next: PaginationState) => {
      setPageSize(next.pageSize)
      const sp = new URLSearchParams(searchParams.toString())
      if (next.pageIndex <= 0) sp.delete("page")
      else sp.set("page", String(next.pageIndex + 1))
      const qs = sp.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname)
    },
    [pathname, router, searchParams]
  )

  const columns = React.useMemo<ColumnDef<ProductPublicationRow>[]>(
    () => [
      {
        id: "sku",
        header: "SKU / 商品",
        meta: { label: "SKU / 商品", width: "reference" },
        cell: ({ row }) => (
          <div className="min-w-[12rem] max-w-[16rem]">
            <div className="truncate text-sm font-medium">
              <span className="num">{row.original.skuCode}</span>
            </div>
            <div className="truncate text-sm">{row.original.productName}</div>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.specification}
              <span className="mx-1">·</span>
              <span className="num">{row.original.publicationCode}</span>
            </div>
          </div>
        ),
      },
      {
        id: "mall",
        header: "目标商城",
        meta: { label: "目标商城", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.targetMallName}</span>
        ),
      },
      {
        id: "acked",
        header: "商城生效版",
        meta: { label: "商城生效版", width: "status" },
        cell: ({ row }) =>
          row.original.currentAckedRevisionNo != null ? (
            <span className="num text-sm">
              r{row.original.currentAckedRevisionNo}
            </span>
          ) : (
            <span className="text-xs text-muted-foreground">尚未生效</span>
          ),
      },
      {
        id: "latest",
        header: "最新发布版",
        meta: { label: "最新发布版", width: "status" },
        cell: ({ row }) => (
          <div className="text-sm">
            {row.original.latestRevisionNo != null ? (
              <span className="num">r{row.original.latestRevisionNo}</span>
            ) : (
              "—"
            )}
            {row.original.hasPendingConfirmation ? (
              <Badge variant="outline" className="ml-1 text-2xs">
                待确认
              </Badge>
            ) : null}
          </div>
        ),
      },
      {
        id: "offering",
        header: "固定供给",
        meta: { label: "固定供给", width: "default" },
        cell: ({ row }) => (
          <div className="min-w-0 text-sm">
            <div className="truncate">{row.original.fixedOffering.supplierName}</div>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.fixedOffering.availabilityLabel}
            </div>
          </div>
        ),
      },
      {
        id: "price",
        header: "含税销售价",
        meta: {
          label: "含税销售价",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.salesPriceGross
              ? `¥${row.original.salesPriceGross}`
              : "—"}
          </span>
        ),
      },
      {
        id: "pubStatus",
        header: "发布状态",
        meta: { label: "发布状态", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.publicationStatusLabel}
            tone={row.original.publicationStatusTone}
          />
        ),
      },
      {
        id: "delivery",
        header: "商城接收",
        meta: { label: "商城接收", width: "status" },
        cell: ({ row }) =>
          row.original.latestDelivery ? (
            <div>
              <BusinessStatusBadge
                context="list"
                label={row.original.latestDelivery.statusLabel}
                tone={row.original.latestDelivery.statusTone}
              />
              {row.original.latestDelivery.errorSummary ? (
                <div className="mt-0.5 max-w-[10rem] truncate text-tiny text-destructive">
                  {row.original.latestDelivery.errorSummary}
                </div>
              ) : null}
            </div>
          ) : (
            <span className="text-xs text-muted-foreground">—</span>
          ),
      },
      {
        id: "ackAt",
        header: "商城确认时间",
        meta: { label: "商城确认时间", width: "default", numeric: true },
        cell: ({ row }) => (
          <span className="num text-xs text-muted-foreground">
            {row.original.latestDelivery?.mallAckAt
              ? formatDateTime(row.original.latestDelivery.mallAckAt, "monthDay", "passthrough")
              : "—"}
          </span>
        ),
      },
      {
        id: "owner",
        header: "负责人",
        meta: { label: "负责人", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.ownerLabel}</span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            <Button
              type="button"
              variant="ghost"
              size="xs"
              onClick={() => setPreviewId(row.original.publicationId)}
            >
              预览
            </Button>
            <Button
              type="button"
              variant="outline"
              size="xs"
              render={
                <Link
                  href={`/commerce/publications/${encodeURIComponent(row.original.publicationId)}`}
                />
              }
            >
              打开
            </Button>
          </div>
        ),
      },
    ],
    []
  )

  const metrics = data?.metrics

  return (
    <PageScaffold>
      <PageHeader
        title="商品发布"
        breadcrumbs={[
          { id: "com", label: "商城与发布", href: "/commerce/publications" },
          { id: "pub", label: "商品发布", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt="列表"
            dateTime={data?.queriedAt}
            state={listQuery.isFetching ? "syncing" : "fresh"}
            label="发布列表"
          />
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => void listQuery.refetch()}
            >
              <RefreshCwIcon />
              刷新
            </Button>
            <Button
              type="button"
              size="sm"
              disabled
              title={data?.creationBlocker.message}
            >
              <BanIcon />
              新建发布
            </Button>
          </div>
        }
      />

      {data?.creationBlocker ? (
        <Alert variant="warning">
          <PlusIcon />
          <AlertTitle>新建已阻断</AlertTitle>
          <AlertDescription>
            {data.creationBlocker.message}
          </AlertDescription>
        </Alert>
      ) : null}

      {/* 指标与「发布状态/发送状态」双向互斥：指标点击清除状态维度、状态变更清除指标。
          这是有意设计（避免指标×状态矛盾空结果），与通用「指标点击不清其它筛选」不同；保留并注明。 */}
      <MetricStrip>
        <MetricFilterItem
          label="待发布"
          value={metrics?.pendingPublish ?? "—"}
          active={metric === "pending_publish"}
          onClick={() =>
            replaceParams({
              metric:
                metric === "pending_publish" ? undefined : "pending_publish",
              deliveryStatus: undefined,
              publicationStatus: undefined,
            })
          }
        />
        <MetricFilterItem
          label="待商城确认"
          value={metrics?.pendingConfirm ?? "—"}
          active={metric === "pending_confirm"}
          onClick={() =>
            replaceParams({
              metric:
                metric === "pending_confirm" ? undefined : "pending_confirm",
              deliveryStatus: undefined,
              publicationStatus: undefined,
            })
          }
        />
        <MetricFilterItem
          label="失败/转人工"
          value={metrics?.failedOrHandoff ?? "—"}
          active={metric === "failed_handoff"}
          onClick={() =>
            replaceParams({
              metric:
                metric === "failed_handoff" ? undefined : "failed_handoff",
              deliveryStatus: undefined,
              publicationStatus: undefined,
            })
          }
        />
        <MetricFilterItem
          label="商城已生效"
          value={metrics?.mallLive ?? "—"}
          active={metric === "mall_live"}
          onClick={() =>
            replaceParams({
              metric: metric === "mall_live" ? undefined : "mall_live",
              deliveryStatus: undefined,
              publicationStatus: undefined,
            })
          }
        />
        <MetricFilterItem
          label="已暂停"
          value={metrics?.paused ?? "—"}
          active={metric === "paused"}
          onClick={() =>
            replaceParams({
              metric: metric === "paused" ? undefined : "paused",
              deliveryStatus: undefined,
              publicationStatus: undefined,
            })
          }
        />
      </MetricStrip>

      <BusinessTableFrame
        title="发布列表"
        description="管理各 SKU 在目标商城的发布版本与发送确认状态。"
        toolbar={
          <ListToolbar
            search={
              <InputGroup className="max-w-md">
                <InputGroupAddon>
                  <SearchIcon className="size-4" />
                </InputGroupAddon>
                <InputGroupInput
                  ref={searchInputRef}
                  value={searchInput}
                  placeholder="发布编号、SKU、商品名（/ 聚焦）"
                  onChange={(e) => setSearchInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitSearch()
                  }}
                />
              </InputGroup>
            }
            filters={
              <>
                <OptionCombobox
                  value={mallId ?? "all"}
                  onValueChange={(v) => {
                    const next = v ?? "all"
                    replaceParams({
                      mall: next === "all" ? undefined : next,
                    })
                  }}
                  options={[
                    { value: "all", label: "全部商城" },
                    ...MALLS.map((m) => ({
                      value: m.id,
                      label: m.name,
                    })),
                  ]}
                  className="w-36"
                  size="sm"
                  allowClear={false}
                  aria-label="目标商城"
                  placeholder="全部商城"
                />
                <OptionCombobox
                  value={publicationStatus}
                  onValueChange={(v) =>
                    replaceParams({
                      publicationStatus: v ?? "all",
                      metric: undefined,
                    })
                  }
                  options={[
                    { value: "all", label: "有效发布" },
                    ...(
                      Object.keys(PUBLICATION_STATUS_LABEL) as Array<
                        keyof typeof PUBLICATION_STATUS_LABEL
                      >
                    ).map((k) => ({
                      value: k,
                      label: PUBLICATION_STATUS_LABEL[k],
                    })),
                  ]}
                  className="w-36"
                  size="sm"
                  allowClear={false}
                  aria-label="发布状态"
                  placeholder="发布状态"
                />
                <OptionCombobox
                  value={deliveryStatus}
                  onValueChange={(v) =>
                    replaceParams({
                      deliveryStatus: v ?? "all",
                      metric: undefined,
                    })
                  }
                  options={[
                    { value: "all", label: "发送状态" },
                    { value: "pending_confirm", label: "待商城确认" },
                    { value: "failed", label: "失败" },
                    { value: "handoff", label: "转人工" },
                    { value: "acked", label: "已确认" },
                  ]}
                  className="w-40"
                  size="sm"
                  allowClear={false}
                  aria-label="发送状态"
                  placeholder="发送状态"
                />
              </>
            }
            secondary={
              (skuId && data?.resolvedFilters.skuCode) ||
              (supplierOfferingRevisionId &&
                data?.resolvedFilters.supplierName) ||
              data?.filterSummary ? (
                <>
                  {skuId && data?.resolvedFilters.skuCode ? (
                    <FilterChip
                      label={`已按 SKU：${data.resolvedFilters.skuCode}`}
                      clearLabel={`移除按 ${data.resolvedFilters.skuCode} 筛选`}
                      onClear={() =>
                        replaceParams({
                          skuId: undefined,
                          supplierOfferingRevisionId: undefined,
                        })
                      }
                    />
                  ) : null}
                  {supplierOfferingRevisionId &&
                  data?.resolvedFilters.supplierName ? (
                    <FilterChip
                      label={`已按固定供给：${data.resolvedFilters.supplierName}`}
                      clearLabel={`移除按 ${data.resolvedFilters.supplierName} 筛选`}
                      onClear={() =>
                        replaceParams({
                          skuId: undefined,
                          supplierOfferingRevisionId: undefined,
                        })
                      }
                    />
                  ) : null}
                  {data?.filterSummary ? (
                    <span className="text-xs text-muted-foreground">
                      {data.filterSummary}
                    </span>
                  ) : null}
                </>
              ) : undefined
            }
            actions={
              <>
                {(qParam ||
                  mallId ||
                  skuId ||
                  supplierOfferingRevisionId ||
                  publicationStatus !== "all" ||
                  deliveryStatus !== "all" ||
                  metric !== "all") && (
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={clearFilters}
                  >
                    清除筛选
                  </Button>
                )}
              </>
            }
          />
        }
        table={
          listQuery.isPending ? (
            <div className="h-64 animate-pulse rounded-lg bg-muted" aria-busy />
          ) : listQuery.isError ? (
            <BusinessFailureState
              title="加载失败"
              error={listQuery.error}
              className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
              action={
                <Button
                  type="button"
                  variant="secondary"
                  className="rounded-lg shadow-none"
                  onClick={() => void listQuery.refetch()}
                >
                  重试
                </Button>
              }
            />
          ) : items.length === 0 ? (
            <BusinessEmptyState
              kind={
                data?.emptyReason === "FILTER_NO_RESULT" ? "filter" : "no-data"
              }
              title={
                data?.emptyReason === "FILTER_NO_RESULT"
                  ? "无符合条件的发布"
                  : "尚无商品发布"
              }
              description={
                data?.emptyReason === "FILTER_NO_RESULT"
                  ? "可清除筛选或调整条件后重试；已失效发布请在「发布状态」选择「已失效」查看。"
                  : data?.creationBlocker.message
              }
              className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
              action={
                data?.emptyReason === "FILTER_NO_RESULT" ? (
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="rounded-lg shadow-none"
                    onClick={clearFilters}
                  >
                    清除筛选
                  </Button>
                ) : undefined
              }
            />
          ) : (
            <DataTable
              data={items}
              columns={columns}
              getRowId={(row) => row.publicationId}
              density="compact"
              layout="flush"
              enableColumnPinning
              columnPinning={columnPinning}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              rowCount={data?.total ?? 0}
              manualPagination
              loading={listQuery.isFetching}
              onRowPreview={(row) => setPreviewId(row.publicationId)}
              onRowOpen={(row) => {
                router.push(
                  `/commerce/publications/${encodeURIComponent(row.publicationId)}`
                )
              }}
              showPagination
              pageSizeOptions={[10, 20, 50]}
            />
          )
        }
      />

      <QuickPreviewSheet
        open={previewId != null}
        onOpenChange={(open) => {
          if (!open) setPreviewId(null)
        }}
        title={previewRow?.selectedRevision.name ?? "发布预览"}
        description={
          previewRow
            ? `${previewRow.identity.skuCode} · ${previewRow.identity.targetMallName}`
            : undefined
        }
      >
        {previewQuery.isPending ? (
          <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
        ) : previewRow ? (
          <div className="space-y-3 text-sm">
            <dl className="grid gap-2 sm:grid-cols-2">
              <div>
                <dt className="text-xs text-muted-foreground">发布编号</dt>
                <dd className="num">{previewRow.identity.publicationCode}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">发布状态</dt>
                <dd>{previewRow.statusLabel}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">商城生效版</dt>
                <dd className="num">
                  {previewRow.currentAckedRevisionNo != null
                    ? `r${previewRow.currentAckedRevisionNo}`
                    : "尚未生效"}
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">最新发布版</dt>
                <dd className="num">
                  {previewRow.latestRevisionNo != null
                    ? `r${previewRow.latestRevisionNo}`
                    : "—"}
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">固定供给</dt>
                <dd>
                  {previewRow.selectedRevision.fixedOffering.supplierName}
                  <div className="text-xs text-muted-foreground">
                    {previewRow.selectedRevision.fixedOffering.availabilityLabel}
                  </div>
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">商城接收</dt>
                <dd>
                  {(() => {
                    const latestDelivery = previewRow.deliveries.find(
                      (d) => d.revisionId === previewRow.latestRevisionId
                    )
                    return latestDelivery?.statusLabel ?? "—"
                  })()}
                </dd>
              </div>
            </dl>
            {previewRow.safetyPause ? (
              <>
                <Separator />
                <SafetyPausePanel
                  pause={previewRow.safetyPause}
                  compact
                  sourceObjectLabel={`${previewRow.selectedRevision.fixedOffering.supplierName} · ${previewRow.identity.skuCode}`}
                  affectedPublicationLabels={{
                    [previewRow.identity.publicationId]:
                      previewRow.identity.publicationCode,
                  }}
                />
              </>
            ) : null}
            <Button
              type="button"
              className="w-full"
              render={
                <Link
                  href={`/commerce/publications/${encodeURIComponent(previewRow.identity.publicationId)}`}
                />
              }
            >
              查看详情
            </Button>
          </div>
        ) : null}
      </QuickPreviewSheet>
    </PageScaffold>
  )
}
