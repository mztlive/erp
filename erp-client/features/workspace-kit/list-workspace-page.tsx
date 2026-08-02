"use client"

import * as React from "react"
import { DownloadIcon, PlusIcon, SearchIcon } from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  PageActions,
  PageHeader,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { exportListRowsToCsv } from "@/features/workspace-kit/export-list-csv"
import { filterListRows } from "@/features/workspace-kit/filter-list-rows"
import type {
  ListColumnDef,
  ListRow,
  MetricDef,
  WorkspacePageDef,
} from "@/features/workspace-kit/types"

function buildColumns(
  columns: readonly ListColumnDef[]
): ColumnDef<ListRow>[] {
  return columns.map((column) => ({
    id: column.key,
    accessorFn: (row) => row.cells[column.key] ?? "",
    header: column.header,
    meta: {
      label: column.header,
      width: column.numeric ? "amount" : "default",
      align: column.numeric ? "end" : "start",
      numeric: column.numeric,
    },
    cell: ({ row }) => {
      if (column.status && row.original.status) {
        return (
          <BusinessStatusBadge context="list" {...row.original.status} />
        )
      }
      return (
        <span className={column.numeric ? "num text-sm" : "text-sm"}>
          {row.original.cells[column.key] ?? "—"}
        </span>
      )
    },
  }))
}

export function ListWorkspacePage({ def }: { def: WorkspacePageDef }) {
  if (def.shell.kind !== "list") {
    throw new Error(`ListWorkspacePage expects list shell for ${def.id}`)
  }
  const { payload } = def.shell
  const [search, setSearch] = React.useState("")
  const [metricKey, setMetricKey] = React.useState(
    payload.metrics[0]?.key ?? "all"
  )
  const [filterLabel, setFilterLabel] = React.useState(
    payload.filterLabels?.[0] ?? "全部"
  )
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [createOpen, setCreateOpen] = React.useState(false)
  const [actionResult, setActionResult] = React.useState<{
    title: string
    description: string
    reference: string
  } | null>(null)

  const filtered = React.useMemo(
    () =>
      filterListRows(payload.rows, {
        search,
        metricKey,
        filterLabel,
        metrics: payload.metrics,
        filterLabels: payload.filterLabels,
      }),
    [
      filterLabel,
      metricKey,
      payload.filterLabels,
      payload.metrics,
      payload.rows,
      search,
    ]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return filtered.slice(start, start + pagination.pageSize)
  }, [filtered, pagination.pageIndex, pagination.pageSize])

  const columns = React.useMemo(
    () => buildColumns(payload.columns),
    [payload.columns]
  )

  const breadcrumbs = def.breadcrumbs.map((item, index) =>
    index === def.breadcrumbs.length - 1 || !item.href
      ? { id: item.id, label: item.label, current: true as const }
      : { id: item.id, label: item.label, href: item.href, current: false as const }
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title={def.title}
        breadcrumbs={breadcrumbs}
        metadata={
          <DataFreshness
            updatedAt="刚刚"
            dateTime={new Date().toISOString()}
            state="fresh"
            label="列表数据"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "export",
                label: "导出",
                icon: DownloadIcon,
                variant: "outline",
                disabled: filtered.length === 0,
                onClick: () => {
                  exportListRowsToCsv(
                    filtered,
                    payload.columns,
                    `${def.title}-导出`
                  )
                  setActionResult({
                    title: "导出已生成",
                    description: `已下载当前筛选 ${filtered.length} 条记录的 CSV。`,
                    reference: `EXP-${def.id}-${filtered.length}`,
                  })
                },
              },
              ...(payload.primaryActionLabel
                ? [
                    {
                      actionKey: "primary",
                      label: payload.primaryActionLabel,
                      icon: PlusIcon,
                      onClick: () => setCreateOpen(true),
                    },
                  ]
                : []),
            ]}
          />
        }
      />

      {actionResult ? (
        <FormalActionResult
          status="succeeded"
          title={actionResult.title}
          description={actionResult.description}
          reference={actionResult.reference}
        />
      ) : null}

      {payload.metrics.length > 0 ? (
        <MetricStrip
          columns={Math.min(4, payload.metrics.length) as 2 | 3 | 4}
          aria-label={`${def.title}指标筛选`}
        >
          {payload.metrics.map((metric: MetricDef) => (
            <MetricFilterItem
              key={metric.key}
              label={metric.label}
              value={metric.value}
              detail={metric.detail}
              active={metricKey === metric.key}
              onClick={() => {
                setMetricKey(metric.key)
                setPagination((prev) => ({ ...prev, pageIndex: 0 }))
              }}
            />
          ))}
        </MetricStrip>
      ) : null}

      <BusinessTableFrame
        title={`${def.title}列表`}
        description={
          filterLabel === (payload.filterLabels?.[0] ?? "全部")
            ? null
            : `当前筛选：${filterLabel}`
        }
        toolbar={
          <ListToolbar
            search={
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  value={search}
                  onChange={(event) => {
                    setSearch(event.target.value)
                    setPagination((prev) => ({ ...prev, pageIndex: 0 }))
                  }}
                  placeholder={payload.searchPlaceholder}
                  aria-label={`搜索${def.title}`}
                />
              </InputGroup>
            }
            filters={
              payload.filterLabels && payload.filterLabels.length > 0 ? (
                <ToggleGroup
                  value={[filterLabel]}
                  onValueChange={(values) => {
                    const next = values[0]
                    if (next) setFilterLabel(next)
                    setPagination((prev) => ({ ...prev, pageIndex: 0 }))
                  }}
                  variant="outline"
                  size="sm"
                  spacing={0}
                >
                  {payload.filterLabels.map((label) => (
                    <ToggleGroupItem key={label} value={label}>
                      {label}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              ) : null
            }
            actions={
              <span className="text-xs text-muted-foreground" aria-live="polite">
                共 {filtered.length.toLocaleString("zh-CN")} 条
              </span>
            }
          />
        }
        table={
          <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.id}
            rowCount={filtered.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="compact"
          />
        }
      />

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{payload.primaryActionLabel ?? "新建"}</DialogTitle>
            <DialogDescription>
              将创建一条草稿记录（演示环境，不写入真实数据）。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              取消
            </DialogClose>
            <Button
              type="button"
              onClick={() => {
                const reference = `NEW-${def.id}-${Date.now().toString(36).toUpperCase()}`
                setActionResult({
                  title: `${payload.primaryActionLabel ?? "新建"}已提交`,
                  description: "草稿已创建，可继续业务录入。",
                  reference,
                })
                setCreateOpen(false)
              }}
            >
              确认创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

export function ListWorkspaceLoading({ title }: { title: string }) {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader title={title} />
      <div className="rounded-lg border border-border bg-card p-8 text-sm text-muted-foreground">
        加载中
      </div>
    </div>
  )
}

export function ListWorkspaceError({
  title,
  onRetry,
}: {
  title: string
  onRetry: () => void
}) {
  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader title={title} description="列表数据加载失败。" />
      <Button type="button" onClick={onRetry}>
        重试
      </Button>
    </div>
  )
}
