"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import {
  BanIcon,
  DownloadIcon,
  HistoryIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  BackgroundJobProgress,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  OptionCombobox,
  PageActions,
  PageHeader,
  QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import {
  buildMasterDataExportCsv,
  downloadCsv,
} from "@/features/master-data/queries"
import {
  masterDataCopy,
  masterDataSearchPlaceholder,
  lifecycleFilterLabel,
  metricFilterLabel,
  revisionTimingFilterLabel,
} from "@/features/master-data/copy"
import { resourceLabel } from "@/features/master-data/data"
import { formatEffectiveRange } from "@/features/master-data/filter"
import { CategoryTreePage } from "@/features/master-data/category-tree-page"
import {
  MasterDataCreateDialog,
  MasterDataDisableDialog,
  MasterDataReviseDialog,
} from "@/features/master-data/master-data-action-dialog"
import { MasterDataPreviewPanel } from "@/features/master-data/master-data-preview"
import {
  useMasterDataCenterQuery,
  useMasterDataListQuery,
} from "@/features/master-data/queries"
import {
  MASTER_DATA_RESOURCES,
  type MasterDataListItem,
  type MasterDataResource,
} from "@/features/master-data/types"

const VALID = new Set(MASTER_DATA_RESOURCES.map((item) => item.key))

function isResource(value: string): value is MasterDataResource {
  return VALID.has(value as MasterDataResource)
}

function ResourceNav({
  resource,
  navRef,
}: {
  resource: string
  navRef: React.RefObject<HTMLElement | null>
}) {
  return (
    <nav
      ref={navRef}
      aria-label={masterDataCopy.resourceNavAria}
      className="flex flex-wrap gap-2 border-b border-border pb-3"
    >
      {MASTER_DATA_RESOURCES.map((item) => {
        const selected = item.key === resource
        return (
          <Button
            key={item.key}
            size="sm"
            aria-current={selected ? "page" : undefined}
            variant={selected ? "secondary" : "ghost"}
            render={<Link href={`/master-data/${item.key}`} />}
          >
            {item.label}
          </Button>
        )
      })}
    </nav>
  )
}

/** 禁用按钮的阻断原因提示：disabled 状态下浏览器不显示 title，用外层 span 承载。 */
function DisabledActionHint({
  message,
  children,
}: {
  message?: string
  children: React.ReactNode
}) {
  return message ? (
    <span title={message} className="inline-flex">
      {children}
    </span>
  ) : (
    <>{children}</>
  )
}

export function MasterDataPage({ resource }: { resource: string }) {
  const navRef = React.useRef<HTMLElement | null>(null)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const resultsHeadingRef = React.useRef<HTMLHeadingElement | null>(null)
  const lastFocusedRowId = React.useRef<string | null>(null)

  const valid = isResource(resource)

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (
        event.key === "/" &&
        !(event.target instanceof HTMLInputElement) &&
        !(event.target instanceof HTMLTextAreaElement)
      ) {
        // 弹窗 / 抽屉打开时不让 / 聚焦背景搜索框
        if (document.querySelector('[role="dialog"], [data-slot="sheet"]')) {
          return
        }
        event.preventDefault()
        searchInputRef.current?.focus()
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
  }, [])

  React.useEffect(() => {
    if (!valid) return
    // Focus results title after resource switch for a11y announcement
    const t = window.setTimeout(() => {
      resultsHeadingRef.current?.focus()
    }, 0)
    return () => window.clearTimeout(t)
  }, [resource, valid])

  if (!valid) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title={masterDataCopy.unknownResourceTitle}
          description={masterDataCopy.unknownResourceDesc()}
        />
        <ResourceNav resource="" navRef={navRef} />
      </div>
    )
  }

  /** 商品分类：树形维护，不走扁平列表。 */
  if (resource === "categories") {
    return <CategoryTreePage navRef={navRef} />
  }

  return (
    <MasterDataListWorkspace
      resource={resource}
      navRef={navRef}
      searchInputRef={searchInputRef}
      resultsHeadingRef={resultsHeadingRef}
      lastFocusedRowId={lastFocusedRowId}
    />
  )
}

