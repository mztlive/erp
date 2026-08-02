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
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  PageHeader,
  QuickPreviewSheet,
} from "@/components/business"
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
import {
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select"
import { Separator } from "@/components/ui/separator"
import { MALLS } from "@/features/product-publications/api"
import { usePublicationListQuery } from "@/features/product-publications/queries"
import { SafetyPausePanel } from "@/features/product-publications/safety-pause-panel"
import type {
  ProductPublicationListQuery,
  ProductPublicationRow,
} from "@/features/product-publications/types"
import { PUBLICATION_STATUS_LABEL } from "@/features/product-publications/types"

function formatTime(iso: string) {
  try {
    return new Date(iso).toLocaleString("zh-CN", {
      hour12: false,
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    })
  } catch {
    return iso
  }
}

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
  const mallId = searchParams.get("mall") ?? undefined
  const publicationStatus = searchParams.get("publicationStatus") ?? "all"
  const deliveryStatus = searchParams.get("deliveryStatus") ?? "all"
  const metric = parseMetric(searchParams.get("metric"))

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [columnPinning] = React.useState<ColumnPinningState>({
    left: ["sku"],
    right: ["actions"],
  })

  React.useEffect(() => {
    // URL is source of truth when filters/metrics change outside the search box
     
    setSearchInput(qParam)
  }, [qParam])

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
    mallId,
    publicationStatus:
      publicationStatus === "all" ? undefined : publicationStatus,
    deliveryStatus: deliveryStatus === "all" ? undefined : deliveryStatus,
    metric: metric === "all" ? undefined : metric,
    page: pagination.pageIndex + 1,
    pageSize: pagination.pageSize,
  }

  const listQuery = usePublicationListQuery(query)
  const data = listQuery.data
  const items = data?.items ?? []
  const previewRow = items.find((r) => r.publicationId === previewId) ?? null

  const replaceParams = React.useCallback(
    (patch: Record<string, string | undefined>) => {
      const sp = new URLSearchParams(searchParams.toString())
      for (const [k, v] of Object.entries(patch)) {
        if (!v || v === "all") sp.delete(k)
        else sp.set(k, v)
      }
      const qs = sp.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname)
      setPagination((p) => ({ ...p, pageIndex: 0 }))
    },
    [pathname, router, searchParams]
  )

  const commitSearch = () => {
    replaceParams({ q: searchInput.trim() || undefined })
  }

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
              <Badge variant="outline" className="ml-1 text-[10px]">
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
              <span className="num">
                {row.original.fixedOffering.offeringRevisionId}
              </span>
              {" · "}
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
                <div className="mt-0.5 max-w-[10rem] truncate text-[11px] text-destructive">
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
        header: "商城确认",
        meta: { label: "商城确认", width: "default", numeric: true },
        cell: ({ row }) => (
          <span className="num text-xs text-muted-foreground">
            {row.original.latestDelivery?.mallAckAt
              ? formatTime(row.original.latestDelivery.mallAckAt)
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
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-4 md:p-5">
      <PageHeader
        title="商品发布"
        breadcrumbs={[
          { id: "com", label: "商城与发布", href: "/commerce/publications" },
          { id: "pub", label: "商品发布", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt="列表"
            dateTime={data?.queriedAt ?? new Date().toISOString()}
            state={listQuery.isFetching ? "stale" : "fresh"}
            label="发布列表"
          />
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
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
            <span className="font-mono text-xs">
              {data.creationBlocker.code}
            </span>
            {" · "}
            {data.creationBlocker.message}
          </AlertDescription>
        </Alert>
      ) : null}

      <MetricStrip>
        <MetricFilterItem
          label="待发布"
          value={metrics?.pendingPublish ?? "—"}
          active={metric === "pending_publish"}
          onClick={() =>
            replaceParams({
              metric:
                metric === "pending_publish" ? undefined : "pending_publish",
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
            })
          }
        />
      </MetricStrip>

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
          <div className="flex flex-wrap items-center gap-2">
            <NativeSelect
              value={mallId ?? "all"}
              onChange={(e) =>
                replaceParams({
                  mall:
                    e.target.value === "all" ? undefined : e.target.value,
                })
              }
              className="w-36"
              aria-label="目标商城"
            >
              <NativeSelectOption value="all">全部商城</NativeSelectOption>
              {MALLS.map((m) => (
                <NativeSelectOption key={m.id} value={m.id}>
                  {m.name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <NativeSelect
              value={publicationStatus}
              onChange={(e) =>
                replaceParams({ publicationStatus: e.target.value })
              }
              className="w-36"
              aria-label="发布状态"
            >
              <NativeSelectOption value="all">发布状态</NativeSelectOption>
              {(
                Object.keys(PUBLICATION_STATUS_LABEL) as Array<
                  keyof typeof PUBLICATION_STATUS_LABEL
                >
              ).map((k) => (
                <NativeSelectOption key={k} value={k}>
                  {PUBLICATION_STATUS_LABEL[k]}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <NativeSelect
              value={deliveryStatus}
              onChange={(e) =>
                replaceParams({
                  deliveryStatus: e.target.value,
                  metric: undefined,
                })
              }
              className="w-40"
              aria-label="投递状态"
            >
              <NativeSelectOption value="all">投递状态</NativeSelectOption>
              <NativeSelectOption value="pending_confirm">
                待商城确认
              </NativeSelectOption>
              <NativeSelectOption value="failed">失败</NativeSelectOption>
              <NativeSelectOption value="handoff">转人工</NativeSelectOption>
              <NativeSelectOption value="acked">已确认</NativeSelectOption>
            </NativeSelect>
            {(qParam ||
              mallId ||
              publicationStatus !== "all" ||
              deliveryStatus !== "all" ||
              metric !== "all") && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => {
                  setSearchInput("")
                  router.replace(pathname)
                  setPagination((p) => ({ ...p, pageIndex: 0 }))
                }}
              >
                清除筛选
              </Button>
            )}
          </div>
        }
      />

      {data?.filterSummary ? (
        <p className="text-xs text-muted-foreground">{data.filterSummary}</p>
      ) : null}

      <BusinessTableFrame
        title="发布列表"
        description="SKU 与操作列固定；默认紧凑行高以在 1440×900 露出更多行。"
        table={
          listQuery.isPending ? (
            <div className="h-64 animate-pulse rounded-lg bg-muted" aria-busy />
          ) : listQuery.isError ? (
            <BusinessEmptyState
              kind="no-data"
              title="加载失败"
              description="无法读取商品发布列表。"
              action={
                <Button type="button" onClick={() => void listQuery.refetch()}>
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
                  ? "可清除筛选或调整条件后重试。"
                  : data?.creationBlocker.message
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
              onPaginationChange={setPagination}
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
        title={previewRow?.productName ?? "发布预览"}
        description={
          previewRow
            ? `${previewRow.skuCode} · ${previewRow.targetMallName}`
            : undefined
        }
      >
        {previewRow ? (
          <div className="space-y-3 text-sm">
            <dl className="grid gap-2 sm:grid-cols-2">
              <div>
                <dt className="text-xs text-muted-foreground">发布编号</dt>
                <dd className="num">{previewRow.publicationCode}</dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">发布状态</dt>
                <dd>{previewRow.publicationStatusLabel}</dd>
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
                  {previewRow.fixedOffering.supplierName}
                  <div className="num text-xs text-muted-foreground">
                    {previewRow.fixedOffering.offeringRevisionId}
                  </div>
                </dd>
              </div>
              <div>
                <dt className="text-xs text-muted-foreground">商城接收</dt>
                <dd>
                  {previewRow.latestDelivery?.statusLabel ?? "—"}
                  {previewRow.latestDelivery?.errorSummary ? (
                    <div className="text-xs text-destructive">
                      {previewRow.latestDelivery.errorSummary}
                    </div>
                  ) : null}
                </dd>
              </div>
            </dl>
            {previewRow.safetyPause ? (
              <>
                <Separator />
                <SafetyPausePanel pause={previewRow.safetyPause} compact />
              </>
            ) : null}
            <Button
              type="button"
              className="w-full"
              render={
                <Link
                  href={`/commerce/publications/${encodeURIComponent(previewRow.publicationId)}`}
                />
              }
            >
              查看详情
            </Button>
          </div>
        ) : null}
      </QuickPreviewSheet>
    </div>
  )
}
