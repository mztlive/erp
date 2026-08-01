"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState, RowSelectionState } from "@tanstack/react-table"
import {
  ExternalLinkIcon,
  RefreshCwIcon,
  SearchIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  BackgroundJobProgress,
  BusinessEmptyState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  DocumentHeader,
  DocumentSection,
  DocumentSummary,
  FormalActionConfirmDialog,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  PageActions,
  PageHeader,
  QuickPreviewSheet,
  RevisionTimeline,
  StatusTrackSummary,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  useBulkProjectionCommandMutation,
  useExecutionProjectionDetailQuery,
  useExecutionProjectionListQuery,
  useProjectionDeliveryCommandMutation,
} from "@/features/execution-projections/queries"
import type {
  BulkProjectionJob,
  DeliveryStatus,
  ExecutionProjectionMetricKey,
  ExecutionProjectionRow,
  LatencyBand,
  ProjectionDeliveryCommandResult,
  ProjectionSource,
  ReconciliationStatus,
} from "@/features/execution-projections/types"
import {
  DELIVERY_STATUS_LABEL,
  LATENCY_LABEL,
  RECONCILIATION_LABEL,
  SOURCE_LABEL,
} from "@/features/execution-projections/types"
import { cn } from "@/lib/utils"

type ResultState = {
  status: "succeeded" | "blocked" | "unknown" | "processing"
  title: string
  description: string
  reference: string
  facts: Array<{ label: string; value: string }>
  w29Href?: string
  stayUnknown?: boolean
} | null

type PendingAction =
  | {
      kind: "QUERY_RESULT"
      row: ExecutionProjectionRow
      objectVersion: string
    }
  | {
      kind: "RETRY"
      row: ExecutionProjectionRow
      objectVersion: string
    }
  | {
      kind: "ESCALATE"
      row: ExecutionProjectionRow
      objectVersion: string
    }
  | {
      kind: "BULK_QUERY"
      ids: string[]
    }
  | {
      kind: "BULK_RETRY"
      ids: string[]
    }
  | null

function parseMetric(raw: string | null): ExecutionProjectionMetricKey | "all" {
  if (
    raw === "pending_send" ||
    raw === "inflight" ||
    raw === "timeout" ||
    raw === "fail_manual" ||
    raw === "acked"
  ) {
    return raw
  }
  return "all"
}

function parseSource(raw: string | null): ProjectionSource | "all" {
  if (raw === "MIGRATION_BASELINE" || raw === "ERP_SALES_REVISION") return raw
  return "all"
}

function parseLatency(raw: string | null): LatencyBand | "all" {
  if (raw === "normal" || raw === "near_sla" || raw === "over_sla") return raw
  return "all"
}

function parseRecon(raw: string | null): ReconciliationStatus | "all" {
  if (raw === "MATCHED" || raw === "VERSION_MISMATCH" || raw === "NONE") {
    return raw
  }
  return "all"
}

function w29Href(workItemId?: string, errorTaskId?: string) {
  const params = new URLSearchParams()
  if (workItemId) params.set("workItemId", workItemId)
  if (errorTaskId) params.set("errorTaskId", errorTaskId)
  params.set("from", "W23")
  const qs = params.toString()
  return `/governance/integration-errors${qs ? `?${qs}` : ""}`
}

function shortHash(hash: string) {
  if (hash.length <= 18) return hash
  return `${hash.slice(0, 12)}…${hash.slice(-4)}`
}

function commandToResultState(
  result: ProjectionDeliveryCommandResult
): ResultState {
  if (result.stillUnknown || result.result === "STILL_UNKNOWN") {
    return {
      status: "unknown",
      title: "结果仍未知",
      description:
        "未明确前不显示成功、不跳过、不计入已确认指标。请再次查询或升级到接口错误中心。",
      reference: result.operationId,
      stayUnknown: true,
      facts: [
        { label: "操作编号", value: result.operationId },
        { label: "对象", value: `${result.salesOrderNo} · ${result.deliveryId}` },
        { label: "时间", value: result.occurredAt },
        { label: "下一步", value: result.nextAction },
      ],
      w29Href: w29Href(result.workItemId, result.errorTaskId),
    }
  }
  if (result.result === "ESCALATED") {
    return {
      status: "succeeded",
      title: "已升级到错误中心",
      description: "处理任务仅在错误中心领取与完成；本页不提供任务处理。",
      reference: result.operationId,
      facts: [
        { label: "操作编号", value: result.operationId },
        { label: "对象", value: `${result.salesOrderNo} · ${result.deliveryId}` },
        { label: "时间", value: result.occurredAt },
        { label: "下一步", value: result.nextAction },
        {
          label: "W29 任务",
          value: result.workItemId ?? result.errorTaskId ?? "—",
        },
      ],
      w29Href: w29Href(result.workItemId, result.errorTaskId),
    }
  }
  if (result.result === "FAILED") {
    return {
      status: "blocked",
      title: result.resultLabel,
      description: "销售记录与应收未回退。可重试投递或升级到接口错误中心。",
      reference: result.operationId,
      facts: [
        { label: "操作编号", value: result.operationId },
        { label: "对象", value: `${result.salesOrderNo} · ${result.deliveryId}` },
        { label: "时间", value: result.occurredAt },
        { label: "下一步", value: result.nextAction },
      ],
    }
  }
  return {
    status: "succeeded",
    title: result.resultLabel,
    description: result.nextAction,
    reference: result.operationId,
    facts: [
      { label: "操作编号", value: result.operationId },
      { label: "对象", value: `${result.salesOrderNo} · ${result.deliveryId}` },
      { label: "时间", value: result.occurredAt },
      { label: "下一步", value: result.nextAction },
    ],
  }
}

