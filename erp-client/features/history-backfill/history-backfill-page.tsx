"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  ArrowLeftIcon,
  DownloadIcon,
  ExternalLinkIcon,
  RefreshCwIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  BackgroundJobProgress,
  BusinessEmptyState,
  BusinessStatusBadge,
  BusinessTableFrame,
  CostCoverageNotice,
  DataFreshness,
  DataTable,
  DocumentHeader,
  FormalActionConfirmDialog,
  FormalActionResult,
  ImportStageIndicator,
  ListToolbar,
  MetricItem,
  MetricStrip,
  OptionCombobox,
  PageHeader,
  type ImportStageStates,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { listMallOptions } from "@/features/history-backfill/api"
import {
  useHistoryBackfillCommandMutation,
  useHistoryBackfillDemoControls,
  useHistoryBackfillDetailQuery,
  useHistoryBackfillListQuery,
  useQueryHistoryBackfillIdempotencyMutation,
} from "@/features/history-backfill/queries"
import type {
  BackfillPipelineStage,
  CostBasis,
  CreateBackfillContext,
  HistoryBackfillCommandResult,
  HistoryBackfillEnvironment,
  HistoryBackfillItemView,
  HistoryBackfillListItem,
  HistoryBackfillProcessingStatus,
  HistoryBackfillReportReviewStatus,
  HistoryBackfillView,
  ItemResult,
  JobSection,
  MallOrderFactType,
  ViewerRoleDemo,
} from "@/features/history-backfill/types"
import {
  COST_BASIS_LABEL,
  ENVIRONMENT_LABEL,
  FACT_TYPE_LABEL,
  ITEM_RESULT_LABEL,
  ITEM_RESULT_TONE,
  PIPELINE_ORDER,
  PIPELINE_STAGE_LABEL,
  PIPELINE_TO_INDICATOR,
  PROCESSING_STATUS_LABEL,
  PROCESSING_STATUS_TONE,
  REPORT_REVIEW_STATUS_LABEL,
  REPORT_REVIEW_STATUS_TONE,
  ROLE_LABEL,
  VIEW_LABEL,
} from "@/features/history-backfill/types"
import {
  buildHistoryBackfillSearchParams,
  parseHistoryBackfillSearchParams,
  type HistoryBackfillUrlState,
} from "@/features/history-backfill/url-state"
import { resultText } from "@/lib/ui-text"

const SECTION_TABS: { id: JobSection; label: string }[] = [
  { id: "overview", label: "概览" },
  { id: "facts", label: "记录结果" },
  { id: "dedupe", label: "去重" },
  { id: "unattributed", label: "未归集" },
  { id: "cost", label: "成本口径" },
  { id: "failures", label: "失败诊断" },
  { id: "report", label: "审计报告" },
]

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

function formatDay(iso: string) {
  return iso.slice(0, 10)
}

