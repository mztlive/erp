"use client"

import * as React from "react"
import Link from "next/link"
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
} from "@/features/master-data/api"
import { masterDataCopy } from "@/features/master-data/copy"
import { resourceLabel } from "@/features/master-data/data"
import { formatEffectiveRange } from "@/features/master-data/filter"
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
      role="tablist"
      className="flex flex-wrap gap-2 border-b border-border pb-3"
    >
      {MASTER_DATA_RESOURCES.map((item) => {
        const selected = item.key === resource
        return (
          <Button
            key={item.key}
            size="sm"
            role="tab"
            aria-selected={selected}
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
          description={masterDataCopy.unknownResourceDesc(resource)}
        />
        <ResourceNav resource="" navRef={navRef} />
      </div>
    )
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
    permissionVersion: string
  } | null>(null)

  const listQuery = useMasterDataListQuery({
    resource,
    q: search,
    lifecycleStatus,
    revisionTiming,
    metricKey,
  })

  const rows = listQuery.data?.rows ?? []
  const metrics = listQuery.data?.metrics ?? []
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

  const filterSnapshotLabel = React.useMemo(() => {
    const parts = [
      `分类=${resourceLabel(resource)}`,
      `启用状态=${lifecycleStatus}`,
      `版本状态=${revisionTiming}`,
      `指标=${metricKey}`,
      search.trim() ? `搜索=${search.trim()}` : "搜索=空",
    ]
    return parts.join(" · ")
  }, [lifecycleStatus, metricKey, resource, revisionTiming, search])

  const handleExport = React.useCallback(() => {
    if (!listQuery.data || rows.length === 0) return
    if (!permissionDemo?.canExport) return
    const csv = buildMasterDataExportCsv(
      rows,
      filterSnapshotLabel,
      listQuery.data.permissionVersion
    )
    downloadCsv(csv, `基础资料-${resourceLabel(resource)}`)
    setExportMeta({
      jobId: `EXP-W14-${Date.now().toString(36)}`,
      rowCount: rows.length,
      filterSnapshotLabel,
      permissionVersion: listQuery.data.permissionVersion,
    })
  }, [
    filterSnapshotLabel,
    listQuery.data,
    permissionDemo?.canExport,
    resource,
    rows,
  ])

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
      {
        id: "period",
        header: masterDataCopy.colEffective,
        meta: { label: masterDataCopy.colEffective },
        cell: ({ row }) => (
          <span className="num text-xs">
            {formatEffectiveRange(
              row.original.effectiveFrom,
              row.original.effectiveTo
            )}
          </span>
        ),
      },
      {
        id: "blocker",
        header: masterDataCopy.colBlocker,
        meta: { label: masterDataCopy.colBlocker },
        cell: ({ row }) =>
          row.original.primaryBlocker ? (
            <span className="text-xs text-destructive">
              {row.original.primaryBlocker}
            </span>
          ) : (
            <span className="text-xs text-muted-foreground">—</span>
          ),
      },
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
                  setPreviewId(item.stableId)
                }}
              >
                {masterDataCopy.actionView}
              </Button>
              <Button
                type="button"
                size="xs"
                variant="ghost"
                disabled={!canRevise}
                title={reviseBlocker?.message}
                onClick={(e) => {
                  e.stopPropagation()
                  setReviseTarget(item)
                }}
              >
                <HistoryIcon data-icon="inline-start" aria-hidden />
                {masterDataCopy.actionUpdate}
              </Button>
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
            </div>
          )
        },
      },
    ],
    []
  )

  const isWarehouse = resource === "warehouses"
  const primaryDisabled = isWarehouse

  if (listQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title={masterDataCopy.pageTitle(resourceLabel(resource))} />
        <ResourceNav resource={resource} navRef={navRef} />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </div>
    )
  }

  if (listQuery.isError || !listQuery.data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title={masterDataCopy.pageTitle(resourceLabel(resource))} />
        <ResourceNav resource={resource} navRef={navRef} />
        <Button type="button" onClick={() => void listQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

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
            dateTime={listQuery.data.queriedAt}
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
                disabled: false,
                onClick: () => setCreateOpen(true),
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
        />
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
                  placeholder={masterDataCopy.searchPlaceholder}
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
            onRowPreview={(row) => {
              lastFocusedRowId.current = row.stableId
              setPreviewId(row.stableId)
            }}
            onRowOpen={(row) => {
              lastFocusedRowId.current = row.stableId
              setPreviewId(row.stableId)
            }}
          />
        }
      />

      <QuickPreviewSheet
        open={previewRow != null}
        onOpenChange={(open) => {
          if (!open) {
            setPreviewId(null)
            // restore focus to last row when possible
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
              <Button
                type="button"
                variant="outline"
                disabled={!previewRow.allowedActions.includes("CREATE_REVISION")}
                title={
                  previewRow.actionBlockers.find(
                    (b) => b.action === "CREATE_REVISION"
                  )?.message
                }
                onClick={() => setReviseTarget(previewRow)}
              >
                {masterDataCopy.actionUpdate}
              </Button>
              <Button
                type="button"
                variant="outline"
                disabled={!previewRow.allowedActions.includes("DISABLE")}
                title={
                  previewRow.actionBlockers.find((b) => b.action === "DISABLE")
                    ?.message
                }
                onClick={() => setDisableTarget(previewRow)}
              >
                {masterDataCopy.actionDisable}
              </Button>
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

      <MasterDataCreateDialog
        open={createOpen}
        onOpenChange={setCreateOpen}
        resource={resource}
      />
      <MasterDataReviseDialog
        open={reviseTarget != null}
        onOpenChange={(open) => {
          if (!open) setReviseTarget(null)
        }}
        resource={resource}
        target={reviseTarget}
      />
      <MasterDataDisableDialog
        open={disableTarget != null}
        onOpenChange={(open) => {
          if (!open) setDisableTarget(null)
        }}
        resource={resource}
        target={disableTarget}
      />

      {/* silence unused for primaryDisabled if tree-shaken — used as semantic flag via isWarehouse */}
      <span className="sr-only" data-primary-disabled={primaryDisabled} />
    </div>
  )
}