function MasterDataListWorkspace({
  resource,
  navRef,
  searchInputRef,
  resultsHeadingRef,
  lastFocusedRowId,
}: {
  resource: MasterDataResource
  navRef: React.RefObject<HTMLElement | null>
  searchInputRef: React.RefObject<HTMLInputElement | null>
  resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
  lastFocusedRowId: React.MutableRefObject<string | null>
}) {
  const router = useRouter()
  /** 商品（SPU）走详情页，不用侧边 sheet。 */
  const isProductResource = resource === "products"
  /** 供应商走详情页（查看与编辑同一页面），不用侧边 sheet / 编辑弹窗。 */
  const isSupplierResource = resource === "suppliers"
  const skipPreviewSheet = isProductResource || isSupplierResource
  /** 品牌 / 分类字典不展示生效期间列。 */
  const showEffectiveColumn = resource !== "brands"
  const [search, setSearch] = React.useState("")
  const [lifecycleStatus, setLifecycleStatus] = React.useState<
    "enabled" | "disabled" | "all"
  >("all")
  const [revisionTiming, setRevisionTiming] = React.useState<
    "current" | "future" | "all"
  >("all")
  const [metricKey, setMetricKey] = React.useState("all")
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [createOpen, setCreateOpen] = React.useState(false)
  const [reviseTarget, setReviseTarget] =
    React.useState<MasterDataListItem | null>(null)
  const [disableTarget, setDisableTarget] =
    React.useState<MasterDataListItem | null>(null)
  const [exportMeta, setExportMeta] = React.useState<{
    jobId: string
    rowCount: number
    filterSnapshotLabel: string
  } | null>(null)

  // 切换资源时重置筛选条件，避免旧分类的搜索/指标残留导致新分类首屏为空且无提示。
  React.useEffect(() => {
    setSearch("")
    setLifecycleStatus("all")
    setRevisionTiming("all")
    setMetricKey("all")
    setPagination((p) => ({ ...p, pageIndex: 0 }))
    setPreviewId(null)
    setExportMeta(null)
  }, [resource])

  const listQuery = useMasterDataListQuery({
    resource,
    q: search,
    lifecycleStatus,
    revisionTiming,
    metricKey,
  })

  const rows = React.useMemo(
    () => listQuery.data?.rows ?? [],
    [listQuery.data?.rows]
  )
  const permissionDemo = listQuery.data?.permissionDemo

  const previewDetailQuery = useMasterDataCenterQuery(
    resource,
    previewId ?? ""
  )

  const previewRow = React.useMemo(
    () => rows.find((r) => r.stableId === previewId) ?? null,
    [previewId, rows]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return rows.slice(start, start + pagination.pageSize)
  }, [pagination.pageIndex, pagination.pageSize, rows])

  const resetPagination = React.useCallback(() => {
    setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
  }, [])

  /** 指标与当前搜索/启停/版本筛选同步，避免「全部 3」与表格行数矛盾。 */
  const syncedMetrics = React.useMemo(() => {
    const base = listQuery.data?.metrics ?? []
    if (rows.length === 0 || listQuery.data == null) return base
    const metricCount = (key: string): number => {
      switch (key) {
        case "enabled":
          return rows.filter((r) => r.lifecycleStatus === "ENABLED").length
        case "disabled":
          return rows.filter((r) => r.lifecycleStatus === "DISABLED").length
        case "pending":
          return rows.filter((r) => r.revisionTiming === "FUTURE").length
        case "expiring":
          return rows.filter((r) => r.metricTags.includes("expiring")).length
        default:
          return rows.length
      }
    }
    return base.map((metric) => ({ ...metric, value: metricCount(metric.key) }))
  }, [listQuery.data, rows])

  const filterSnapshotLabel = React.useMemo(() => {
    const parts = [
      `分类=${resourceLabel(resource)}`,
      `启用状态=${lifecycleFilterLabel(lifecycleStatus)}`,
      `版本状态=${revisionTimingFilterLabel(revisionTiming)}`,
      `指标=${metricFilterLabel(metricKey)}`,
      search.trim() ? `搜索=${search.trim()}` : "搜索=空",
    ]
    return parts.join(" · ")
  }, [lifecycleStatus, metricKey, resource, revisionTiming, search])

  const handleExport = React.useCallback(() => {
    if (!listQuery.data || rows.length === 0) return
    if (!permissionDemo?.canExport) return
    const csv = buildMasterDataExportCsv(rows, filterSnapshotLabel)
    downloadCsv(csv, `基础资料-${resourceLabel(resource)}`)
    const datePart = new Date().toISOString().slice(0, 10).replace(/-/g, "")
    setExportMeta({
      jobId: `导出-${datePart}-${String(Date.now() % 100000).padStart(5, "0")}`,
      rowCount: rows.length,
      filterSnapshotLabel,
    })
  }, [filterSnapshotLabel, listQuery.data, permissionDemo?.canExport, resource, rows])

  const columns = React.useMemo<ColumnDef<MasterDataListItem>[]>(
    () => [
      {
        id: "stableNo",
        accessorKey: "stableNo",
        header: masterDataCopy.colStableNo,
        meta: { label: masterDataCopy.colStableNo, width: "default" as const },
        cell: ({ row }) => (
          <span className="num text-sm">{row.original.stableNo}</span>
        ),
      },
      {
        id: "name",
        accessorKey: "name",
        header: masterDataCopy.colName,
        meta: { label: masterDataCopy.colName },
        cell: ({ row }) => (
          <div className="min-w-0">
            <div className="truncate text-sm font-medium">
              {row.original.name}
            </div>
            {row.original.keyFacts[0] ? (
              <div className="truncate text-xs text-muted-foreground">
                {row.original.keyFacts[0].label}：
                {row.original.keyFacts[0].value}
              </div>
            ) : null}
          </div>
        ),
      },
      {
        id: "revisionNo",
        header: masterDataCopy.colVersion,
        meta: { label: masterDataCopy.colVersion, width: "amount" as const },
        cell: ({ row }) => (
          <span className="num text-sm">v{row.original.revisionNo}</span>
        ),
      },
      {
        id: "lifecycle",
        header: masterDataCopy.colLifecycle,
        meta: { label: masterDataCopy.colLifecycle },
        cell: ({ row }) => (
          <div className="flex flex-col gap-1">
            <BusinessStatusBadge
              context="list"
              label={row.original.lifecycleStatusLabel}
              tone={row.original.lifecycleTone}
            />
            {row.original.scheduledLifecycleLabel ? (
              <span className="text-[11px] text-muted-foreground">
                {row.original.scheduledLifecycleLabel}
              </span>
            ) : null}
          </div>
        ),
      },
      {
        id: "revisionTiming",
        header: masterDataCopy.colVersionState,
        meta: { label: masterDataCopy.colVersionState },
        cell: ({ row }) => (
          <Badge
            variant={
              row.original.revisionTiming === "FUTURE" ? "warning" : "secondary"
            }
          >
            {row.original.revisionTimingLabel}
          </Badge>
        ),
      },
    ...(showEffectiveColumn
      ? [
          {
            id: "period",
            header: masterDataCopy.colEffective,
            meta: {
              label: masterDataCopy.colEffective,
            },
            cell: ({ row }: { row: { original: MasterDataListItem } }) => (
              <span className="num text-xs">
                {formatEffectiveRange(
                  row.original.effectiveFrom,
                  row.original.effectiveTo
                )}
              </span>
            ),
          } satisfies ColumnDef<MasterDataListItem>,
        ]
      : []),
      ...(rows.some((r) => r.primaryBlocker)
        ? [
            {
              id: "blocker",
              header: masterDataCopy.colBlocker,
              meta: { label: masterDataCopy.colBlocker },
              cell: ({ row }: { row: { original: MasterDataListItem } }) =>
                row.original.primaryBlocker ? (
                  <span className="text-xs text-destructive">
                    {row.original.primaryBlocker}
                  </span>
                ) : (
                  <span className="text-xs text-muted-foreground">—</span>
                ),
            } satisfies ColumnDef<MasterDataListItem>,
          ]
        : []),
      {
        id: "actions",
        header: masterDataCopy.colActions,
        meta: { label: masterDataCopy.colActions },
        cell: ({ row }) => {
          const item = row.original
          const canRevise = item.allowedActions.includes("CREATE_REVISION")
          const canDisable = item.allowedActions.includes("DISABLE")
          const reviseBlocker = item.actionBlockers.find(
            (b) => b.action === "CREATE_REVISION"
          )
          const disableBlocker = item.actionBlockers.find(
            (b) => b.action === "DISABLE"
          )
          return (
            <div className="flex flex-wrap gap-1">
              <Button
                type="button"
                size="xs"
                variant="ghost"
                onClick={(e) => {
                  e.stopPropagation()
                  lastFocusedRowId.current = item.stableId
                  if (isProductResource || isSupplierResource) {
                    router.push(
                      `/master-data/${resource}/${item.stableId}?section=overview`
                    )
                  } else {
                    setPreviewId(item.stableId)
                  }
                }}
              >
                {masterDataCopy.actionView}
              </Button>
              <DisabledActionHint message={reviseBlocker?.message}>
                <Button
                  type="button"
                  size="xs"
                  variant="ghost"
                  disabled={!canRevise}
                  title={reviseBlocker?.message}
                  onClick={(e) => {
                    e.stopPropagation()
                    if (isProductResource || isSupplierResource) {
                      // 详情页即编辑，与「查看」同一路由
                      router.push(
                        `/master-data/${resource}/${item.stableId}?section=overview`
                      )
                    } else {
                      setReviseTarget(item)
                    }
                  }}
                >
                  <HistoryIcon data-icon="inline-start" aria-hidden />
                  {masterDataCopy.actionUpdate}
                </Button>
              </DisabledActionHint>
              <DisabledActionHint message={disableBlocker?.message}>
                <Button
                  type="button"
                  size="xs"
                  variant="ghost"
                  disabled={!canDisable}
                  title={disableBlocker?.message}
                  onClick={(e) => {
                    e.stopPropagation()
                    setDisableTarget(item)
                  }}
                >
                  <BanIcon data-icon="inline-start" aria-hidden />
                  {masterDataCopy.actionDisable}
                </Button>
              </DisabledActionHint>
            </div>
          )
        },
      },
    ],
    [
      isProductResource,
      isSupplierResource,
      lastFocusedRowId,
      resource,
      router,
      rows,
      showEffectiveColumn,
    ]
  )

  const isWarehouse = resource === "warehouses"

  if (listQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title={masterDataCopy.pageTitle(resourceLabel(resource))} />
        <ResourceNav resource={resource} navRef={navRef} />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </div>
    )
  }

  const listLoadFailed = listQuery.isError || !listQuery.data
  const hasActiveFilters =
    search.trim() !== "" ||
    lifecycleStatus !== "all" ||
    revisionTiming !== "all" ||
    metricKey !== "all"
  const metrics = syncedMetrics
  const noDataWithCreate = !listLoadFailed && rows.length === 0

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:gap-3.5 md:p-4">


      <PageHeader
        title={masterDataCopy.pageTitle(resourceLabel(resource))}
        breadcrumbs={[
          {
            id: "md",
            label: "基础资料",
            href: "/master-data",
          },
          {
            id: "resource",
            label: resourceLabel(resource),
            current: true,
          },
        ]}
        metadata={
          <DataFreshness
            updatedAt="刚刚"
            dateTime={listQuery.data?.queriedAt ?? ""}
            state="fresh"
            label="基础资料列表"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "export",
                label: masterDataCopy.actionExport,
                icon: DownloadIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: rows.length === 0 || !permissionDemo?.canExport,
                onClick: handleExport,
              },
              {
                actionKey: "create",
                label: isWarehouse
                  ? masterDataCopy.actionCreateClosed
                  : masterDataCopy.actionCreate,
                mobileVisibility: "hide",
                icon: PlusIcon,
                // 仓库写门禁未开放：按钮真正禁用，不再进入注定失败的表单。
                disabled: isWarehouse,
                title: isWarehouse
                  ? masterDataCopy.warehouseWriteBody
                  : undefined,
                onClick: () => {
                  if (isProductResource || isSupplierResource) {
                    router.push(`/master-data/${resource}/new`)
                  } else {
                    setCreateOpen(true)
                  }
                },
              },
            ]}
          />
        }
      />


      {isWarehouse ? (
        <FormalActionResult
          status="blocked"
          title={masterDataCopy.warehouseWriteTitle}
          description={masterDataCopy.warehouseWriteBody}
          actions={
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={<Link href="/master-data/sellable-items" />}
              >
                去公司商品池
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={<Link href="/inventory?view=balance" />}
              >
                打开库存台账
              </Button>
            </div>
          }
        />
      ) : null}

      {resource === "brands" ? (
        <p className="text-sm text-muted-foreground">
          {masterDataCopy.brandListHint}
        </p>
      ) : null}

      {resource === "sellable-items" ? (
        <p className="text-sm text-muted-foreground">
          {masterDataCopy.sellableItemsHint}
        </p>
      ) : null}

      {exportMeta ? (
        <BackgroundJobProgress
          mode="all-or-nothing"
          status="succeeded"
          total={exportMeta.rowCount}
          completed={exportMeta.rowCount}
          succeeded={exportMeta.rowCount}
          label={masterDataCopy.exportDone}
          description={
            <>
              按当前筛选导出 {exportMeta.rowCount} 条。任务号{" "}
              <span className="num">{exportMeta.jobId}</span>
              。不含无权限查看的敏感信息。
            </>
          }
        />
      ) : null}

      {metrics.length > 0 ? (
        <MetricStrip
          columns={4}
          aria-label={`${resourceLabel(resource)}指标筛选`}
        >
          {metrics.map((metric) => (
            <MetricFilterItem
              key={metric.key}
              label={metric.label}
              value={metric.value}
              detail={metric.detail}
              active={metricKey === metric.key}
              onClick={() => {
                setMetricKey(metric.key)
                resetPagination()
              }}
            />
          ))}
        </MetricStrip>
      ) : null}

      <h2
        ref={resultsHeadingRef}
        tabIndex={-1}
        className="sr-only outline-none"
      >
        {resourceLabel(resource)} · {rows.length} 条结果
      </h2>

      <BusinessTableFrame
        title={`${resourceLabel(resource)}列表`}
        description={masterDataCopy.listDescription(rows.length)}
        toolbar={
          <ListToolbar
            search={
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  ref={searchInputRef}
                  value={search}
                  onChange={(e) => {
                    setSearch(e.target.value)
                    resetPagination()
                  }}
                  placeholder={masterDataSearchPlaceholder(resource)}
                  aria-label={masterDataCopy.searchAria}
                />
              </InputGroup>
            }
            filters={
              <>
                <ToggleGroup
                  value={[lifecycleStatus]}
                  onValueChange={(values) => {
                    const next =
                      (values[0] as typeof lifecycleStatus | undefined) ??
                      "all"
                    setLifecycleStatus(next)
                    resetPagination()
                  }}
                  variant="outline"
                  size="sm"
                  spacing={0}
                  aria-label={masterDataCopy.filterLifecycleAria}
                >
                  <ToggleGroupItem value="all">全部</ToggleGroupItem>
                  <ToggleGroupItem value="enabled">
                    {masterDataCopy.lifecycleEnabled}
                  </ToggleGroupItem>
                  <ToggleGroupItem value="disabled">
                    {masterDataCopy.lifecycleDisabled}
                  </ToggleGroupItem>
                </ToggleGroup>
                <OptionCombobox
                  className="w-[10.5rem]"
                  value={revisionTiming}
                  aria-label={masterDataCopy.filterVersionAria}
                  onValueChange={(v) => {
                    setRevisionTiming(
                      (v ?? "all") as typeof revisionTiming
                    )
                    resetPagination()
                  }}
                  options={[
                    { value: "all", label: masterDataCopy.versionAll },
                    {
                      value: "current",
                      label: masterDataCopy.versionCurrent,
                    },
                    {
                      value: "future",
                      label: masterDataCopy.versionFuture,
                    },
                  ]}
                  size="sm"
                  allowClear={false}
                  placeholder={masterDataCopy.versionAll}
                />
              </>
            }
            actions={
              <span className="text-xs text-muted-foreground" aria-live="polite">
                {resourceLabel(resource)} · {rows.length} 条
              </span>
            }
          />
        }
        table={
          <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.stableId}
            rowCount={rows.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="compact"
            defaultColumnPinning={{
              left: ["stableNo"],
              right: ["actions"],
            }}
            errorState={
              listLoadFailed ? (
                <BusinessFailureState
                  kind="system"
                  description={masterDataCopy.centerLoadFail}
                  onRetry={() => void listQuery.refetch()}
                />
              ) : undefined
            }
            emptyState={
              noDataWithCreate ? (
                <BusinessEmptyState
                  kind={hasActiveFilters ? "filter" : "no-data"}
                  title={
                    hasActiveFilters
                      ? "当前筛选无结果"
                      : `还没有${resourceLabel(resource)}资料`
                  }
                  description={
                    hasActiveFilters
                      ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                      : "点击「新建」创建第一份资料；历史记录会随资料保留。"
                  }
                  action={
                    !hasActiveFilters && !isWarehouse ? (
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => {
                          if (isProductResource || isSupplierResource) {
                            router.push(`/master-data/${resource}/new`)
                          } else {
                            setCreateOpen(true)
                          }
                        }}
                      >
                        {masterDataCopy.actionCreate}
                      </Button>
                    ) : undefined
                  }
                />
              ) : undefined
            }
            onRowPreview={(row) => {
              lastFocusedRowId.current = row.stableId
              if (isProductResource || isSupplierResource) {
                router.push(
                  `/master-data/${resource}/${row.stableId}?section=overview`
                )
              } else {
                setPreviewId(row.stableId)
              }
            }}
            onRowOpen={(row) => {
              lastFocusedRowId.current = row.stableId
              if (isProductResource || isSupplierResource) {
                router.push(
                  `/master-data/${resource}/${row.stableId}?section=overview`
                )
                return
              }
              setPreviewId(row.stableId)
            }}
          />
        }
      />

      {!skipPreviewSheet ? (
        <QuickPreviewSheet
          open={previewRow != null}
          onOpenChange={(open) => {
            if (!open) {
              setPreviewId(null)
              if (lastFocusedRowId.current) {
                const el = document.querySelector(
                  `[data-row-id="${lastFocusedRowId.current}"]`
                )
                if (el instanceof HTMLElement) el.focus()
              }
            }
          }}
          size="detail"
          title={previewRow?.name ?? "基础资料预览"}
          identity={
            previewRow ? (
              <span className="num">
                {previewRow.stableNo} · v{previewRow.revisionNo}
              </span>
            ) : null
          }
          summary={
            previewRow ? (
              <div className="flex flex-wrap items-center gap-2">
                <BusinessStatusBadge
                  context="preview"
                  label={previewRow.lifecycleStatusLabel}
                  tone={previewRow.lifecycleTone}
                />
                <Badge
                  variant={
                    previewRow.revisionTiming === "FUTURE"
                      ? "warning"
                      : "secondary"
                  }
                >
                  {previewRow.revisionTimingLabel}
                </Badge>
              </div>
            ) : null
          }
          footer={
            previewRow ? (
              <>
                <Button
                  type="button"
                  variant="outline"
                  onClick={() => setPreviewId(null)}
                >
                  关闭
                </Button>
                <DisabledActionHint
                  message={previewRow.actionBlockers.find(
                    (b) => b.action === "CREATE_REVISION"
                  )?.message}
                >
                  <Button
                    type="button"
                    variant="outline"
                    disabled={
                      !previewRow.allowedActions.includes("CREATE_REVISION")
                    }
                    title={
                      previewRow.actionBlockers.find(
                        (b) => b.action === "CREATE_REVISION"
                      )?.message
                    }
                    onClick={() => setReviseTarget(previewRow)}
                  >
                    {masterDataCopy.actionUpdate}
                  </Button>
                </DisabledActionHint>
                <DisabledActionHint
                  message={previewRow.actionBlockers.find(
                    (b) => b.action === "DISABLE"
                  )?.message}
                >
                  <Button
                    type="button"
                    variant="outline"
                    disabled={!previewRow.allowedActions.includes("DISABLE")}
                    title={
                      previewRow.actionBlockers.find(
                        (b) => b.action === "DISABLE"
                      )?.message
                    }
                    onClick={() => setDisableTarget(previewRow)}
                  >
                    {masterDataCopy.actionDisable}
                  </Button>
                </DisabledActionHint>
                <Button
                  type="button"
                  render={
                    <Link
                      href={`/master-data/${resource}/${previewRow.stableId}?section=overview`}
                    />
                  }
                >
                  {masterDataCopy.actionOpenDetail}
                </Button>
              </>
            ) : null
          }
        >
          {previewRow ? (
            <MasterDataPreviewPanel
              row={previewRow}
              detail={previewDetailQuery.data}
              detailLoading={previewDetailQuery.isPending}
            />
          ) : null}
        </QuickPreviewSheet>
      ) : null}

      {!isProductResource && !isSupplierResource ? (
        <MasterDataCreateDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          resource={resource}
        />
      ) : null}
      {!isProductResource && !isSupplierResource ? (
        <MasterDataReviseDialog
          open={reviseTarget != null}
          onOpenChange={(open) => {
            if (!open) setReviseTarget(null)
          }}
          resource={resource}
          target={reviseTarget}
        />
      ) : null}
      <MasterDataDisableDialog
        open={disableTarget != null}
        onOpenChange={(open) => {
          if (!open) setDisableTarget(null)
        }}
        resource={resource}
        target={disableTarget}
      />
    </div>
  )
}