function newRequestId(prefix: string) {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`
}

function buildStageStates(current: BackfillPipelineStage): ImportStageStates {
  const currentIdx = PIPELINE_ORDER.indexOf(current)
  const states: {
    [K in import("@/components/business").ImportStageKey]: {
      status: "pending" | "current" | "complete" | "failed"
      description?: string
    }
  } = {
    upload: { status: "pending", description: PIPELINE_STAGE_LABEL.SCOPE },
    mapping: {
      status: "pending",
      description: PIPELINE_STAGE_LABEL.VALIDATE_SOURCE,
    },
    validation: { status: "pending", description: PIPELINE_STAGE_LABEL.INGEST },
    preview: {
      status: "pending",
      description: PIPELINE_STAGE_LABEL.ATTRIBUTE,
    },
    submission: { status: "pending", description: PIPELINE_STAGE_LABEL.REPORT },
    result: { status: "pending", description: PIPELINE_STAGE_LABEL.DONE },
  }
  for (let i = 0; i < PIPELINE_ORDER.length; i += 1) {
    const stage = PIPELINE_ORDER[i]!
    const key = PIPELINE_TO_INDICATOR[stage]
    let status: "pending" | "current" | "complete" | "failed" = "pending"
    if (i < currentIdx) status = "complete"
    else if (i === currentIdx) status = "current"
    states[key] = { status, description: PIPELINE_STAGE_LABEL[stage] }
  }
  return states
}

function mapJobProgressStatus(
  processing: HistoryBackfillProcessingStatus
): "queued" | "running" | "succeeded" | "partial" | "failed" {
  if (processing === "RUNNING" || processing === "VALIDATING") return "running"
  if (processing === "COMPLETED") return "succeeded"
  if (processing === "PARTIAL") return "partial"
  if (processing === "FAILED") return "failed"
  return "queued"
}

function FormalResultBanner({
  result,
  onQuery,
  querying,
}: {
  result: HistoryBackfillCommandResult | null
  onQuery?: () => void
  querying?: boolean
}) {
  if (!result) return null
  const status =
    result.status === "COMMITTED"
      ? "succeeded"
      : result.status === "BLOCKED"
        ? "blocked"
        : result.status === "RESULT_UNKNOWN"
          ? "unknown"
          : "rejected"
  return (
    <FormalActionResult
      status={status}
      title={result.title}
      description={result.description}
      facts={[
        { label: "操作 ID", value: result.operationId },
        { label: resultText.originalTaskNo, value: result.idempotencyKey },
        ...(result.jobNo
          ? [{ label: "任务号", value: result.jobNo }]
          : []),
        ...(result.nextStep
          ? [{ label: "下一步", value: result.nextStep }]
          : []),
      ]}
      actions={
        result.status === "RESULT_UNKNOWN" && onQuery ? (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={querying}
            onClick={onQuery}
          >
            查询原操作结果
          </Button>
        ) : undefined
      }
    />
  )
}

function RoleDemoBar({
  role,
  onChange,
}: {
  role: ViewerRoleDemo
  onChange: (r: ViewerRoleDemo) => void
}) {
  return (
    <div className="flex flex-wrap items-center gap-2 rounded-xl border bg-muted/40 px-3 py-2 text-sm">
      <span className="text-muted-foreground">演示角色</span>
      <OptionCombobox
        value={role}
        onValueChange={(v) => {
          if (v == null) return
          onChange(v as ViewerRoleDemo)
        }}
        options={(Object.keys(ROLE_LABEL) as ViewerRoleDemo[]).map((r) => ({
          value: r,
          label: ROLE_LABEL[r],
        }))}
        className="w-[12rem]"
        size="sm"
        allowClear={false}
      />
    </div>
  )
}

function Fact({
  label,
  value,
  mono,
}: {
  label: string
  value: React.ReactNode
  mono?: boolean
}) {
  return (
    <div className="space-y-0.5">
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={mono ? "num font-mono text-sm" : "text-sm"}>{value}</div>
    </div>
  )
}

export function HistoryBackfillPage({
  routeJobId,
}: {
  /** 来自 `/governance/history-backfill/:jobId` */
  routeJobId?: string
}) {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const urlState = React.useMemo(
    () => parseHistoryBackfillSearchParams(searchParams),
    [searchParams]
  )

  const jobId = routeJobId ?? urlState.jobId
  const role: ViewerRoleDemo = urlState.role ?? "SYSTEM_ADMIN"

  const listPath = "/governance/history-backfill"

  const replaceListUrl = React.useCallback(
    (next: HistoryBackfillUrlState) => {
      const qs = buildHistoryBackfillSearchParams(
        { ...next, jobId: undefined },
        { omitJobId: true }
      )
      router.replace(`${listPath}${qs}`, { scroll: false })
    },
    [router]
  )

  const replaceDetailUrl = React.useCallback(
    (id: string, next: HistoryBackfillUrlState) => {
      const qs = buildHistoryBackfillSearchParams(
        { ...next, jobId: undefined },
        { omitJobId: true }
      )
      router.replace(`${listPath}/${id}${qs}`, { scroll: false })
    },
    [router]
  )

  const patchUrl = React.useCallback(
    (patch: Partial<HistoryBackfillUrlState>) => {
      const next = { ...urlState, ...patch }
      if (jobId) replaceDetailUrl(jobId, next)
      else replaceListUrl(next)
    },
    [urlState, jobId, replaceDetailUrl, replaceListUrl]
  )

  if (jobId) {
    return (
      <JobDetailView
        jobId={jobId}
        urlState={urlState}
        role={role}
        patchUrl={patchUrl}
        onBack={() => {
          replaceListUrl({ ...urlState, jobId: undefined, section: "overview" })
        }}
        onOpenJob={(id) => replaceDetailUrl(id, { ...urlState, section: "overview" })}
      />
    )
  }

  return (
    <JobListView
      urlState={urlState}
      role={role}
      patchUrl={patchUrl}
      onOpenJob={(id) => replaceDetailUrl(id, { ...urlState, section: "overview" })}
      pathname={pathname}
    />
  )
}

function JobListView({
  urlState,
  role,
  patchUrl,
  onOpenJob,
}: {
  urlState: HistoryBackfillUrlState
  role: ViewerRoleDemo
  patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
  onOpenJob: (id: string) => void
  pathname: string
}) {
  const [qDraft, setQDraft] = React.useState(urlState.q ?? "")
  const [createOpen, setCreateOpen] = React.useState(false)
  const [actionResult, setActionResult] =
    React.useState<HistoryBackfillCommandResult | null>(null)
  const demo = useHistoryBackfillDemoControls()
  const commandMutation = useHistoryBackfillCommandMutation()
  const queryIdem = useQueryHistoryBackfillIdempotencyMutation()

  const listQuery = useHistoryBackfillListQuery({
    view: urlState.view,
    mallId: urlState.mallId,
    environment: urlState.environment,
    processingStatus: urlState.processingStatus,
    reportReviewStatus: urlState.reportReviewStatus,
    basis: urlState.basis,
    q: urlState.q,
    page: urlState.page,
    pageSize: 20,
    role,
  })

  const data = listQuery.data
  const malls = React.useMemo(() => listMallOptions(), [])

  const columns = React.useMemo<ColumnDef<HistoryBackfillListItem>[]>(
    () => [
      {
        id: "jobNo",
        header: "任务号",
        cell: ({ row }) => (
          <Button
            variant="link"
            className="h-auto p-0 font-mono text-sm"
            onClick={() => onOpenJob(row.original.id)}
          >
            {row.original.jobNo}
          </Button>
        ),
      },
      {
        id: "mall",
        header: "商城",
        cell: ({ row }) => (
          <div className="space-y-0.5">
            <div className="text-sm">{row.original.mallName}</div>
            <Badge
              variant={
                row.original.environment === "production"
                  ? "destructive"
                  : "secondary"
              }
              className="text-[10px]"
            >
              {ENVIRONMENT_LABEL[row.original.environment]}
            </Badge>
          </div>
        ),
      },
      {
        id: "range",
        header: "范围 [start,T)",
        cell: ({ row }) => (
          <span className="num font-mono text-xs">
            {row.original.rangeLabel}
          </span>
        ),
      },
      {
        id: "processing",
        header: "处理状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={PROCESSING_STATUS_LABEL[row.original.processingStatus]}
            tone={PROCESSING_STATUS_TONE[row.original.processingStatus]}
          />
        ),
      },
      {
        id: "reportReview",
        header: "报告确认",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={REPORT_REVIEW_STATUS_LABEL[row.original.reportReviewStatus]}
            tone={REPORT_REVIEW_STATUS_TONE[row.original.reportReviewStatus]}
          />
        ),
      },
      {
        id: "progress",
        header: "进度",
        cell: ({ row }) => (
          <span className="num text-sm">{row.original.progressLabel}</span>
        ),
      },
      {
        id: "dedupe",
        header: "去重",
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.deduplicatedCount.toLocaleString("zh-CN")}
          </span>
        ),
      },
      {
        id: "unattr",
        header: "未归集",
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.unattributedCount.toLocaleString("zh-CN")}
          </span>
        ),
      },
      {
        id: "cost",
        header: "成本覆盖",
        cell: ({ row }) => (
          <span className="text-xs">{row.original.costCoverageLabel}</span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        cell: ({ row }) => (
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => onOpenJob(row.original.id)}
          >
            打开
          </Button>
        ),
      },
    ],
    [onOpenJob]
  )

  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: Math.max(0, urlState.page - 1),
    pageSize: 20,
  })

  React.useEffect(() => {
    setPagination((p) => ({
      ...p,
      pageIndex: Math.max(0, urlState.page - 1),
    }))
  }, [urlState.page])

  if (role === "NO_MODULE") {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="历史消费回填"
          description="管理 [requiredHistoryStart, T) 历史回填任务。"
        />
        <BusinessEmptyState
          kind="no-scope"
          title="无模块权限"
          description="当前角色看不到历史消费回填入口数据。"
        />
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="历史消费回填"
        breadcrumbs={[
          {
            id: "gov",
            label: "治理",
            href: "/governance/history-backfill",
            current: false,
          },
          { id: "hb", label: "历史消费回填", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={
              data?.queriedAt ? formatTime(data.queriedAt) : "刚刚"
            }
            dateTime={data?.queriedAt ?? new Date().toISOString()}
            state={listQuery.isFetching ? "stale" : "fresh"}
            label="回填任务"
          />
        }
        actions={
          <Button
            type="button"
            className="max-sm:hidden"
            onClick={() => setCreateOpen(true)}
          >
            创建回填任务
          </Button>
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <RoleDemoBar
          role={role}
          onChange={(r) =>
            patchUrl({ role: r === "SYSTEM_ADMIN" ? undefined : r })
          }
        />
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => void demo.setCreateContextMode("ok")}
        >
          演示·覆盖完整
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => void demo.setCreateContextMode("gap")}
        >
          演示·覆盖缺口
        </Button>
      </div>

      <FormalResultBanner
        result={actionResult}
        querying={queryIdem.isPending}
        onQuery={() => {
          if (!actionResult?.idempotencyKey) return
          void queryIdem
            .mutateAsync({ idempotencyKey: actionResult.idempotencyKey })
            .then((r) => {
              if (r) setActionResult(r)
            })
        }}
      />

      <Tabs
        value={urlState.view}
        onValueChange={(v) => {
          if (v == null) return
          patchUrl({ view: v as HistoryBackfillView, page: 1 })
        }}
      >
        <TabsList>
          {(Object.keys(VIEW_LABEL) as HistoryBackfillView[]).map((v) => (
            <TabsTrigger key={v} value={v}>
              {VIEW_LABEL[v]}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      <MetricStrip columns={5} aria-label="回填任务指标">
        <MetricItem label="执行中" value={data?.metrics.running ?? "—"} />
        <MetricItem label="待归集" value={data?.metrics.unattributed ?? "—"} />
        <MetricItem
          label="重叠去重"
          value={data?.metrics.deduplicated ?? "—"}
        />
        <MetricItem
          label="NONE 消费"
          value={data?.metrics.noneConsumption ?? "—"}
        />
        <MetricItem label="失败项" value={data?.metrics.failed ?? "—"} />
      </MetricStrip>

      <ListToolbar
        filters={
          <div className="flex flex-wrap items-end gap-2">
            <div className="space-y-1">
              <Label className="text-xs">商城</Label>
              <OptionCombobox
                value={urlState.mallId ?? "all"}
                onValueChange={(v) => {
                  if (v == null) return
                  patchUrl({
                    mallId: v === "all" ? undefined : v,
                    page: 1,
                  })
                }}
                options={[
                  { value: "all", label: "全部商城" },
                  ...malls.map((m) => ({ value: m.id, label: m.name })),
                ]}
                className="w-[10rem]"
                size="sm"
                placeholder="全部商城"
                allowClear={false}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">环境</Label>
              <OptionCombobox
                value={urlState.environment ?? "all"}
                onValueChange={(v) => {
                  if (v == null) return
                  patchUrl({
                    environment:
                      v === "all"
                        ? undefined
                        : (v as HistoryBackfillEnvironment),
                    page: 1,
                  })
                }}
                options={[
                  { value: "all", label: "全部环境" },
                  { value: "production", label: "生产环境" },
                  { value: "verification", label: "验证环境" },
                ]}
                className="w-[9rem]"
                size="sm"
                placeholder="全部环境"
                allowClear={false}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">处理状态</Label>
              <OptionCombobox
                value={urlState.processingStatus ?? "all"}
                onValueChange={(v) => {
                  if (v == null) return
                  patchUrl({
                    processingStatus:
                      v === "all"
                        ? undefined
                        : (v as HistoryBackfillProcessingStatus),
                    page: 1,
                  })
                }}
                options={[
                  { value: "all", label: "全部处理状态" },
                  ...(
                    Object.keys(
                      PROCESSING_STATUS_LABEL
                    ) as HistoryBackfillProcessingStatus[]
                  ).map((s) => ({
                    value: s,
                    label: PROCESSING_STATUS_LABEL[s],
                  })),
                ]}
                className="w-[11rem]"
                size="sm"
                placeholder="全部"
                allowClear={false}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">报告确认</Label>
              <OptionCombobox
                value={urlState.reportReviewStatus ?? "all"}
                onValueChange={(v) => {
                  if (v == null) return
                  patchUrl({
                    reportReviewStatus:
                      v === "all"
                        ? undefined
                        : (v as HistoryBackfillReportReviewStatus),
                    page: 1,
                  })
                }}
                options={[
                  { value: "all", label: "全部确认状态" },
                  ...(
                    Object.keys(
                      REPORT_REVIEW_STATUS_LABEL
                    ) as HistoryBackfillReportReviewStatus[]
                  ).map((s) => ({
                    value: s,
                    label: REPORT_REVIEW_STATUS_LABEL[s],
                  })),
                ]}
                className="w-[11rem]"
                size="sm"
                placeholder="全部"
                allowClear={false}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">成本口径</Label>
              <OptionCombobox
                value={urlState.basis ?? "all"}
                onValueChange={(v) => {
                  if (v == null) return
                  patchUrl({
                    basis: v === "all" ? undefined : (v as CostBasis),
                    page: 1,
                  })
                }}
                options={[
                  { value: "all", label: "全部口径" },
                  { value: "ACTUAL", label: "ACTUAL" },
                  { value: "STANDARD", label: "STANDARD" },
                  { value: "NONE", label: "NONE" },
                ]}
                className="w-[10rem]"
                size="sm"
                placeholder="全部"
                allowClear={false}
              />
            </div>
            <div className="space-y-1">
              <Label className="text-xs">搜索</Label>
              <form
                className="flex gap-1"
                onSubmit={(e) => {
                  e.preventDefault()
                  patchUrl({ q: qDraft || undefined, page: 1 })
                }}
              >
                <Input
                  className="h-8 w-[12rem]"
                  value={qDraft}
                  onChange={(e) => setQDraft(e.target.value)}
                  placeholder="任务号 / 商城"
                />
                <Button type="submit" size="sm" variant="secondary">
                  搜索
                </Button>
              </form>
            </div>
          </div>
        }
      />

      <Alert>
        <ShieldAlertIcon />
        <AlertTitle>范围与敏感边界</AlertTitle>
        <AlertDescription>
          半开区间 [rangeStart, T)，occurredAt = T 不进历史回填。技术处理完成 ≠
          报告已确认 ≠ 全历史业务完成。页面与导出不含卡号、卡密、绑定手机、完整地址或原始消息内容。
        </AlertDescription>
      </Alert>

      {listQuery.isError ? (
        <BusinessEmptyState
          kind="no-data"
          title="任务列表加载失败"
          description="请重试。不会自行补造任务。"
          action={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重试
            </Button>
          }
        />
      ) : (
        <BusinessTableFrame
          title="回填任务"
          description={`共 ${data?.totalCount ?? 0} 个任务 · processingStatus 与 reportReviewStatus 分列`}
          table={
            <DataTable
              data={[...(data?.rows ?? [])]}
              columns={columns}
              getRowId={(row) => row.id}
              rowCount={data?.totalCount ?? 0}
              pagination={pagination}
              onPaginationChange={(next) => {
                setPagination(next)
                patchUrl({ page: next.pageIndex + 1 })
              }}
              layout="flush"
              density="compact"
              loading={listQuery.isPending}
            />
          }
        />
      )}

      <CreateBackfillSheet
        open={createOpen}
        onOpenChange={setCreateOpen}
        context={data?.createContext}
        pending={commandMutation.isPending}
        role={role}
        onSubmit={async () => {
          const ctx = data?.createContext
          if (!ctx) return
          const operationId = newRequestId("op")
          const idempotencyKey = newRequestId("idem_create")
          const result = await commandMutation.mutateAsync({
            action: "CREATE_DRAFT",
            cutoverId: ctx.cutoverId,
            rangeStart: ctx.requiredHistoryStart,
            rangeEnd: ctx.rangeEnd,
            operationId,
            idempotencyKey,
            role,
          })
          setActionResult(result)
          if (result.status === "COMMITTED" && result.jobId) {
            setCreateOpen(false)
            onOpenJob(result.jobId)
          }
        }}
      />
    </div>
  )
}

function CreateBackfillSheet({
  open,
  onOpenChange,
  context,
  pending,
  role,
  onSubmit,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  context?: CreateBackfillContext
  pending: boolean
  role: ViewerRoleDemo
  onSubmit: () => Promise<void>
}) {
  const blocked = !context?.canCreateDraft || role !== "SYSTEM_ADMIN"
  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" size="detail" className="overflow-y-auto">
        <SheetHeader>
          <SheetTitle>创建回填任务</SheetTitle>
          <SheetDescription>
            回填起点固定为系统登记的必需历史起点，不可晚于该日期。回填范围覆盖起点至当前日期。
          </SheetDescription>
        </SheetHeader>

        {!context ? (
          <p className="text-sm text-muted-foreground">正在加载创建上下文…</p>
        ) : (
          <div className="mt-4 space-y-4">
            <div className="grid gap-3 sm:grid-cols-2">
              <Fact label="商城" value={context.mallName} />
              <Fact
                label="环境"
                value={ENVIRONMENT_LABEL[context.environment]}
              />
              <Fact
                label="必需历史起点"
                value={formatDay(context.requiredHistoryStart)}
                mono
              />
              <Fact
                label="回填起点（固定）"
                value={formatDay(context.requiredHistoryStart)}
                mono
              />
              <Fact
                label="消费回流启用日 / 回填终点"
                value={formatDay(context.rangeEnd)}
                mono
              />
              <Fact
                label="来源可提供起点"
                value={formatDay(context.sourceCoverageStart)}
                mono
              />
              <Fact
                label="预计记录数"
                value={context.estimatedFactCount.toLocaleString("zh-CN")}
              />
              <Fact
                label="来源覆盖"
                value={context.coverageComplete ? "完整" : "不足 · 阻断"}
              />
            </div>

            <Alert>
              <TriangleAlertIcon />
              <AlertTitle>T 前支付只补台账</AlertTitle>
              <AlertDescription>
                履约链固定 LEGACY_MANUAL，不创建供应商订单。occurredAt = T
                不在回填范围内。
              </AlertDescription>
            </Alert>

            {context.coverageGaps.length > 0 ? (
              <Alert variant="destructive">
                <AlertTitle>覆盖缺口 · 禁止开始回填</AlertTitle>
                <AlertDescription>
                  <ul className="mt-1 list-disc space-y-1 pl-4">
                    {context.coverageGaps.map((g) => (
                      <li key={`${g.from}-${g.to}`}>
                        {formatDay(g.from)} → {formatDay(g.to)} · {g.reasonLabel}
                      </li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            ) : null}

            {context.blockReasons.length > 0 ? (
              <Alert variant="destructive">
                <AlertTitle>创建阻断</AlertTitle>
                <AlertDescription>
                  <ul className="mt-1 list-disc space-y-1 pl-4">
                    {context.blockReasons.map((r) => (
                      <li key={r}>{r}</li>
                    ))}
                  </ul>
                </AlertDescription>
              </Alert>
            ) : null}

            {context.hasOverlappingFormalJob ? (
              <Alert variant="destructive">
                <AlertTitle>禁止重叠业务批次</AlertTitle>
                <AlertDescription>
                  已存在回填任务 {context.overlappingJobNo}
                  。修复只能续跑原任务，不能新建覆盖同一 [rangeStart, T) 的批次。
                </AlertDescription>
              </Alert>
            ) : null}
          </div>
        )}

        <SheetFooter className="mt-6">
          <Button
            type="button"
            variant="secondary"
            onClick={() => onOpenChange(false)}
          >
            取消
          </Button>
          <Button
            type="button"
            disabled={blocked || pending || !context}
            onClick={() => void onSubmit()}
          >
            {pending ? "提交中…" : "创建任务草稿"}
          </Button>
        </SheetFooter>
      </SheetContent>
    </Sheet>
  )
}

function JobDetailView({
  jobId,
  urlState,
  role,
  patchUrl,
  onBack,
}: {
  jobId: string
  urlState: HistoryBackfillUrlState
  role: ViewerRoleDemo
  patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
  onBack: () => void
  onOpenJob: (id: string) => void
}) {
  const [actionResult, setActionResult] =
    React.useState<HistoryBackfillCommandResult | null>(null)
  const [startOpen, setStartOpen] = React.useState(false)
  const [resumeOpen, setResumeOpen] = React.useState(false)
  const [downloadNote, setDownloadNote] = React.useState<string | null>(null)
  const demo = useHistoryBackfillDemoControls()
  const commandMutation = useHistoryBackfillCommandMutation()
  const queryIdem = useQueryHistoryBackfillIdempotencyMutation()

  const results = urlState.result ? [urlState.result] : undefined
  const factTypes = urlState.factType ? [urlState.factType] : undefined
  const costBases = urlState.costBasis ? [urlState.costBasis] : undefined

  const detailQuery = useHistoryBackfillDetailQuery({
    jobId,
    results,
    factTypes,
    costBases,
    q: urlState.q,
    page: 1,
    pageSize: 100,
    role,
    section: urlState.section,
  })

  const view = detailQuery.data
  const job = view?.job
  const section = urlState.section

  if (detailQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
        <div className="h-24 animate-pulse rounded-2xl bg-muted" />
        <div className="h-40 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (detailQuery.isError || !job) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessEmptyState
          kind="no-data"
          title="任务不存在或无权查看"
          description="请返回列表或检查任务身份。"
          action={
            <Button type="button" variant="secondary" onClick={onBack}>
              返回列表
            </Button>
          }
        />
      </div>
    )
  }

  // 收窄后固定引用，供 async 闭包安全使用
  const currentJob = job

  const stageStates = buildStageStates(currentJob.pipelineStage)
  const progressStatus = mapJobProgressStatus(currentJob.processingStatus)
  const noneRow = currentJob.costBasis.find((c) => c.basis === "NONE")
  const canStart = currentJob.allowedActions.includes("START")
  const canResume = currentJob.allowedActions.includes("RESUME")
  const canValidate = currentJob.allowedActions.includes("VALIDATE_SOURCE")
  const startBlockers = currentJob.actionBlockers.filter((b) => b.action === "START")
  const report = view?.report

  const primaryProcessing = {
    label: PROCESSING_STATUS_LABEL[currentJob.processingStatus],
    tone: PROCESSING_STATUS_TONE[currentJob.processingStatus],
  }

  const filteredItems = view?.items ?? []
  const sectionItems = (() => {
    if (section === "dedupe")
      return filteredItems.filter((i) => i.result === "DEDUPLICATED")
    if (section === "unattributed")
      return filteredItems.filter((i) => i.result === "UNATTRIBUTED")
    if (section === "failures")
      return filteredItems.filter((i) => i.result === "FAILED")
    if (section === "facts") return filteredItems
    return filteredItems
  })()

  const dominantBasis: CostBasis =
    ([...currentJob.costBasis].sort((a, b) => b.count - a.count)[0]?.basis as CostBasis) ??
    "NONE"

  const coverageState =
    currentJob.coveragePercent >= 99
      ? "complete"
      : currentJob.coveragePercent <= 0
        ? "none"
        : "partial"

  async function runCommand(
    action: "VALIDATE_SOURCE" | "START" | "RESUME" | "REATTRIBUTE"
  ) {
    const operationId = newRequestId("op")
    const idempotencyKey =
      action === "RESUME"
        ? `${currentJob.idempotencyNamespace}:resume:${currentJob.lockVersion}`
        : newRequestId(`idem_${action.toLowerCase()}`)
    const result = await commandMutation.mutateAsync({
      action,
      jobId: currentJob.id,
      expectedLockVersion: currentJob.lockVersion,
      rangeStart: currentJob.rangeStart,
      rangeEnd: currentJob.rangeEnd,
      operationId,
      idempotencyKey,
      role,
    })
    setActionResult(result)
    if (action === "START") setStartOpen(false)
    if (action === "RESUME") setResumeOpen(false)
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          {
            id: "gov",
            label: "治理",
            href: "/governance/history-backfill",
          },
          {
            id: "hb",
            label: "历史消费回填",
            href: "/governance/history-backfill",
          },
          {
            id: "job",
            label: currentJob.jobNo,
            current: true,
          },
        ]}
        actions={
          <div className="flex flex-wrap gap-2">
            <Button type="button" variant="outline" size="sm" onClick={onBack}>
              <ArrowLeftIcon className="size-4" />
              返回任务列表
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => void detailQuery.refetch()}
            >
              <RefreshCwIcon className="size-4" />
              刷新
            </Button>
          </div>
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <RoleDemoBar
          role={role}
          onChange={(r) =>
            patchUrl({ role: r === "SYSTEM_ADMIN" ? undefined : r })
          }
        />
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => demo.setForceUnknown(true)}
        >
          下次动作·结果未知
        </Button>
      </div>

      <DocumentHeader
        density="compact"
        title={currentJob.mallName}
        documentNumber={currentJob.jobNo}
        primaryStatus={primaryProcessing}
        version={`lv-${currentJob.lockVersion}`}
        meta={
          <span className="text-muted-foreground">
            {ENVIRONMENT_LABEL[currentJob.environment]} · [
            {formatDay(currentJob.rangeStart)}, {formatDay(currentJob.rangeEnd)})
          </span>
        }
        statuses={[
          {
            id: "report",
            label: "报告确认",
            status: {
              label: REPORT_REVIEW_STATUS_LABEL[currentJob.reportReviewStatus],
              tone: REPORT_REVIEW_STATUS_TONE[currentJob.reportReviewStatus],
            },
          },
          {
            id: "mall",
            label: "商城",
            status: {
              label: `${currentJob.mallName} · ${ENVIRONMENT_LABEL[currentJob.environment]}`,
              tone:
                currentJob.environment === "production" ? "destructive" : "info",
            },
          },
          {
            id: "range",
            label: "范围",
            status: {
              label: `[${formatDay(currentJob.rangeStart)}, ${formatDay(currentJob.rangeEnd)})`,
              tone: "neutral",
            },
          },
          {
            id: "downstream",
            label: "下游功能",
            status: {
              label: currentJob.formalDownstreamUnlocked ? "已解锁" : "关闭",
              tone: currentJob.formalDownstreamUnlocked ? "success" : "warning",
            },
          },
        ]}
        secondaryActions={
          <>
            {canValidate ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={commandMutation.isPending}
                onClick={() => void runCommand("VALIDATE_SOURCE")}
              >
                校验来源
              </Button>
            ) : null}
            {canStart ? (
              <Button
                type="button"
                size="sm"
                onClick={() => setStartOpen(true)}
              >
                开始回填
              </Button>
            ) : null}
            {canResume ? (
              <Button
                type="button"
                size="sm"
                onClick={() => setResumeOpen(true)}
              >
                续跑原任务
              </Button>
            ) : null}
            {report ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  setDownloadNote(
                    `${report.downloadLabel} · ${report.reportId} v${report.reportVersion} · 已记录审计`
                  )
                }}
              >
                <DownloadIcon className="size-4" />
                下载报告
              </Button>
            ) : null}
          </>
        }
      />

      {currentJob.processingStatus === "COMPLETED" &&
      currentJob.reportReviewStatus !== "CONFIRMED" ? (
        <Alert>
          <TriangleAlertIcon />
          <AlertTitle>
            技术处理完成 ≠ 报告已确认 / 全历史业务完成
          </AlertTitle>
          <AlertDescription>
            processingStatus=COMPLETED 仅表示技术处理完成。当前报告确认状态为「
            {REPORT_REVIEW_STATUS_LABEL[currentJob.reportReviewStatus]}
            」。下游功能门禁：
            {currentJob.formalDownstreamUnlocked ? "已解锁" : "保持关闭"}。
          </AlertDescription>
        </Alert>
      ) : null}

      {!currentJob.coverageComplete ? (
        <Alert variant="destructive">
          <AlertTitle>全历史覆盖不足 · 阻断执行</AlertTitle>
          <AlertDescription>
            requiredHistoryStart={formatDay(currentJob.requiredHistoryStart)}，
            sourceCoverageStart=
            {currentJob.sourceCoverageStart
              ? formatDay(currentJob.sourceCoverageStart)
              : "—"}
            。不得把较晚时间改成 rangeStart 后宣称全历史完成。
            <ul className="mt-2 list-disc pl-4">
              {currentJob.coverageGaps.map((g) => (
                <li key={`${g.from}-${g.to}`}>
                  {formatDay(g.from)} → {formatDay(g.to)} · {g.reasonLabel}
                </li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      ) : null}

      <FormalResultBanner
        result={actionResult}
        querying={queryIdem.isPending}
        onQuery={() => {
          if (!actionResult?.idempotencyKey) return
          void queryIdem
            .mutateAsync({ idempotencyKey: actionResult.idempotencyKey })
            .then((r) => {
              if (r) setActionResult(r)
            })
        }}
      />

      {downloadNote ? (
        <Alert>
          <DownloadIcon />
          <AlertTitle>下载结果</AlertTitle>
          <AlertDescription>{downloadNote}</AlertDescription>
        </Alert>
      ) : null}

      <ImportStageIndicator
        stages={stageStates}
        aria-label="回填处理阶段"
      />

      <BackgroundJobProgress
        mode="partialAllowed"
        status={progressStatus}
        total={currentJob.progress.totalCount}
        completed={currentJob.progress.processedCount}
        succeeded={currentJob.progress.insertedCount}
        skipped={currentJob.progress.deduplicatedCount}
        failed={currentJob.progress.failedCount + currentJob.progress.unattributedCount}
        label={`后台回填进度 · ${currentJob.jobNo}`}
        description={
          <>
            后台执行，不伪装同步完成。已处理 {currentJob.progress.processedCount.toLocaleString("zh-CN")}{" "}
            · 新增 {currentJob.progress.insertedCount.toLocaleString("zh-CN")} · 去重{" "}
            {currentJob.progress.deduplicatedCount.toLocaleString("zh-CN")} · 待归集{" "}
            {currentJob.progress.unattributedCount.toLocaleString("zh-CN")} · 失败{" "}
            {currentJob.progress.failedCount.toLocaleString("zh-CN")}
            {currentJob.progress.heartbeatAt
              ? ` · 最近心跳 ${formatTime(currentJob.progress.heartbeatAt)}`
              : ""}
          </>
        }
      />

      <CostCoverageNotice
        basis={dominantBasis}
        coveragePercent={currentJob.coveragePercent}
        coverageLabel={currentJob.coverageRate ?? "—"}
        coverageState={coverageState}
        breakdown={{
          ACTUAL:
            currentJob.costBasis.find((c) => c.basis === "ACTUAL")
              ?.consumptionAmountGross ?? "—",
          STANDARD:
            currentJob.costBasis.find((c) => c.basis === "STANDARD")
              ?.consumptionAmountGross ?? "—",
          NONE:
            noneRow && noneRow.count > 0
              ? `${noneRow.consumptionAmountGross} · 成本空（非 0）`
              : "—",
        }}
        profitBasis="回填成本按逐笔记录：商城成本记录 → 消费时点供给版本 → NONE；禁止当前供给价"
        notice={
          <span>
            NONE 成本字段为空而非 0，仅进入消费金额与覆盖率分母。STANDARD
            必须命中消费发生时点版本。
          </span>
        }
      />

      <div className="grid gap-3 rounded-2xl border bg-card p-4 sm:grid-cols-2 lg:grid-cols-4">
        <Fact label="发起人" value={currentJob.requestedBy} />
        <Fact label="发起时间" value={formatTime(currentJob.requestedAt)} />
        <Fact label="来来源更新时间" value={formatTime(currentJob.sourceAsOf)} />
        <Fact
          label={resultText.originalTaskId}
          value={currentJob.idempotencyNamespace}
          mono
        />
        <Fact label="范围说明" value={currentJob.scopeNote} />
        <Fact label="履约说明" value={currentJob.legacyManualNote} />
      </div>

      {startBlockers.length > 0 && !canStart ? (
        <Alert>
          <AlertTitle>处理动作阻断</AlertTitle>
          <AlertDescription>
            <ul className="list-disc pl-4">
              {startBlockers.map((b) => (
                <li key={b.code}>
                  {b.code}：{b.message}
                </li>
              ))}
            </ul>
          </AlertDescription>
        </Alert>
      ) : null}

      <Tabs
        value={section}
        onValueChange={(v) => {
          if (v == null) return
          patchUrl({ section: v as JobSection })
        }}
      >
        <TabsList className="flex h-auto flex-wrap">
          {SECTION_TABS.map((t) => (
            <TabsTrigger key={t.id} value={t.id}>
              {t.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>

      {(section === "facts" ||
        section === "dedupe" ||
        section === "unattributed" ||
        section === "failures") && (
        <ItemFilters
          urlState={urlState}
          patchUrl={patchUrl}
          section={section}
        />
      )}

      {section === "overview" ? (
        <OverviewSection job={currentJob} items={filteredItems} />
      ) : null}

      {section === "facts" ||
      section === "dedupe" ||
      section === "unattributed" ||
      section === "failures" ? (
        <ItemsTable
          items={sectionItems}
          section={section}
          loading={detailQuery.isFetching}
        />
      ) : null}

      {section === "cost" ? (
        <CostSection job={currentJob} items={filteredItems} />
      ) : null}

      {section === "report" ? (
        <ReportSection
          job={currentJob}
          report={report}
          onDownload={() => {
            if (!report) return
            setDownloadNote(
              `${report.downloadLabel} · Schema ${report.schemaVersion} · 规则 ${report.ruleVersion}`
            )
          }}
        />
      ) : null}

      <FormalActionConfirmDialog
        open={startOpen}
        onOpenChange={setStartOpen}
        actionLabel="开始回填"
        title="确认开始历史回填"
        description="将锁定回填范围并创建后台任务，只补充缺失记录；回填起点前的支付不计入；范围创建后不可修改。"
        fromStatus={{
          label: PROCESSING_STATUS_LABEL[currentJob.processingStatus],
          tone: PROCESSING_STATUS_TONE[currentJob.processingStatus],
        }}
        toStatus={{ label: "运行中", tone: "info" }}
        lockedFields={[
          `rangeStart = requiredHistoryStart = ${formatDay(currentJob.requiredHistoryStart)}`,
          `rangeEnd = T = ${formatDay(currentJob.rangeEnd)}`,
          `商城 ${currentJob.mallName} · ${ENVIRONMENT_LABEL[currentJob.environment]}`,
        ]}
        effects={[
          "后台执行五类关键记录回填",
          "与实时记录按业务记录键去重",
          "成本按 ACTUAL / 时点 STANDARD / NONE 评估",
        ]}
        irreversibleEffects={[
          "已成功写入的业务记录不因失败或续跑回滚",
          "范围冻结后不可修改",
        ]}
        pending={commandMutation.isPending}
        onConfirm={() => runCommand("START")}
      />

      <FormalActionConfirmDialog
        open={resumeOpen}
        onOpenChange={setResumeOpen}
        actionLabel="续跑原任务"
        title="确认续跑失败/中断任务"
        description="沿原任务、原范围与原任务标识续跑，不新建重叠业务批次。"
        fromStatus={{
          label: PROCESSING_STATUS_LABEL[currentJob.processingStatus],
          tone: PROCESSING_STATUS_TONE[currentJob.processingStatus],
        }}
        toStatus={{ label: "运行中", tone: "info" }}
        lockedFields={[
          `任务 ${currentJob.jobNo}`,
          `范围 [${formatDay(currentJob.rangeStart)}, ${formatDay(currentJob.rangeEnd)})`,
          `原任务标识 ${currentJob.idempotencyNamespace}`,
          `已成功 ${currentJob.progress.insertedCount} · 待处理剩余项`,
        ]}
        effects={["逐项仍使用相同业务记录键", "已成功记录保持不变"]}
        irreversibleEffects={["不删除已入库记录"]}
        pending={commandMutation.isPending}
        onConfirm={() => runCommand("RESUME")}
      />
    </div>
  )
}

function ItemFilters({
  urlState,
  patchUrl,
  section,
}: {
  urlState: HistoryBackfillUrlState
  patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
  section: JobSection
}) {
  return (
    <div className="flex flex-wrap items-end gap-2 rounded-xl border bg-muted/30 p-3">
      {section === "facts" ? (
        <>
          <div className="space-y-1">
            <Label className="text-xs">结果</Label>
            <OptionCombobox
              value={urlState.result ?? "all"}
              onValueChange={(v) => {
                if (v == null) return
                patchUrl({
                  result: v === "all" ? undefined : (v as ItemResult),
                })
              }}
              options={[
                { value: "all", label: "全部结果" },
                ...(Object.keys(ITEM_RESULT_LABEL) as ItemResult[]).map(
                  (r) => ({
                    value: r,
                    label: ITEM_RESULT_LABEL[r],
                  })
                ),
              ]}
              className="w-[10rem]"
              size="sm"
              allowClear={false}
            />
          </div>
          <div className="space-y-1">
            <Label className="text-xs">记录类型</Label>
            <OptionCombobox
              value={urlState.factType ?? "all"}
              onValueChange={(v) => {
                if (v == null) return
                patchUrl({
                  factType:
                    v === "all" ? undefined : (v as MallOrderFactType),
                })
              }}
              options={[
                { value: "all", label: "全部五类" },
                ...(Object.keys(FACT_TYPE_LABEL) as MallOrderFactType[]).map(
                  (t) => ({
                    value: t,
                    label: FACT_TYPE_LABEL[t],
                  })
                ),
              ]}
              className="w-[12rem]"
              size="sm"
              allowClear={false}
            />
          </div>
          <div className="space-y-1">
            <Label className="text-xs">成本口径</Label>
            <OptionCombobox
              value={urlState.costBasis ?? "all"}
              onValueChange={(v) => {
                if (v == null) return
                patchUrl({
                  costBasis: v === "all" ? undefined : (v as CostBasis),
                })
              }}
              options={[
                { value: "all", label: "全部" },
                { value: "ACTUAL", label: "ACTUAL" },
                { value: "STANDARD", label: "STANDARD" },
                { value: "NONE", label: "NONE" },
              ]}
              className="w-[9rem]"
              size="sm"
              allowClear={false}
            />
          </div>
        </>
      ) : null}
      <p className="text-xs text-muted-foreground">
        URL：result / factType / costBasis · 五类记录与多次退款/恢复不合并
      </p>
    </div>
  )
}

function OverviewSection({
  job,
  items,
}: {
  job: NonNullable<
    Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
  >["job"]
  items: HistoryBackfillItemView[]
}) {
  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card>
        <CardHeader>
          <CardTitle>任务身份与范围</CardTitle>
          <CardDescription>范围起点固定等于必须覆盖起点</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2">
          <Fact label="切换编号" value={job.cutoverId} mono />
          <Fact
            label="必须覆盖起点"
            value={formatDay(job.requiredHistoryStart)}
            mono
          />
          <Fact label="范围起点" value={formatDay(job.rangeStart)} mono />
          <Fact label="截止时点" value={formatDay(job.rangeEnd)} mono />
          <Fact
            label="覆盖完整"
            value={job.coverageComplete ? "是" : "否"}
          />
          <Fact
            label="阶段"
            value={PIPELINE_STAGE_LABEL[job.pipelineStage]}
          />
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>结果记录</CardTitle>
          <CardDescription>统计由系统统一计算，与明细列表可能因分页存在差异。</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2">
          <Fact
            label="来源记录数"
            value={job.progress.totalCount.toLocaleString("zh-CN")}
          />
          <Fact
            label="已处理"
            value={job.progress.processedCount.toLocaleString("zh-CN")}
          />
          <Fact
            label="新增"
            value={job.progress.insertedCount.toLocaleString("zh-CN")}
          />
          <Fact
            label="去重"
            value={job.progress.deduplicatedCount.toLocaleString("zh-CN")}
          />
          <Fact
            label="待归集"
            value={job.progress.unattributedCount.toLocaleString("zh-CN")}
          />
          <Fact
            label="失败"
            value={job.progress.failedCount.toLocaleString("zh-CN")}
          />
          <Fact label="当前页明细" value={`${items.length} 条`} />
        </CardContent>
      </Card>
    </div>
  )
}

function ItemsTable({
  items,
  section,
  loading,
}: {
  items: HistoryBackfillItemView[]
  section: JobSection
  loading?: boolean
}) {
  const columns = React.useMemo<ColumnDef<HistoryBackfillItemView>[]>(
    () => [
      {
        id: "factType",
        header: "记录类型",
        cell: ({ row }) => (
          <span className="text-sm">{FACT_TYPE_LABEL[row.original.factType]}</span>
        ),
      },
      {
        id: "key",
        header: "业务记录键摘要",
        cell: ({ row }) => (
          <span className="font-mono text-xs">
            {row.original.businessFactKeySummary}
          </span>
        ),
      },
      {
        id: "order",
        header: "商城订单",
        cell: ({ row }) => (
          <div className="space-y-0.5">
            <div className="font-mono text-xs">{row.original.mallOrderNo}</div>
            {row.original.sourceDocNo ? (
              <div className="text-[11px] text-muted-foreground">
                子单 {row.original.sourceDocNo}
              </div>
            ) : null}
          </div>
        ),
      },
      {
        id: "occurred",
        header: "发生时间",
        cell: ({ row }) => (
          <span className="num text-xs">
            {formatTime(row.original.occurredAt)}
          </span>
        ),
      },
      {
        id: "result",
        header: "结果",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={ITEM_RESULT_LABEL[row.original.result]}
            tone={ITEM_RESULT_TONE[row.original.result]}
          />
        ),
      },
      {
        id: "cost",
        header: "成本",
        cell: ({ row }) => {
          const b = row.original.costBasis
          if (!b || b === "N_A") return <span className="text-xs">不适用</span>
          if (b === "NONE") {
            return (
              <span className="text-xs text-warning-foreground">
                NONE · 成本空
              </span>
            )
          }
          return (
            <span className="text-xs">
              {b}
              {row.original.costAmountNet
                ? ` · ${row.original.costAmountNet}`
                : ""}
            </span>
          )
        },
      },
      {
        id: "extra",
        header: section === "dedupe" ? "去重证明" : "说明 / 去向",
        cell: ({ row }) => {
          const item = row.original
          if (item.dedupeProof) {
            return (
              <div className="max-w-[16rem] text-xs">
                <div>
                  {item.dedupeProof.matchedSource === "REALTIME"
                    ? "命中实时记录"
                    : "命中原回填记录"}
                </div>
                <div className="text-muted-foreground">
                  {item.dedupeProof.formalFactSummary}
                </div>
                <div className="font-mono text-[10px] text-muted-foreground">
                  {item.dedupeProof.formalFactId} · msg{" "}
                  {item.dedupeProof.originalMessageId}
                </div>
              </div>
            )
          }
          if (item.result === "UNATTRIBUTED") {
            return (
              <div className="space-y-1">
                <div className="text-xs">{item.unattributedReason}</div>
                <Button
                  render={
                    <Link
                      href={`/governance/integration-errors?from=W30&jobId=${item.jobId}&factKey=${encodeURIComponent(item.businessFactKeySummary)}`}
                    />
                  }
                  size="sm"
                  variant="outline"
                  className="h-7 text-xs"
                >
                  去接口错误中心处理
                  <ExternalLinkIcon className="size-3" />
                </Button>
              </div>
            )
          }
          if (item.failure) {
            return (
              <div className="max-w-[14rem] text-xs">
                <div className="font-mono">{item.failure.errorCode}</div>
                <div>{item.failure.summary}</div>
                <div className="text-muted-foreground">
                  {item.failure.stage} ·{" "}
                  {item.failure.retryable ? "可续跑" : "需业务修复"}
                </div>
              </div>
            )
          }
          return (
            <span className="text-xs text-muted-foreground">
              {item.fulfillmentChain === "LEGACY_MANUAL"
                ? "LEGACY_MANUAL"
                : "—"}
            </span>
          )
        },
      },
    ],
    [section]
  )

  if (items.length === 0) {
    return (
      <BusinessEmptyState
        kind="no-data"
        title="当前筛选无明细"
        description="五类关键记录分别保留；同一订单的支付/取消/完成/多次退款/多次余额恢复不会被合并。"
      />
    )
  }

  return (
    <BusinessTableFrame
      title={
        section === "dedupe"
          ? "去重证明"
          : section === "unattributed"
            ? "待归集（原记录已保存）"
            : section === "failures"
              ? "失败诊断"
              : "记录结果"
      }
      description="不含卡号/卡密/手机/完整地址/原始消息内容 · 商城订单号不是唯一任务号"
      table={
        <DataTable
          data={[...items]}
          columns={columns}
          getRowId={(row) => row.itemId}
          rowCount={items.length}
          layout="flush"
          density="compact"
          loading={loading}
        />
      }
    />
  )
}

function CostSection({
  job,
  items,
}: {
  job: NonNullable<
    Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
  >["job"]
  items: HistoryBackfillItemView[]
}) {
  return (
    <div className="space-y-4">
      <div className="grid gap-3 md:grid-cols-3">
        {job.costBasis.map((row) => (
          <Card key={row.basis}>
            <CardHeader className="pb-2">
              <CardTitle className="text-base">
                {COST_BASIS_LABEL[row.basis]}
              </CardTitle>
              <CardDescription>{row.count.toLocaleString("zh-CN")} 笔</CardDescription>
            </CardHeader>
            <CardContent className="space-y-1 text-sm">
              <div>消费金额（含税）：{row.consumptionAmountGross}</div>
              <div>
                成本净额：
                {row.basis === "NONE"
                  ? "空（禁止写 0）"
                  : (row.costAmountNet ?? "—")}
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
      <Alert>
        <AlertTitle>禁止当前供给价</AlertTitle>
        <AlertDescription>
          STANDARD 必须命中消费发生时点有效供给版本；NONE
          不得用当前价、猜测税率或销项税率替代进项。覆盖率{" "}
          {job.coverageRate ?? "—"}（NONE 进分母）。
        </AlertDescription>
      </Alert>
      <Separator />
      <ItemsTable
        items={items.filter(
          (i) => i.costBasis === "ACTUAL" || i.costBasis === "STANDARD" || i.costBasis === "NONE"
        )}
        section="facts"
      />
    </div>
  )
}

function ReportSection({
  job,
  report,
  onDownload,
}: {
  job: NonNullable<
    Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
  >["job"]
  report?: NonNullable<
    Awaited<ReturnType<typeof useHistoryBackfillDetailQuery>>["data"]
  >["report"]
  onDownload: () => void
}) {
  if (!report) {
    return (
      <BusinessEmptyState
        kind="no-data"
        title="技术报告尚未生成"
        description="processingStatus 达到部分完成或技术处理完成后可生成可审计报告。"
      />
    )
  }

  const unconfirmed = report.reviewLabel === "UNCONFIRMED"

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <div className="flex flex-wrap items-start justify-between gap-2">
            <div>
              <CardTitle>审计报告</CardTitle>
              <CardDescription>
                {report.reportId} · v{report.reportVersion} ·{" "}
                {formatTime(report.generatedAt)}
              </CardDescription>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge variant={unconfirmed ? "outline" : "default"}>
                {report.downloadLabel}
              </Badge>
              <BusinessStatusBadge
                context="detail"
                label={PROCESSING_STATUS_LABEL[report.processingStatus]}
                tone={PROCESSING_STATUS_TONE[report.processingStatus]}
              />
              <BusinessStatusBadge
                context="detail"
                label={REPORT_REVIEW_STATUS_LABEL[report.reportReviewStatus]}
                tone={REPORT_REVIEW_STATUS_TONE[report.reportReviewStatus]}
              />
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          {unconfirmed ? (
            <Alert>
              <TriangleAlertIcon />
              <AlertTitle>技术报告 · 未确认</AlertTitle>
              <AlertDescription>
                报告复核策略未配置或报告未确认时，下载固定标「技术报告 ·
                未确认」。确认动作与下游门禁保持关闭；不得仅因技术完成解锁。
              </AlertDescription>
            </Alert>
          ) : null}

          {report.fullHistoryFinalComplete ? (
            <Alert>
              <AlertTitle>全历史回填最终完成</AlertTitle>
              <AlertDescription>
                技术处理完成、来源覆盖完整且报告已确认。
              </AlertDescription>
            </Alert>
          ) : (
            <Alert>
              <AlertTitle>尚未全历史最终完成</AlertTitle>
              <AlertDescription>
                当前不可宣称「全历史回填最终完成」。下游功能：
                {job.formalDownstreamUnlocked ? "已解锁" : "关闭"}。
              </AlertDescription>
            </Alert>
          )}

          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <Fact
              label="范围"
              value={`[${formatDay(report.rangeStart)}, ${formatDay(report.rangeEnd)})`}
              mono
            />
            <Fact label="T" value={formatDay(report.cutoverAt)} mono />
            <Fact
              label="总笔数"
              value={report.totalCount.toLocaleString("zh-CN")}
            />
            <Fact label="总金额" value={report.totalAmount} />
            <Fact
              label="去重"
              value={report.deduplicatedCount.toLocaleString("zh-CN")}
            />
            <Fact label="覆盖率" value={report.coverageRate ?? "—"} />
            <Fact label="Schema" value={report.schemaVersion} mono />
            <Fact label="规则版本" value={report.ruleVersion} mono />
            <Fact label="操作者" value={report.operatorLabel} />
          </div>

          <div className="grid gap-3 md:grid-cols-3">
            {report.costBasis.map((c) => (
              <div
                key={c.basis}
                className="rounded-xl border bg-muted/30 p-3 text-sm"
              >
                <div className="font-medium">{COST_BASIS_LABEL[c.basis]}</div>
                <div>{c.count.toLocaleString("zh-CN")} 笔</div>
                <div>{c.consumptionAmountGross}</div>
                <div className="text-muted-foreground">
                  成本：
                  {c.basis === "NONE"
                    ? "空"
                    : (c.costAmountNet ?? "—")}
                </div>
              </div>
            ))}
          </div>

          <div className="grid gap-4 md:grid-cols-2">
            <div>
              <h4 className="mb-2 text-sm font-medium">未归集清单摘要</h4>
              <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                {report.unattributedSummaries.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ul>
            </div>
            <div>
              <h4 className="mb-2 text-sm font-medium">失败清单摘要</h4>
              <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                {report.failedSummaries.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ul>
            </div>
          </div>

          <p className="text-xs text-muted-foreground">
            {report.sensitiveRedactionNote}
          </p>

          <div className="flex flex-wrap gap-2">
            <Button type="button" onClick={onDownload}>
              <DownloadIcon className="size-4" />
              下载{unconfirmed ? "技术报告 · 未确认" : "已确认报告"}
            </Button>
            {job.reportReviewStatus === "POLICY_NOT_CONFIGURED" ? (
              <Badge variant="outline">确认动作不可用 · 策略未配置</Badge>
            ) : null}
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
