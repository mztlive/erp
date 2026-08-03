"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
  BusinessDiffPanel,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ImportIssueTable,
  ImportStageIndicator,
  MetricItem,
  MetricStrip,
  PageHeader,
  type ImportStageKey,
  type ImportStageStates,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import type { ListRow, WorkspacePageDef } from "@/features/workspace-kit/types"

function buildImportStages(
  stages: readonly {
    key: string
    label: string
    status: "pending" | "current" | "complete" | "failed"
  }[]
): ImportStageStates {
  const ordered: ImportStageKey[] = [
    "upload",
    "mapping",
    "validation",
    "preview",
    "submission",
    "result",
  ]
  const result = {} as Record<
    ImportStageKey,
    { status: "pending" | "current" | "complete" | "failed"; description?: string }
  >
  for (let i = 0; i < ordered.length; i += 1) {
    const source = stages[i]
    result[ordered[i]] = source
      ? { status: source.status, description: source.label }
      : { status: "pending" }
  }
  return result as ImportStageStates
}

export function GovernanceWorkspacePage({ def }: { def: WorkspacePageDef }) {
  if (def.shell.kind !== "governance") {
    throw new Error(
      `GovernanceWorkspacePage expects governance shell for ${def.id}`
    )
  }
  const { payload } = def.shell
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })

  const stageStates = React.useMemo(
    () => buildImportStages(payload.stages),
    [payload.stages]
  )

  const columns = React.useMemo<ColumnDef<ListRow>[]>(
    () =>
      payload.batchColumns.map((column) => ({
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
      })),
    [payload.batchColumns]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return payload.batches.slice(start, start + pagination.pageSize)
  }, [pagination.pageIndex, pagination.pageSize, payload.batches])

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
            state="fresh"
            label="治理数据"
          />
        }
      />

      <Card size="sm">
        <CardHeader className="border-b">
          <CardTitle>治理阶段</CardTitle>
          <CardDescription>当前批次所处阶段。</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4 pt-4">
          <div className="flex flex-wrap gap-2">
            {payload.stages.map((stage) => (
              <Badge
                key={stage.key}
                variant={
                  stage.status === "current"
                    ? "info"
                    : stage.status === "complete"
                      ? "secondary"
                      : stage.status === "failed"
                        ? "destructive"
                        : "outline"
                }
              >
                {stage.label}
                {stage.status === "current" ? " · 进行中" : ""}
                {stage.status === "complete" ? " · 已完成" : ""}
              </Badge>
            ))}
          </div>
          <ImportStageIndicator stages={stageStates} aria-label="治理进度" />
        </CardContent>
      </Card>

      <MetricStrip
        columns={Math.min(4, payload.metrics.length) as 2 | 3 | 4}
        aria-label={`${def.title}治理指标`}
      >
        {payload.metrics.map((metric) => (
          <MetricItem
            key={metric.key}
            label={metric.label}
            value={metric.value}
            detail={metric.detail}
          />
        ))}
      </MetricStrip>

      <BusinessTableFrame
        title="批次与任务"
        description="按阶段推进；确认前展示差异与校验问题。"
        table={
          <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.id}
            rowCount={payload.batches.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="compact"
          />
        }
      />

      {payload.issues.length > 0 ? (
        <ImportIssueTable
          issues={payload.issues.map((issue, index) => ({
            id: issue.id,
            rowNumber: issue.objectLabel ?? index + 1,
            field: issue.field ?? "—",
            errorCode:
              issue.severity === "error"
                ? "错误"
                : issue.severity === "warning"
                  ? "警告"
                  : "提示",
            message: issue.message,
            repairable: issue.severity !== "error",
          }))}
        />
      ) : null}

      {payload.diffEntries && payload.diffEntries.length > 0 ? (
        <BusinessDiffPanel
          changes={payload.diffEntries.map((entry) => ({
            id: entry.id,
            field: entry.field,
            before: entry.before,
            after: entry.after,
          }))}
        />
      ) : null}
    </div>
  )
}