export function ExecutionProjectionsPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const q = searchParams.get("q") ?? ""
  const mallId = searchParams.get("mall") ?? "all"
  const deliveryStatus = searchParams.get("deliveryStatus") ?? "all"
  const source = parseSource(searchParams.get("source"))
  const latency = parseLatency(searchParams.get("latency"))
  const reconciliation = parseRecon(searchParams.get("reconciliation"))
  const metric = parseMetric(searchParams.get("metric"))
  const projectionId = searchParams.get("projectionId") ?? undefined
  const revisionId = searchParams.get("revision") ?? undefined
  const page = Math.max(1, Number(searchParams.get("page") ?? "1") || 1)
  const pageSize = 8

  const listQueryInput = React.useMemo(
    () => ({
      q: q || undefined,
      mallId: mallId === "all" ? undefined : mallId,
      deliveryStatus:
        deliveryStatus === "all" ? undefined : deliveryStatus,
      source,
      latency,
      reconciliation,
      metric,
      page,
      pageSize,
    }),
    [
      q,
      mallId,
      deliveryStatus,
      source,
      latency,
      reconciliation,
      metric,
      page,
      pageSize,
    ]
  )

  const listQuery = useExecutionProjectionListQuery(listQueryInput)
  const detailQuery = useExecutionProjectionDetailQuery(
    projectionId,
    revisionId
  )
  const commandMutation = useProjectionDeliveryCommandMutation()
  const bulkMutation = useBulkProjectionCommandMutation()

  const [searchDraft, setSearchDraft] = React.useState(q)
  React.useEffect(() => {
    setSearchDraft(q)
  }, [q])

  const [rowSelection, setRowSelection] = React.useState<RowSelectionState>({})
  const [result, setResult] = React.useState<ResultState>(null)
  const [bulkJob, setBulkJob] = React.useState<BulkProjectionJob | null>(null)
  const [pendingAction, setPendingAction] = React.useState<PendingAction>(null)
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [objectTab, setObjectTab] = React.useState("overview")
  const resultRef = React.useRef<HTMLDivElement>(null)

  const replaceParams = React.useCallback(
    (patch: Record<string, string | null | undefined>) => {
      const next = new URLSearchParams(searchParams.toString())
      for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === "" || value === "all") next.delete(key)
        else next.set(key, value)
      }
      const qs = next.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
    },
    [pathname, router, searchParams]
  )

  const view = listQuery.data
  const rows = view?.rows ?? []
  const metrics = view?.metrics ?? []
  const total = view?.pageInfo.total ?? 0
  const selectedIds = React.useMemo(
    () => Object.keys(rowSelection).filter((id) => rowSelection[id]),
    [rowSelection]
  )

  const previewQuery = useExecutionProjectionDetailQuery(
    previewId ?? undefined
  )

  React.useEffect(() => {
    if (result) {
      resultRef.current?.focus()
    }
  }, [result])

  const pagination: PaginationState = {
    pageIndex: page - 1,
    pageSize,
  }

  const columns = React.useMemo<ColumnDef<ExecutionProjectionRow>[]>(
    () => [
      {
        id: "select",
        header: ({ table }) => (
          <Checkbox
            aria-label="全选本页可选项"
            checked={table.getIsAllPageRowsSelected()}
            indeterminate={
              table.getIsSomePageRowsSelected() &&
              !table.getIsAllPageRowsSelected()
            }
            onCheckedChange={(value) =>
              table.toggleAllPageRowsSelected(Boolean(value))
            }
          />
        ),
        cell: ({ row }) => (
          <Checkbox
            aria-label={`选择 ${row.original.salesOrderNo}`}
            checked={row.getIsSelected()}
            onCheckedChange={(value) => row.toggleSelected(Boolean(value))}
            onClick={(e) => e.stopPropagation()}
          />
        ),
        meta: { label: "选择", width: "status" },
        enableSorting: false,
      },
      {
        id: "salesOrder",
        accessorKey: "salesOrderNo",
        header: "销售单",
        meta: { label: "销售单", width: "default" },
        cell: ({ row }) => (
          <div className="min-w-[9rem]">
            <div className="num text-sm font-medium">
              {row.original.salesOrderNo}
            </div>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.customerLabel}
            </div>
          </div>
        ),
      },
      {
        id: "erpVersion",
        header: "ERP版本",
        meta: { label: "ERP版本", width: "status", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm">
            v{row.original.salesOrderRevisionNo}
          </span>
        ),
      },
      {
        id: "projVersion",
        header: "数据版本",
        meta: { label: "数据版本", width: "status", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm">
            v{row.original.projectionRevisionNo}
          </span>
        ),
      },
      {
        id: "source",
        header: "来源",
        meta: { label: "来源", width: "default" },
        cell: ({ row }) => (
          <Badge
            variant={
              row.original.projectionSource === "MIGRATION_BASELINE"
                ? "warning"
                : "secondary"
            }
          >
            {SOURCE_LABEL[row.original.projectionSource]}
          </Badge>
        ),
      },
      {
        id: "mall",
        accessorKey: "targetMallName",
        header: "商城",
        meta: { label: "商城", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.targetMallName}</span>
        ),
      },
      {
        id: "delivery",
        header: "接收状态",
        meta: { label: "接收状态", width: "status" },
        cell: ({ row }) => (
          <div className="flex flex-col gap-1">
            <BusinessStatusBadge
              context="list"
              label={row.original.delivery.statusLabel}
              tone={row.original.delivery.statusTone}
            />
            {row.original.latencyBand === "over_sla" ? (
              <span className="text-[11px] text-warning-foreground">
                超过 SLA
              </span>
            ) : row.original.latencyBand === "near_sla" ? (
              <span className="text-[11px] text-muted-foreground">
                接近 SLA
              </span>
            ) : null}
          </div>
        ),
      },
      {
        id: "acked",
        header: "商城已确认版",
        meta: { label: "商城已确认版", width: "status", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.currentAckedRevisionNo != null
              ? `v${row.original.currentAckedRevisionNo}`
              : "尚未确认"}
          </span>
        ),
      },
      {
        id: "attempt",
        header: "最近尝试",
        meta: { label: "最近尝试", width: "default" },
        cell: ({ row }) => (
          <div className="text-xs">
            <div className="num">
              {row.original.delivery.attemptCount} 次
            </div>
            <div className="text-muted-foreground">
              {row.original.delivery.lastAttemptAt ?? "—"}
            </div>
          </div>
        ),
      },
      {
        id: "error",
        header: "失败原因",
        meta: { label: "失败原因", width: "default" },
        cell: ({ row }) => (
          <div className="max-w-[12rem]">
            {row.original.reconciliationStatus === "VERSION_MISMATCH" ? (
              <Badge variant="warning" className="mb-1">
                版本差异
              </Badge>
            ) : null}
            <span className="line-clamp-2 text-xs text-muted-foreground">
              {row.original.delivery.errorSummary ?? "—"}
            </span>
          </div>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => {
          const r = row.original
          const canQuery = r.allowedActions.includes("QUERY_RESULT")
          const canRetry = r.allowedActions.includes("RETRY")
          const canEscalate = r.allowedActions.includes("ESCALATE")
          return (
            <div
              className="flex min-w-[11rem] flex-wrap justify-end gap-1"
              onClick={(e) => e.stopPropagation()}
              onKeyDown={(e) => e.stopPropagation()}
            >
              <Button
                type="button"
                size="xs"
                variant="ghost"
                onClick={() => setPreviewId(r.projectionId)}
              >
                预览
              </Button>
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() =>
                  replaceParams({
                    projectionId: r.projectionId,
                    revision: null,
                  })
                }
              >
                打开
              </Button>
              <Button
                type="button"
                size="xs"
                variant="outline"
                render={
                  <Link href={`/sales/orders/${r.salesOrderId}?section=collaboration`} />
                }
              >
                销售单
              </Button>
              {canQuery ? (
                <Button
                  type="button"
                  size="xs"
                  disabled={commandMutation.isPending}
                  onClick={() =>
                    setPendingAction({
                      kind: "QUERY_RESULT",
                      row: r,
                      objectVersion: r.objectVersion,
                    })
                  }
                >
                  查询结果
                </Button>
              ) : null}
              {canRetry ? (
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  disabled={commandMutation.isPending}
                  onClick={() =>
                    setPendingAction({
                      kind: "RETRY",
                      row: r,
                      objectVersion: r.objectVersion,
                    })
                  }
                >
                  重试
                </Button>
              ) : null}
              {canEscalate ||
              r.reconciliationStatus === "VERSION_MISMATCH" ||
              r.delivery.workItemId ? (
                <Button
                  type="button"
                  size="xs"
                  variant="outline"
                  render={
                    <Link
                      href={w29Href(
                        r.delivery.workItemId,
                        r.delivery.errorTaskId
                      )}
                    />
                  }
                >
                  W29
                </Button>
              ) : null}
            </div>
          )
        },
      },
    ],
    [commandMutation.isPending, replaceParams]
  )

  const openConfirmForRow = async (
    kind: "QUERY_RESULT" | "RETRY" | "ESCALATE",
    row: ExecutionProjectionRow,
    objectVersion: string
  ) => {
    try {
      const detail = detailQuery.data?.identity.projectionId === row.projectionId
        ? detailQuery.data
        : null
      const version = detail?.objectVersion ?? objectVersion
      const result = await commandMutation.mutateAsync({
        projectionId: row.projectionId,
        projectionRevisionId: row.projectionRevisionId,
        deliveryId: row.delivery.deliveryId,
        action: kind,
        expectedObjectVersion: version,
        requestId: `req-${Date.now().toString(36)}`,
        idempotencyKey: `w23-${kind}-${row.projectionId}-${row.projectionRevisionId}`,
      })
      setResult(commandToResultState(result))
      setPendingAction(null)
    } catch (err) {
      setResult({
        status: "blocked",
        title: "操作被阻断",
        description: err instanceof Error ? err.message : "请刷新后重试",
        reference: row.projectionNo,
        facts: [
          { label: "对象", value: row.salesOrderNo },
          { label: "动作", value: kind },
        ],
      })
      setPendingAction(null)
    }
  }

  const runBulk = async (kind: "BULK_QUERY" | "BULK_RETRY") => {
    try {
      const job = await bulkMutation.mutateAsync({
        action: kind,
        projectionIds: selectedIds,
        requestId: `bulk-${Date.now().toString(36)}`,
        idempotencyKey: `w23-${kind}-${selectedIds.slice().sort().join(",")}`,
      })
      setBulkJob(job)
      setRowSelection({})
      setPendingAction(null)
      setResult({
        status:
          job.stillUnknown > 0
            ? "unknown"
            : job.status === "failed"
              ? "blocked"
              : "succeeded",
        title:
          kind === "BULK_RETRY" ? "批量重试已执行" : "批量查询已执行",
        description: job.nextAction,
        reference: job.jobId,
        facts: [
          { label: "操作编号", value: job.jobId },
          {
            label: "选择结果",
            value: job.selectionSnapshotId,
          },
          {
            label: "成功/跳过/失败/仍未知",
            value: `${job.succeeded}/${job.skipped}/${job.failed}/${job.stillUnknown}`,
          },
          { label: "下一步", value: job.nextAction },
        ],
      })
    } catch (err) {
      setResult({
        status: "blocked",
        title: "批量操作被阻断",
        description: err instanceof Error ? err.message : "请重试",
        reference: "bulk",
        facts: [],
      })
      setPendingAction(null)
    }
  }

  const detail = detailQuery.data
  const objectOpen = Boolean(projectionId)

  if (listQuery.isPending && !view) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
        <PageHeader title="执行信息" description="正在加载列表…" />
        <div className="h-20 animate-pulse rounded-xl bg-muted" />
        <div className="h-64 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (listQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
        <PageHeader title="执行信息" description="列表加载失败" />
        <Button type="button" onClick={() => void listQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:gap-3.5 md:p-4">
      <PageHeader
        title="执行信息"
        breadcrumbs={[
          {
            id: "com",
            label: "商城与发布",
            href: "/commerce/execution-projections",
          },
          { id: "ep", label: "执行信息", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt="刚刚"
            dateTime={view?.queriedAt}
            state="fresh"
            label="投递状态"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "refresh",
                label: "刷新",
                icon: RefreshCwIcon,
                variant: "outline",
                onClick: () => {
                  void listQuery.refetch()
                },
              },
              {
                actionKey: "bulk-query",
                label: "批量查询",
                variant: "outline",
                mobileVisibility: "hide",
                disabled: selectedIds.length === 0 || bulkMutation.isPending,
                onClick: () =>
                  setPendingAction({
                    kind: "BULK_QUERY",
                    ids: selectedIds,
                  }),
              },
              {
                actionKey: "bulk-retry",
                label: "批量重试",
                mobileVisibility: "hide",
                disabled: selectedIds.length === 0 || bulkMutation.isPending,
                onClick: () =>
                  setPendingAction({
                    kind: "BULK_RETRY",
                    ids: selectedIds,
                  }),
              },
            ]}
          />
        }
      />

      <Alert>
        <ShieldAlertIcon aria-hidden="true" />
        <AlertTitle>非写者边界</AlertTitle>
        <AlertDescription>
          执行信息由已生效销售版本自动形成。接收失败不回退销售记录、销售版本或应收；
          内容变化须在销售单走变更单。本页仅支持查询结果、重试与升级到接口错误中心，
          不提供任务领取/完成。任何角色下不展示成交金额、配赠、税率、开票、应收与玩法规则。
        </AlertDescription>
      </Alert>

      <div ref={resultRef} tabIndex={-1} className="outline-none">
        {result ? (
          <FormalActionResult
            status={result.status}
            title={result.title}
            description={result.description}
            reference={result.reference}
            facts={result.facts}
            actions={
              result.w29Href ? (
                <Button
                  type="button"
                  size="sm"
                  render={<Link href={result.w29Href} />}
                >
                  打开接口错误中心
                </Button>
              ) : null
            }
          />
        ) : null}
      </div>

      {bulkJob ? (
        <BackgroundJobProgress
          mode="partialAllowed"
          status={bulkJob.status}
          total={bulkJob.total}
          completed={bulkJob.completed}
          succeeded={bulkJob.succeeded}
          skipped={bulkJob.skipped + bulkJob.stillUnknown}
          failed={bulkJob.failed}
          label={
            bulkJob.action === "BULK_RETRY"
              ? "批量重试任务"
              : "批量查询任务"
          }
          description={
            <>
              服务端筛选结果{" "}
              <span className="num">{bulkJob.selectionSnapshotId}</span>
              。成功 {bulkJob.succeeded} · 跳过 {bulkJob.skipped} · 仍未知{" "}
              {bulkJob.stillUnknown} · 失败 {bulkJob.failed}。
              {bulkJob.stillUnknown > 0
                ? " 仍未知项未按成功处理、未计入已确认。"
                : null}
            </>
          }
        />
      ) : null}

      <MetricStrip columns={5} aria-label="执行信息指标筛选">
        {metrics.map((m) => (
          <MetricFilterItem
            key={m.key}
            label={m.label}
            value={m.value}
            detail={m.detail}
            active={metric === m.key}
            onClick={() =>
              replaceParams({
                metric: metric === m.key ? null : m.key,
                page: "1",
              })
            }
          />
        ))}
      </MetricStrip>

      <ListToolbar
        search={
          <form
            className="flex min-w-[14rem] flex-1 gap-2"
            onSubmit={(e) => {
              e.preventDefault()
              replaceParams({ q: searchDraft.trim() || null, page: "1" })
            }}
          >
            <InputGroup className="max-w-sm">
              <InputGroupAddon>
                <SearchIcon aria-hidden="true" />
              </InputGroupAddon>
              <InputGroupInput
                value={searchDraft}
                onChange={(e) => setSearchDraft(e.target.value)}
                placeholder="销售单号、执行编号、客户"
                aria-label="搜索执行信息"
              />
            </InputGroup>
            <Button type="submit" size="sm" variant="secondary">
              搜索
            </Button>
          </form>
        }
        filters={
          <div className="flex flex-wrap items-center gap-2">
            <NativeSelect
              aria-label="目标商城"
              value={mallId}
              onChange={(e) =>
                replaceParams({ mall: e.target.value, page: "1" })
              }
            >
              <NativeSelectOption value="all">全部商城</NativeSelectOption>
              {(view?.malls ?? []).map((m) => (
                <NativeSelectOption key={m.id} value={m.id}>
                  {m.name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <NativeSelect
              aria-label="接收状态"
              value={deliveryStatus}
              onChange={(e) =>
                replaceParams({
                  deliveryStatus: e.target.value,
                  page: "1",
                })
              }
            >
              <NativeSelectOption value="all">全部接收状态</NativeSelectOption>
              {(
                [
                  "UNKNOWN",
                  "FAILED",
                  "ESCALATED_MANUAL",
                  "RETRYING",
                  "SENDING",
                  "PENDING",
                  "ACKED",
                ] as DeliveryStatus[]
              ).map((s) => (
                <NativeSelectOption key={s} value={s}>
                  {DELIVERY_STATUS_LABEL[s]}
                </NativeSelectOption>
              ))}
              <NativeSelectOption value="UNKNOWN,FAILED,ESCALATED_MANUAL">
                未知+失败+转人工
              </NativeSelectOption>
            </NativeSelect>
            <NativeSelect
              aria-label="延迟分组"
              value={latency}
              onChange={(e) =>
                replaceParams({ latency: e.target.value, page: "1" })
              }
            >
              <NativeSelectOption value="all">延迟：全部</NativeSelectOption>
              {(Object.keys(LATENCY_LABEL) as LatencyBand[]).map((k) => (
                <NativeSelectOption key={k} value={k}>
                  {LATENCY_LABEL[k]}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <NativeSelect
              aria-label="版本差异"
              value={reconciliation}
              onChange={(e) =>
                replaceParams({
                  reconciliation: e.target.value,
                  page: "1",
                })
              }
            >
              <NativeSelectOption value="all">对账：全部</NativeSelectOption>
              <NativeSelectOption value="VERSION_MISMATCH">
                仅版本差异
              </NativeSelectOption>
              <NativeSelectOption value="MATCHED">版本一致</NativeSelectOption>
            </NativeSelect>
            <NativeSelect
              aria-label="数据来源"
              value={source}
              onChange={(e) =>
                replaceParams({ source: e.target.value, page: "1" })
              }
            >
              <NativeSelectOption value="all">来源：全部</NativeSelectOption>
              <NativeSelectOption value="ERP_SALES_REVISION">
                ERP 销售版本
              </NativeSelectOption>
              <NativeSelectOption value="MIGRATION_BASELINE">
                迁移基线
              </NativeSelectOption>
            </NativeSelect>
          </div>
        }
        actions={
          <span className="text-xs text-muted-foreground">
            {view?.filterSummary}
            {" · "}
            <span className="num">{total}</span> 条
          </span>
        }
      />

      {selectedIds.length > 0 ? (
        <div
          role="region"
          aria-label="批量选择"
          className="flex flex-wrap items-center justify-between gap-2 rounded-2xl border border-border bg-muted/40 px-3 py-2 text-sm"
        >
          <span>
            已显式选择{" "}
            <span className="num font-medium">{selectedIds.length}</span>{" "}
            项（Q3：不支持当前筛选全部；服务端冻结筛选结果）
          </span>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => setRowSelection({})}
            >
              清除选择
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={bulkMutation.isPending}
              onClick={() =>
                setPendingAction({ kind: "BULK_QUERY", ids: selectedIds })
              }
            >
              批量查询结果
            </Button>
            <Button
              type="button"
              size="sm"
              disabled={bulkMutation.isPending}
              onClick={() =>
                setPendingAction({ kind: "BULK_RETRY", ids: selectedIds })
              }
            >
              批量重试
            </Button>
          </div>
        </div>
      ) : null}

      <BusinessTableFrame
        title="执行信息列表"
        description="销售单身份列与操作列固定；每页 6–8 行。指标与行数据同源权限范围。"
        table={
          <DataTable
            columns={columns}
            data={rows}
            getRowId={(row) => row.projectionId}
            rowCount={total}
            enableRowSelection
            rowSelection={rowSelection}
            onRowSelectionChange={setRowSelection}
            onRowPreview={(row) => setPreviewId(row.projectionId)}
            onRowOpen={(row) =>
              replaceParams({
                projectionId: row.projectionId,
                revision: null,
              })
            }
            pagination={pagination}
            onPaginationChange={(next) => {
              replaceParams({ page: String(next.pageIndex + 1) })
            }}
            manualPagination
            layout="flush"
            density="compact"
            pageSizeOptions={[8, 20, 50]}
            defaultColumnPinning={{
              left: ["select", "salesOrder"],
              right: ["actions"],
            }}
            emptyState={
              rows.length === 0 ? (
                <BusinessEmptyState
                  kind="filter"
                  title="没有匹配的执行信息"
                  description={
                    view?.filterSummary
                      ? `当前筛选：${view.filterSummary}`
                      : "可清除筛选或返回销售单查看协同。"
                  }
                  action={
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() =>
                        replaceParams({
                          q: null,
                          mall: null,
                          deliveryStatus: null,
                          source: null,
                          latency: null,
                          reconciliation: null,
                          metric: null,
                          page: null,
                        })
                      }
                    >
                      清除筛选
                    </Button>
                  }
                />
              ) : undefined
            }
          />
        }
      />

      <p className="text-xs text-muted-foreground">
        列表约 6–8 行/页（当前 pageSize={pageSize}
        ）。销售单列与操作列固定。结果未知不计入「已确认」指标。
        {view?.defaultViewNote}
      </p>

      {/* 对象中心半屏 / 主区 */}
      <QuickPreviewSheet
        open={objectOpen}
        onOpenChange={(open) => {
          if (!open) replaceParams({ projectionId: null, revision: null })
        }}
        size="detail"
        title={
          detail
            ? `执行信息 · ${detail.identity.salesOrderNo}`
            : "执行信息对象"
        }
        description={
          detail
            ? `${detail.identity.projectionNo} · ${detail.identity.targetMallName}`
            : "加载中…"
        }
        identity={
          detail ? (
            <span className="num">{detail.identity.projectionId}</span>
          ) : null
        }
      >
        {detailQuery.isPending ? (
          <div className="h-48 animate-pulse rounded-xl bg-muted" />
        ) : detailQuery.isError || !detail ? (
          <BusinessEmptyState
            kind="no-data"
            title="无法加载数据"
            description="数据不存在或无权访问。"
          />
        ) : (
          <div className="flex flex-col gap-4">
            <DocumentHeader
              title={detail.identity.salesOrderNo}
              documentNumber={detail.identity.projectionNo}
              version={`数据 v${detail.selectedRevision.revisionNo} · ERP v${detail.selectedRevision.salesOrderRevisionNo}`}
              primaryStatus={{
                label: detail.tracks.projectionDelivery.label,
                tone: detail.tracks.projectionDelivery.tone,
              }}
              statuses={[
                {
                  id: "sales-fact",
                  label: "销售记录",
                  status: {
                    label: detail.tracks.salesFact.label,
                    tone: detail.tracks.salesFact.tone,
                  },
                },
                {
                  id: "delivery",
                  label: "信息投递",
                  status: {
                    label: detail.tracks.projectionDelivery.label,
                    tone: detail.tracks.projectionDelivery.tone,
                  },
                },
                {
                  id: "mall",
                  label: "商城确认",
                  status: {
                    label: detail.tracks.mallConfirm.label,
                    tone: detail.tracks.mallConfirm.tone,
                  },
                },
              ]}
              primaryAction={
                detail.allowedActions.includes("QUERY_RESULT") ? (
                  <Button
                    type="button"
                    size="sm"
                    disabled={commandMutation.isPending}
                    onClick={() => {
                      const row = rows.find(
                        (r) => r.projectionId === detail.identity.projectionId
                      )
                      if (!row) return
                      setPendingAction({
                        kind: "QUERY_RESULT",
                        row,
                        objectVersion: detail.objectVersion,
                      })
                    }}
                  >
                    查询结果
                  </Button>
                ) : undefined
              }
              secondaryActions={
                <>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={
                      <Link
                        href={`/sales/orders/${detail.identity.salesOrderId}?section=collaboration`}
                      />
                    }
                  >
                    打开销售单协同
                  </Button>
                  {detail.allowedActions.includes("RETRY") ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={commandMutation.isPending}
                      onClick={() => {
                        const row = rows.find(
                          (r) =>
                            r.projectionId === detail.identity.projectionId
                        )
                        if (!row) return
                        setPendingAction({
                          kind: "RETRY",
                          row,
                          objectVersion: detail.objectVersion,
                        })
                      }}
                    >
                      重试投递
                    </Button>
                  ) : null}
                  {detail.allowedActions.includes("ESCALATE") ||
                  detail.reconciliationStatus === "VERSION_MISMATCH" ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => {
                        const d = detail.deliveries[0]
                        if (
                          detail.allowedActions.includes("ESCALATE") &&
                          !d?.workItemId
                        ) {
                          const row = rows.find(
                            (r) =>
                              r.projectionId === detail.identity.projectionId
                          )
                          if (row) {
                            setPendingAction({
                              kind: "ESCALATE",
                              row,
                              objectVersion: detail.objectVersion,
                            })
                            return
                          }
                        }
                        router.push(
                          w29Href(d?.workItemId, d?.errorTaskId)
                        )
                      }}
                    >
                      升级 / 打开接口错误中心
                    </Button>
                  ) : null}
                </>
              }
            />

            <Alert>
              <TriangleAlertIcon aria-hidden="true" />
              <AlertTitle>只读提示</AlertTitle>
              <AlertDescription>{detail.boundaryNotice}</AlertDescription>
            </Alert>

            <StatusTrackSummary
              aria-label="对象中心三轨状态"
              variant="table"
              tracks={[
                {
                  id: "sales-fact",
                  label: "销售记录",
                  status: {
                    label: detail.tracks.salesFact.label,
                    tone: detail.tracks.salesFact.tone,
                    description: detail.tracks.salesFact.description,
                  },
                },
                {
                  id: "projection-delivery",
                  label: "信息投递",
                  status: {
                    label: detail.tracks.projectionDelivery.label,
                    tone: detail.tracks.projectionDelivery.tone,
                    description: detail.tracks.projectionDelivery.description,
                  },
                },
                {
                  id: "mall-confirm",
                  label: "商城确认",
                  status: {
                    label: detail.tracks.mallConfirm.label,
                    tone: detail.tracks.mallConfirm.tone,
                    description: detail.tracks.mallConfirm.description,
                  },
                },
              ]}
            />

            <Tabs value={objectTab} onValueChange={setObjectTab}>
              <TabsList>
                <TabsTrigger value="overview">概览</TabsTrigger>
                <TabsTrigger value="content">执行内容</TabsTrigger>
                <TabsTrigger value="history">投递历史</TabsTrigger>
                <TabsTrigger value="versions">版本对应</TabsTrigger>
                <TabsTrigger value="diff">差异与错误</TabsTrigger>
              </TabsList>
            </Tabs>

            {objectTab === "overview" ? (
              <DocumentSummary
                columns="two"
                items={[
                  {
                    id: "source-ver",
                    label: "来源销售版本",
                    value: `v${detail.selectedRevision.salesOrderRevisionNo}`,
                    numeric: true,
                  },
                  {
                    id: "proj-ver",
                    label: "数据版本",
                    value: `v${detail.selectedRevision.revisionNo}`,
                    numeric: true,
                  },
                  {
                    id: "source",
                    label: "数据来源",
                    value:
                      SOURCE_LABEL[detail.selectedRevision.projectionSource],
                  },
                  {
                    id: "acked",
                    label: "商城已确认版",
                    value:
                      detail.currentAckedRevisionNo != null
                        ? `v${detail.currentAckedRevisionNo}`
                        : "尚未确认",
                    numeric: true,
                  },
                  {
                    id: "latency",
                    label: "等待 / SLA",
                    value: `${detail.pendingDurationLabel} · ${LATENCY_LABEL[detail.latencyBand]}`,
                  },
                  {
                    id: "owner",
                    label: "责任",
                    value: detail.ownerLabel,
                  },
                ]}
              />
            ) : null}

            {objectTab === "content" ? (
              <DocumentSection
                title="执行内容（服务端白名单）"
                description="字段仅来自数据修订；前端不重组装。不含成交金额、配赠、税率、开票、应收、玩法规则。"
              >
                <WhitelistContentGrid
                  content={detail.selectedRevision.content}
                />
              </DocumentSection>
            ) : null}

            {objectTab === "history" ? (
              <DocumentSection title="投递历史">
                <div className="overflow-x-auto rounded-xl border">
                  <table className="w-full text-sm">
                    <thead className="bg-muted/50 text-left text-xs text-muted-foreground">
                      <tr>
                        <th className="px-3 py-2">状态</th>
                        <th className="px-3 py-2">尝试</th>
                        <th className="px-3 py-2">最近</th>
                        <th className="px-3 py-2">下次</th>
                        <th className="px-3 py-2">确认</th>
                        <th className="px-3 py-2">摘要</th>
                      </tr>
                    </thead>
                    <tbody>
                      {detail.deliveries.map((d) => (
                        <tr key={d.deliveryId} className="border-t">
                          <td className="px-3 py-2">
                            <BusinessStatusBadge
                              context="list"
                              label={d.statusLabel}
                              tone={d.statusTone}
                            />
                          </td>
                          <td className="num px-3 py-2">{d.attemptCount}</td>
                          <td className="num px-3 py-2">
                            {d.lastAttemptAt ?? "—"}
                          </td>
                          <td className="num px-3 py-2">
                            {d.nextAttemptAt ?? "—"}
                          </td>
                          <td className="num px-3 py-2">
                            {d.mallAckAt ?? "—"}
                          </td>
                          <td className="px-3 py-2 text-xs text-muted-foreground">
                            {d.errorSummary ??
                              d.mallExecutionBaseline ??
                              "—"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </DocumentSection>
            ) : null}

            {objectTab === "versions" ? (
              <DocumentSection
                title="版本对应"
                description="历史数据固定显示来源销售版本，不被销售单当前版覆盖。"
              >
                <RevisionTimeline
                  revisions={detail.revisionLinks.map((link) => ({
                    id: link.projectionRevisionId,
                    version: link.projectionRevisionNo,
                    source:
                      detail.selectedRevision.projectionSource ===
                      "MIGRATION_BASELINE"
                        ? ("migration-baseline" as const)
                        : ("erp-change" as const),
                    actor: "系统",
                    effectiveAt: {
                      dateTime: link.mallAckAt ?? "2026-08-01T00:00:00+08:00",
                      label: link.mallAckAt
                        ? `确认 ${link.mallAckAt}`
                        : "尚未确认",
                    },
                    isCurrent: link.isCurrentSelection,
                    status: {
                      label: link.deliveryStatusLabel,
                      tone:
                        link.deliveryStatus === "ACKED"
                          ? ("success" as const)
                          : link.deliveryStatus === "FAILED"
                            ? ("destructive" as const)
                            : ("neutral" as const),
                    },
                    reason: (
                      <span>
                        来源销售版本 v{link.sourceSalesRevisionNo}
                        （历史不被销售单当前版覆盖）
                        {link.isCurrentSelection ? " · 当前查看" : ""}
                      </span>
                    ),
                    action: (
                      <Button
                        type="button"
                        size="xs"
                        variant="outline"
                        onClick={() =>
                          replaceParams({
                            projectionId: detail.identity.projectionId,
                            revision: link.projectionRevisionId,
                          })
                        }
                      >
                        查看此修订
                      </Button>
                    ),
                  }))}
                />
                {detail.revisionLinks.some(
                  (l) =>
                    l.sourceSalesRevisionNo !==
                    detail.selectedRevision.salesOrderRevisionNo
                ) ? (
                  <p className="mt-2 text-xs text-muted-foreground">
                    提示：销售单当前版本可能高于历史数据来源版本；历史内容不被覆盖。
                  </p>
                ) : null}
              </DocumentSection>
            ) : null}

            {objectTab === "diff" ? (
              <DocumentSection title="差异与错误">
                {detail.reconciliationStatus === "VERSION_MISMATCH" ? (
                  <Alert variant="warning" className="mb-3">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>版本对账差异</AlertTitle>
                    <AlertDescription>
                      {RECONCILIATION_LABEL.VERSION_MISMATCH}
                      ：只打开接口错误中心 核对，不在本页选择覆盖任一侧记录。
                      <div className="mt-2">
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          render={
                            <Link
                              href={w29Href(
                                detail.deliveries[0]?.workItemId,
                                detail.deliveries[0]?.errorTaskId
                              )}
                            />
                          }
                        >
                          打开接口错误差异任务
                        </Button>
                      </div>
                    </AlertDescription>
                  </Alert>
                ) : (
                  <p className="text-sm text-muted-foreground">
                    当前无版本对账差异。
                  </p>
                )}
                {detail.deliveries[0]?.errorSummary ? (
                  <div className="rounded-xl border p-3 text-sm">
                    <div className="font-medium">失败摘要（脱敏）</div>
                    <p className="mt-1 text-muted-foreground">
                      {detail.deliveries[0].errorCode
                        ? `${detail.deliveries[0].errorCode} · `
                        : ""}
                      {detail.deliveries[0].errorSummary}
                    </p>
                    {detail.deliveries[0].workItemId ? (
                      <div className="mt-2 flex flex-wrap items-center gap-2">
                        <Badge variant="secondary">
                          workItem {detail.deliveries[0].workItemId}
                        </Badge>
                        <Button
                          type="button"
                          size="xs"
                          variant="outline"
                          render={
                            <Link
                              href={w29Href(
                                detail.deliveries[0].workItemId,
                                detail.deliveries[0].errorTaskId
                              )}
                            />
                          }
                        >
                          <ExternalLinkIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                          />
                          在接口错误中心处理
                        </Button>
                      </div>
                    ) : null}
                  </div>
                ) : null}
                <p className="mt-3 text-xs text-muted-foreground">
                  W23 不提供领取、续租、转交、关闭或完成任务动作。
                </p>
              </DocumentSection>
            ) : null}
          </div>
        )}
      </QuickPreviewSheet>

      {/* 行预览 */}
      <QuickPreviewSheet
        open={Boolean(previewId)}
        onOpenChange={(open) => {
          if (!open) setPreviewId(null)
        }}
        title="数据预览"
        description="白名单字段、来源版本与最新投递（只读）"
      >
        {previewQuery.data ? (
          <div className="space-y-3">
            <StatusTrackSummary
              tracks={[
                {
                  id: "s",
                  label: "销售记录",
                  status: {
                    label: previewQuery.data.tracks.salesFact.label,
                    tone: previewQuery.data.tracks.salesFact.tone,
                  },
                },
                {
                  id: "d",
                  label: "信息投递",
                  status: {
                    label: previewQuery.data.tracks.projectionDelivery.label,
                    tone: previewQuery.data.tracks.projectionDelivery.tone,
                  },
                },
                {
                  id: "m",
                  label: "商城确认",
                  status: {
                    label: previewQuery.data.tracks.mallConfirm.label,
                    tone: previewQuery.data.tracks.mallConfirm.tone,
                  },
                },
              ]}
            />
            <Separator />
            <WhitelistContentGrid
              content={previewQuery.data.selectedRevision.content}
            />
            <Button
              type="button"
              size="sm"
              onClick={() => {
                const id = previewId
                setPreviewId(null)
                if (id) replaceParams({ projectionId: id })
              }}
            >
              打开对象中心
            </Button>
          </div>
        ) : (
          <div className="h-32 animate-pulse rounded-xl bg-muted" />
        )}
      </QuickPreviewSheet>

      <FormalActionConfirmDialog
        open={pendingAction != null}
        onOpenChange={(open) => {
          if (!open) setPendingAction(null)
        }}
        title={
          pendingAction?.kind === "QUERY_RESULT"
            ? "查询最终结果"
            : pendingAction?.kind === "RETRY"
              ? "重试投递"
              : pendingAction?.kind === "ESCALATE"
                ? "升级到接口错误中心"
                : pendingAction?.kind === "BULK_QUERY"
                  ? "批量查询结果"
                  : pendingAction?.kind === "BULK_RETRY"
                    ? "批量重试"
                    : "确认操作"
        }
        actionLabel="执行"
        confirmLabel="确认执行"
        fromStatus={
          pendingAction &&
          "row" in pendingAction &&
          pendingAction.row
            ? {
                label: pendingAction.row.delivery.statusLabel,
                tone: pendingAction.row.delivery.statusTone,
              }
            : { label: "当前选择", tone: "neutral" }
        }
        toStatus={
          pendingAction?.kind === "QUERY_RESULT"
            ? { label: "明确结果或仍未知", tone: "warning" }
            : pendingAction?.kind === "RETRY" ||
                pendingAction?.kind === "BULK_RETRY"
              ? { label: "按原任务号重试", tone: "info" }
              : pendingAction?.kind === "ESCALATE"
                ? { label: "W29 待办", tone: "warning" }
                : { label: "后台逐项处理", tone: "info" }
        }
        lockedFields={
          pendingAction && "row" in pendingAction && pendingAction.row
            ? [
                `销售版本 v${pendingAction.row.salesOrderRevisionNo}`,
                `数据修订 v${pendingAction.row.projectionRevisionNo}`,
                pendingAction.row.targetMallName,
                `任务号 ${pendingAction.row.salesOrderNo}+v${pendingAction.row.salesOrderRevisionNo}+${pendingAction.row.targetMallName}`,
              ]
            : pendingAction && "ids" in pendingAction
              ? [
                  `显式选择 ${pendingAction.ids.length} 项`,
                  "服务端筛选结果（非当前筛选全部）",
                ]
              : []
        }
        effects={
          pendingAction?.kind === "QUERY_RESULT"
            ? [
                "未明确前不显示成功",
                "不跳过、不计入已确认指标",
                "超时可再次查询或升级到接口错误中心",
              ]
            : pendingAction?.kind === "RETRY"
              ? [
                  "沿原数据修订继续投递",
                  "不生成新数据修订",
                  "不回退销售记录或应收",
                ]
              : pendingAction?.kind === "ESCALATE"
                ? [
                    "创建或复用接口错误待办（不会重复建单）",
                    "W23 只返回入口，不领取/完成任务",
                  ]
                : pendingAction?.kind === "BULK_RETRY"
                  ? [
                      "服务端冻结筛选结果并逐项重验",
                      "已确认/结果未知/权限变化项跳过",
                      "展示成功/跳过/失败/仍未知",
                    ]
                  : [
                      "服务端筛选结果逐项查询",
                      "仍未知不按成功处理",
                    ]
        }
        nextDepartment={
          pendingAction?.kind === "ESCALATE" ? "W29 错误中心" : "运营 / 系统"
        }
        pending={commandMutation.isPending || bulkMutation.isPending}
        onConfirm={async () => {
          const action = pendingAction
          if (!action) return
          if (action.kind === "BULK_QUERY" || action.kind === "BULK_RETRY") {
            await runBulk(action.kind)
            return
          }
          await openConfirmForRow(
            action.kind,
            action.row,
            action.objectVersion
          )
        }}
      />
    </div>
  )
}

function WhitelistContentGrid({
  content,
}: {
  content: {
    customerExternalIdentity: string
    customerExternalIdentityCopyable: boolean
    voucherCategoryExternalIdentity: string
    voucherCategoryErpName: string
    voucherExpiryAt: string
    faceValue: string
    cardCount: string
    cardForm: string
    effectiveAt: string
    contentHash: string
  }
}) {
  return (
    <dl
      className={cn(
        "grid gap-3 sm:grid-cols-2",
        "rounded-xl border border-border bg-card p-3 text-sm"
      )}
    >
      <div>
        <dt className="text-xs text-muted-foreground">商城客户引用</dt>
        <dd className="num font-medium">
          {content.customerExternalIdentity}
          {!content.customerExternalIdentityCopyable ? (
            <span className="ml-2 text-xs font-normal text-muted-foreground">
              短引用·不可复制完整值
            </span>
          ) : null}
        </dd>
      </div>
      <div>
        <dt className="text-xs text-muted-foreground">商城卡券类目</dt>
        <dd>
          {content.voucherCategoryErpName}
          <span className="ml-2 num text-xs text-muted-foreground">
            {content.voucherCategoryExternalIdentity}
          </span>
        </dd>
      </div>
      <div>
        <dt className="text-xs text-muted-foreground">履约期限</dt>
        <dd className="num">{content.voucherExpiryAt}</dd>
      </div>
      <div>
        <dt className="text-xs text-muted-foreground">面额（执行字段）</dt>
        <dd className="num">{content.faceValue}</dd>
      </div>
      <div>
        <dt className="text-xs text-muted-foreground">数量</dt>
        <dd className="num">{content.cardCount}</dd>
      </div>
      <div>
        <dt className="text-xs text-muted-foreground">卡形态</dt>
        <dd>{content.cardForm}</dd>
      </div>
      <div>
        <dt className="text-xs text-muted-foreground">ERP 生效时间</dt>
        <dd className="num">{content.effectiveAt}</dd>
      </div>
      <div>
        <dt className="text-xs text-muted-foreground">数据版本</dt>
        <dd className="num text-xs">{shortHash(content.contentHash)}</dd>
      </div>
    </dl>
  )
}
