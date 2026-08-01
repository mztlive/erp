"use client"

import * as React from "react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts"

import {
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  MetricItem,
  MetricStrip,
  PageHeader,
} from "@/components/business"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import type { ListRow, WorkspacePageDef } from "@/features/workspace-kit/types"

const chartConfig = {
  value: {
    label: "数值",
    color: "var(--chart-1)",
  },
} satisfies ChartConfig

export function AnalyticsWorkspacePage({ def }: { def: WorkspacePageDef }) {
  if (def.shell.kind !== "analytics") {
    throw new Error(
      `AnalyticsWorkspacePage expects analytics shell for ${def.id}`
    )
  }
  const { payload } = def.shell
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })

  const columns = React.useMemo<ColumnDef<ListRow>[]>(
    () =>
      payload.columns.map((column) => ({
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
    [payload.columns]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return payload.rows.slice(start, start + pagination.pageSize)
  }, [pagination.pageIndex, pagination.pageSize, payload.rows])

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
            updatedAt="今天 09:30"
            dateTime="2026-08-01T09:30:00+08:00"
            state="fresh"
            label="分析汇总"
          />
        }
      />

      <MetricStrip
        columns={Math.min(5, payload.metrics.length) as 2 | 3 | 4 | 5}
        aria-label={`${def.title}核心指标`}
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

      <div className="grid min-w-0 gap-4 xl:grid-cols-2">
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>{payload.seriesTitle}</CardTitle>
            <CardDescription>只读汇总，不可在此页改数。</CardDescription>
          </CardHeader>
          <CardContent className="pt-4">
            <ChartContainer config={chartConfig} className="aspect-[16/9] w-full">
              <BarChart data={[...payload.series]} accessibilityLayer>
                <CartesianGrid vertical={false} />
                <XAxis dataKey="label" tickLine={false} axisLine={false} />
                <YAxis tickLine={false} axisLine={false} width={40} />
                <ChartTooltip content={<ChartTooltipContent />} />
                <Bar dataKey="value" fill="var(--color-value)" radius={4} />
              </BarChart>
            </ChartContainer>
          </CardContent>
        </Card>

        {payload.notes && payload.notes.length > 0 ? (
          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>口径说明</CardTitle>
            </CardHeader>
            <CardContent>
              <ul className="list-disc space-y-2 pl-5 text-sm text-muted-foreground">
                {payload.notes.map((note) => (
                  <li key={note}>{note}</li>
                ))}
              </ul>
            </CardContent>
          </Card>
        ) : (
          <Card size="sm">
            <CardHeader className="border-b">
              <CardTitle>数据更新时间</CardTitle>
            </CardHeader>
            <CardContent className="text-sm text-muted-foreground">
              指标与表格均来自系统最新数据；金额与利润字段按权限展示。
            </CardContent>
          </Card>
        )}
      </div>

      <BusinessTableFrame
        title={payload.tableTitle}
        description="下钻明细仅展示授权范围内的数据。"
        table={
          <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.id}
            rowCount={payload.rows.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="compact"
          />
        }
      />
    </div>
  )
}
