"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  ExternalLinkIcon,
  PauseIcon,
  RefreshCwIcon,
  ShieldAlertIcon,
  TriangleAlertIcon,
} from "lucide-react"
import { z } from "zod"

import {
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionConfirmDialog,
  FormalActionResult,
  MaintenanceBanner,
  MetricFilterItem,
  MetricStrip,
  OptionCombobox,
  PageHeader,
  SequentialProcessBar,
} from "@/components/business"
import { useAppForm } from "@/components/form"
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
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import type {
  MallSnapshotRow,
  MallSyncJobRow,
  MallSyncViewName,
  MappingTaskView,
  ReconciliationDifference,
} from "@/features/mall-sync/types"
import {
  DEFER_REASON_OPTIONS,
  DIRECTION_LABEL,
  STAGE_LABEL,
  VIEW_LABEL,
} from "@/features/mall-sync/types"
import {
  useClaimMappingMutation,
  useConfirmMappingMutation,
  useDeferMappingMutation,
  useMallSyncPageQuery,
  useReapplyMutation,
  useResolveUnknownReapplyMutation,
  useRetryJobMutation,
  useTriggerIncrementalMutation,
  useTriggerSingleOrderMutation,
} from "@/features/mall-sync/queries"
import { SourceSystemsCard } from "@/features/mall-sync/source-systems-card"
import { cn } from "@/lib/utils"
import { formatDateTime } from "@/lib/datetime"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { type ResultState } from "@/components/business/feedback"
import {
  freshnessText,
  versionText,
  workspaceLabel,
} from "@/lib/ui-text"
import {
  JOB_ERROR_CLASS_LABEL,
} from "@/features/mall-sync/types"

const VIEWS: MallSyncViewName[] = [
  "overview",
  "jobs",
  "snapshots",
  "mapping",
  "reconciliation",
  "history",
]

function parseView(raw: string | null): MallSyncViewName {
  if (raw && (VIEWS as string[]).includes(raw)) return raw as MallSyncViewName
  return "overview"
}

type SessionLease = {
  workItemId: string
  subjectVersion: string
}

const confirmSchema = z.object({
  evidenceNote: z.string().trim().min(4, "请填写至少 4 个字的确认依据"),
})

const deferSchema = z.object({
  reasonCode: z.enum([
    "WAITING_SOURCE",
    "NEED_CLARIFICATION",
    "WAITING_MASTER_DATA",
    "OTHER",
  ]),
  note: z.string(),
})

const pullSchema = z.object({
  externalOrderNo: z.string().trim().min(1, "请填写商城销售单号"),
  reason: z.string().trim().min(4, "请填写至少 4 个字的理由"),
})

const incrementalSchema = z.object({
  reason: z.string().trim().min(4, "请填写至少 4 个字的理由"),
})

export function MallSyncPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const view = parseView(searchParams.get("view"))
  const q = searchParams.get("q") ?? ""
  const jobId = searchParams.get("jobId") ?? undefined
  const snapshotId = searchParams.get("snapshotId") ?? undefined
  const mappingTaskId = searchParams.get("mappingTaskId") ?? undefined
  const workItemId =
    searchParams.get("workItemId") ??
    searchParams.get("currentWorkItemId") ??
    undefined
  const differenceId = searchParams.get("differenceId") ?? undefined
  const queueContextId =
    searchParams.get("queueContextId") ?? "queue:W17:mall-sync"

  const [searchInput, setSearchInput] = React.useState(q)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [sessionLease, setSessionLease] = React.useState<SessionLease | null>(
    null
  )
  const [selectedCandidateId, setSelectedCandidateId] = React.useState<
    string | null
  >(null)
  const [result, setResult] = React.useState<ResultState>(null)
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [deferOpen, setDeferOpen] = React.useState(false)
  const [pullOpen, setPullOpen] = React.useState(false)
  const [incrementalOpen, setIncrementalOpen] = React.useState(false)
  const [retryConfirmOpen, setRetryConfirmOpen] = React.useState(false)
  const [actionError, setActionError] = React.useState<string | null>(null)

  const queryInput = React.useMemo(
    () => ({
      view,
      q: q || undefined,
      jobId,
      snapshotId,
      mappingTaskId,
      workItemId,
      differenceId,
      queueContextId,
    }),
    [
      view,
      q,
      jobId,
      snapshotId,
      mappingTaskId,
      workItemId,
      differenceId,
      queueContextId,
    ]
  )

  const pageQuery = useMallSyncPageQuery(queryInput)
  const triggerInc = useTriggerIncrementalMutation()
  const triggerSo = useTriggerSingleOrderMutation()
  const retryJob = useRetryJobMutation()
  const claimMutation = useClaimMappingMutation()
  const confirmMutation = useConfirmMappingMutation()
  const deferMutation = useDeferMappingMutation()
  const reapplyMutation = useReapplyMutation()
  const resolveReapply = useResolveUnknownReapplyMutation()

  const data = pageQuery.data
  const context = data?.context
  const ownership = context?.ownership
  const policyState = context?.manualGovernancePolicy
  const policyMissing = policyState?.state === "MISSING"
  const stage = ownership?.stage ?? "FIRST_PHASE_MALL_OWNED"
  const firstPhase = stage === "FIRST_PHASE_MALL_OWNED"
  const sealed = stage === "SECOND_PHASE_ERP_OWNED"

  const mappingTask = data?.selectedMappingTask
  const mappingIndex = React.useMemo(() => {
    if (!data?.mappingTasks.length || !mappingTask) return { current: 0, total: 0 }
    const idx = data.mappingTasks.findIndex(
      (t) => t.mappingTaskId === mappingTask.mappingTaskId
    )
    return { current: idx >= 0 ? idx + 1 : 1, total: data.mappingTasks.length }
  }, [data?.mappingTasks, mappingTask])

  React.useEffect(() => {
    setSearchInput(q)
  }, [q])

  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchInput === q) return
      patchUrl({ q: searchInput.trim() || null })
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput])

  // 封存后默认引导 history
  React.useEffect(() => {
    if (sealed && view !== "history" && !jobId && !snapshotId && !mappingTaskId) {
      // 不强制 replace 每次；仅当无对象 id 时提示在 banner
    }
  }, [sealed, view, jobId, snapshotId, mappingTaskId])

  // 切换映射任务时重置候选与租约绑定
  React.useEffect(() => {
    setSelectedCandidateId(null)
    setActionError(null)
    if (
      sessionLease &&
      mappingTask?.ownerRoutingState === "CONFIGURED" &&
      sessionLease.workItemId !== mappingTask.workItem.workItemId
    ) {
      setSessionLease(null)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mappingTask?.mappingTaskId])

  function patchUrl(
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean }
  ) {
    patchSearchParams({ router, pathname, searchParams, view }, patch, options)
  }

  const confirmForm = useAppForm({
    defaultValues: { evidenceNote: "" },
    validators: { onChange: confirmSchema },
    onSubmit: async () => {
      setConfirmOpen(true)
    },
  })

  const deferForm = useAppForm({
    defaultValues: {
      reasonCode: "WAITING_SOURCE" as
        | "WAITING_SOURCE"
        | "NEED_CLARIFICATION"
        | "WAITING_MASTER_DATA"
        | "OTHER",
      note: "",
    },
    validators: { onChange: deferSchema },
    onSubmit: async ({ value }) => {
      await handleDefer(value.reasonCode, value.note)
    },
  })

  const pullForm = useAppForm({
    defaultValues: { externalOrderNo: "", reason: "" },
    validators: { onChange: pullSchema },
    onSubmit: async ({ value }) => {
      const res = await triggerSo.mutateAsync({
        externalOrderNo: value.externalOrderNo,
        reason: value.reason,
        policyConfigured: !policyMissing,
        stage,
      })
      if (res.status === "succeeded") {
        setResult({
          status: "succeeded",
          title: "按单补拉已受理",
          description: res.message,
          reference: res.jobNo,
        })
        setPullOpen(false)
        patchUrl({ view: "jobs", jobId: res.jobId })
      } else {
        setActionError(res.message)
      }
    },
  })

  const incrementalForm = useAppForm({
    defaultValues: { reason: "" },
    validators: { onChange: incrementalSchema },
    onSubmit: async ({ value }) => {
      const res = await triggerInc.mutateAsync({
        reason: value.reason,
        policyConfigured: !policyMissing,
        stage,
      })
      if (res.status === "succeeded") {
        setResult({
          status: "succeeded",
          title: "立即增量已受理",
          description: res.message,
          reference: res.jobNo,
        })
        setIncrementalOpen(false)
        patchUrl({ view: "jobs", jobId: res.jobId })
      } else {
        setActionError(res.message)
      }
    },
  })

  async function handleClaim() {
    if (mappingTask?.ownerRoutingState !== "CONFIGURED") return
    setActionError(null)
    try {
      const lease = await claimMutation.mutateAsync({
        workItemId: mappingTask.workItem.workItemId,
        subjectVersion: mappingTask.workItem.subjectVersion,
      })
      setSessionLease({
        workItemId: lease.workItemId,
        subjectVersion: lease.subjectVersion,
      })
    } catch (e) {
      setActionError(e instanceof Error ? e.message : "领取失败")
    }
  }

  async function handleConfirm() {
    if (mappingTask?.ownerRoutingState !== "CONFIGURED" || !sessionLease) return
    const candidate = mappingTask.candidateTargets.find(
      (c) => c.objectId === selectedCandidateId
    )
    if (!candidate || candidate.eligibility !== "ELIGIBLE") {
      setActionError("请选择可用的 ERP 候选（相似不自动确认）")
      return
    }
    const evidenceNote = String(
      confirmForm.getFieldValue("evidenceNote") ?? ""
    ).trim()
    const res = await confirmMutation.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      workItemId: mappingTask.workItem.workItemId,
      expectedSubjectVersion: mappingTask.workItem.subjectVersion,
      expectedLockVersion: mappingTask.lockVersion,
      targetObjectId: candidate.objectId,
      targetLabel: `${candidate.stableNo} ${candidate.label}`,
      evidenceNote,
      stage,
    })
    setConfirmOpen(false)
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "映射已确认",
        description: res.message,
        facts: [
          { label: "已确认目标", value: `${candidate.stableNo} ${candidate.label}` },
        ],
      })
      setSessionLease(null)
      void pageQuery.refetch()
      // 与「先跳过」一致：自动定位到下一项
      const tasks = data?.mappingTasks ?? []
      const idx = tasks.findIndex(
        (t) => t.mappingTaskId === mappingTask.mappingTaskId
      )
      const next = tasks[idx + 1]
      if (next) {
        patchUrl({
          view: "mapping",
          mappingTaskId: next.mappingTaskId,
          workItemId:
            next.ownerRoutingState === "CONFIGURED"
              ? next.workItem.workItemId
              : null,
        })
      }
    } else {
      setActionError(res.message)
    }
  }

  async function handleDefer(reasonCode: string, note?: string) {
    if (mappingTask?.ownerRoutingState !== "CONFIGURED" || !sessionLease) {
      setActionError("请先领取任务")
      return
    }
    const res = await deferMutation.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      workItemId: mappingTask.workItem.workItemId,
      expectedSubjectVersion: mappingTask.workItem.subjectVersion,
      reasonCode,
      note,
      queueContextId,
    })
    setDeferOpen(false)
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "已跳过（任务仍在待处理列表）",
        description: res.message,
      })
      setSessionLease(null)
      // 移动到下一项（仅浏览游标）
      const tasks = data?.mappingTasks ?? []
      const idx = tasks.findIndex(
        (t) => t.mappingTaskId === mappingTask.mappingTaskId
      )
      const next = tasks[idx + 1]
      if (next) {
        patchUrl({
          view: "mapping",
          mappingTaskId: next.mappingTaskId,
          workItemId:
            next.ownerRoutingState === "CONFIGURED"
              ? next.workItem.workItemId
              : null,
        })
      }
    } else {
      setActionError(res.message)
    }
  }

  async function handleReapply() {
    if (!mappingTask) return
    const res = await reapplyMutation.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      sourceSnapshotId: mappingTask.sourceSnapshotId,
      stage,
    })
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "重新归集成功",
        description: res.message,
        reference: res.salesOrderNo,
      })
      void pageQuery.refetch()
    } else if (res.status === "unknown") {
      setResult({
        status: "unknown",
        title: "重新归集结果未知",
        description: res.message,
        stayOnItem: true,
        pendingIdempotencyKey: res.idempotencyKey,
        reference: res.operationId,
      })
      void pageQuery.refetch()
    } else {
      setActionError(res.message)
    }
  }

  async function handleRetryJob() {
    if (!data?.selectedJob) return
    const res = await retryJob.mutateAsync({
      jobId: data.selectedJob.jobId,
      reason: "重试未成功部分的分页",
      stage,
    })
    setRetryConfirmOpen(false)
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "重试已创建",
        description: res.message,
        reference: res.jobNo,
      })
      void pageQuery.refetch()
    } else {
      setActionError(res.message)
    }
  }

  async function handleResolveUnknownReapply() {    if (!mappingTask?.reapplyOperation) return
    const res = await resolveReapply.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      operationId: mappingTask.reapplyOperation.operationId,
      settle: true,
    })
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "重新归集结果已确认",
        description: res.message,
        reference: res.salesOrderNo,
      })
    } else if (res.status === "unknown") {
      setResult({
        status: "unknown",
        title: "仍为结果未知",
        description: res.message,
        stayOnItem: true,
      })
    } else {
      setActionError(res.message)
    }
  }

  const canManualSync =
    firstPhase && !policyMissing && !context?.sourceUnavailable
  const manualSyncDisabledReason = !firstPhase
    ? "已封存：无第一期写动作"
    : policyMissing
      ? "人工治理策略未配置：立即增量/按单补拉已禁用"
      : context?.sourceUnavailable
        ? "来源不可用时不新建推进任务（可重试既有失败）"
        : null

  const jobColumns = React.useMemo<ColumnDef<MallSyncJobRow>[]>(
    () => [
      {
        id: "jobNo",
        accessorFn: (r) => r.jobNo,
        header: "任务号",
        cell: ({ row }) => (
          <button
            type="button"
            className="text-left text-sm font-medium text-primary hover:underline"
            onClick={() =>
              patchUrl({ view: "jobs", jobId: row.original.jobId })
            }
          >
            {row.original.jobNo}
          </button>
        ),
      },
      {
        id: "type",
        accessorFn: (r) => r.jobTypeLabel,
        header: "类型",
        cell: ({ row }) => (
          <span className="text-sm">{row.original.jobTypeLabel}</span>
        ),
      },
      {
        id: "status",
        header: "状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "counts",
        header: "页 / 条 / 错",
        meta: { align: "end", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.pageCount}/{row.original.itemCount}/
            {row.original.errorCount}
          </span>
        ),
      },
      {
        id: "wm",
        header: freshnessText.syncProgress,
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {row.original.watermarkAdvanced ? "已推进" : "未推进"}
          </span>
        ),
      },
      {
        id: "started",
        header: "开始",
        cell: ({ row }) => (
          <span className="text-sm tabular-nums">
            {formatDateTime(row.original.startedAt, "default")}
          </span>
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams]
  )

  const snapshotColumns = React.useMemo<ColumnDef<MallSnapshotRow>[]>(
    () => [
      {
        id: "order",
        header: "商城销售单号",
        cell: ({ row }) => (
          <button
            type="button"
            className="font-mono text-sm text-primary hover:underline"
            onClick={() =>
              patchUrl({
                view: "snapshots",
                snapshotId: row.original.snapshotId,
              })
            }
          >
            {row.original.externalOrderNo}
          </button>
        ),
      },
      {
        id: "status",
        header: "商城状态",
        cell: ({ row }) => (
          <span className="text-sm">{row.original.sourceStatusLabel}</span>
        ),
      },
      {
        id: "mapping",
        header: "数据映射状态",
        cell: ({ row }) => (
          <Badge variant="outline">{row.original.mappingStatusLabel}</Badge>
        ),
      },
      {
        id: "hash",
        header: versionText.dataVersion,
        cell: ({ row }) => (
          <span className="font-mono text-xs text-muted-foreground">
            {row.original.contentHashShort}
          </span>
        ),
      },
      {
        id: "applied",
        header: "ERP 版本",
        cell: ({ row }) =>
          row.original.appliedSalesOrderNo ? (
            <Link
              href={`/sales/orders/${row.original.appliedSalesOrderId}`}
              className="text-sm text-primary hover:underline"
            >
              {row.original.appliedSalesOrderNo}
            </Link>
          ) : (
            <span className="text-sm text-muted-foreground">未形成</span>
          ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams]
  )

  const mappingColumns = React.useMemo<ColumnDef<MappingTaskView>[]>(
    () => [
      {
        id: "order",
        header: "来源单号",
        cell: ({ row }) => (
          <button
            type="button"
            className="font-mono text-sm text-primary hover:underline"
            onClick={() =>
              patchUrl({
                view: "mapping",
                mappingTaskId: row.original.mappingTaskId,
                workItemId:
                  row.original.ownerRoutingState === "CONFIGURED"
                    ? row.original.workItem.workItemId
                    : null,
              })
            }
          >
            {row.original.externalOrderNo}
          </button>
        ),
      },
      {
        id: "type",
        header: "映射类型",
        cell: ({ row }) => (
          <span className="text-sm">{row.original.mappingTypeLabel}</span>
        ),
      },
      {
        id: "mapStatus",
        header: "映射状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.mappingTaskStatusLabel}
            tone={
              row.original.mappingTaskStatus === "RESOLVED"
                ? "success"
                : row.original.mappingTaskStatus === "PENDING"
                  ? "warning"
                  : "neutral"
            }
          />
        ),
      },
      {
        id: "reapply",
        header: "重新归集",
        cell: ({ row }) =>
          row.original.reapplyOperation ? (
            <BusinessStatusBadge
              context="list"
              label={row.original.reapplyOperation.statusLabel}
              tone={
                row.original.reapplyOperation.status === "SUCCEEDED"
                  ? "success"
                  : row.original.reapplyOperation.status === "UNKNOWN"
                    ? "destructive"
                    : "info"
              }
            />
          ) : (
            <span className="text-sm text-muted-foreground">未开始</span>
          ),
      },
      {
        id: "owner",
        header: "责任",
        cell: ({ row }) =>
          row.original.ownerRoutingState === "MISSING" ? (
            <Badge variant="destructive">待责任配置</Badge>
          ) : (
            <span className="text-sm">{row.original.ownerRoleLabel}</span>
          ),
      },
      {
        id: "wi",
        header: "待办",
        cell: ({ row }) =>
          row.original.ownerRoutingState === "CONFIGURED" ? (
            <span className="text-sm">{row.original.workItem.statusLabel}</span>
          ) : (
            <span className="text-sm text-muted-foreground">无</span>
          ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams]
  )

  const diffColumns = React.useMemo<ColumnDef<ReconciliationDifference>[]>(
    () => [
      {
        id: "order",
        header: "来源单号",
        cell: ({ row }) => (
          <button
            type="button"
            className="font-mono text-sm text-primary hover:underline"
            onClick={() =>
              patchUrl({
                view: "reconciliation",
                differenceId: row.original.differenceId,
              })
            }
          >
            {row.original.externalOrderNo}
          </button>
        ),
      },
      {
        id: "type",
        header: "差异类型",
        cell: ({ row }) => (
          <span className="text-sm">{row.original.differenceTypeLabel}</span>
        ),
      },
      {
        id: "fp",
        header: versionText.dataVersion,
        cell: ({ row }) => (
          <span className="font-mono text-xs text-muted-foreground">
            {row.original.sourceFingerprintShort ?? "—"}
            {row.original.erpFingerprintShort
              ? ` ↔ ${row.original.erpFingerprintShort}`
              : ""}
          </span>
        ),
      },
      {
        id: "status",
        header: "状态",
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [searchParams]
  )

  const leaseStatus: "active" | "unclaimed" | "lost" | "released" =
    mappingTask?.ownerRoutingState !== "CONFIGURED"
      ? "released"
      : mappingTask.mappingTaskStatus === "RESOLVED"
        ? "released"
        : sessionLease?.workItemId === mappingTask.workItem.workItemId
          ? "active"
          : "unclaimed"

  const canConfirmMapping =
    mappingTask?.ownerRoutingState === "CONFIGURED" &&
    mappingTask.mappingTaskStatus === "PENDING" &&
    mappingTask.allowedActions.includes("CONFIRM_TARGET") &&
    leaseStatus === "active" &&
    !!selectedCandidateId &&
    !mappingTask.hasConflict

  const pageJobs = React.useMemo(() => {
    const rows = data?.jobs ?? []
    const start = pagination.pageIndex * pagination.pageSize
    return rows.slice(start, start + pagination.pageSize)
  }, [data?.jobs, pagination])

  if (pageQuery.isPending && !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-xl bg-muted" />
        <div className="h-24 animate-pulse rounded-2xl bg-muted" />
        <div className="grid gap-4 lg:grid-cols-2">
          <div className="h-72 animate-pulse rounded-2xl bg-muted" />
          <div className="h-72 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  if (pageQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="商城同步与映射" description="加载失败" />
        <Alert variant="destructive">
          <AlertTitle>查询失败</AlertTitle>
          <AlertDescription>
            {(pageQuery.error as Error)?.message ?? "请重试"}
          </AlertDescription>
        </Alert>
        <Button type="button" onClick={() => void pageQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="商城同步与映射"
        breadcrumbs={[
          { id: "gov", label: "治理", href: "/governance", current: false },
          { id: "sync", label: "商城同步与映射", current: true },
        ]}
        metadata={
          <div className="flex flex-wrap items-center gap-3">
            <DataFreshness
              updatedAt={
                context?.freshness.latestSuccessfulJobAt
                  ? formatDateTime(
                      context.freshness.latestSuccessfulJobAt,
                      "default"
                    )
                  : "—"
              }
              dateTime={context?.freshness.latestSuccessfulJobAt}
              state={context?.sourceUnavailable ? "stale" : "fresh"}
              label="同步数据"
            />
            <Badge variant="outline">
              {context?.sourceSystem.name} · {context?.sourceSystem.environmentLabel}
            </Badge>
          </div>
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!canManualSync}
              title={manualSyncDisabledReason ?? "立即增量（按策略）"}
              onClick={() => {
                setActionError(null)
                setIncrementalOpen(true)
              }}
            >
              立即增量
            </Button>
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={!canManualSync}
              title={manualSyncDisabledReason ?? "按单号补拉"}
              onClick={() => {
                setActionError(null)
                setPullOpen(true)
              }}
            >
              按单补拉
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={() => void pageQuery.refetch()}
            >
              <RefreshCwIcon className="size-4" aria-hidden />
              刷新
            </Button>
          </div>
        }
      />

      {/* OwnershipBanner — 始终可见 */}
      {ownership ? (
        <MaintenanceBanner
          tone={sealed ? "info" : "info"}
          icon={sealed ? ShieldAlertIcon : undefined}
          title={
            sealed
              ? `第一期已封存 · ${DIRECTION_LABEL[ownership.syncDirection]}`
              : `当前主责：${STAGE_LABEL[ownership.stage]} · 方向 ${DIRECTION_LABEL[ownership.syncDirection]}`
          }
          description={
            <div className="space-y-1 text-sm">
              <p>
                <span className="font-medium">商城边界：</span>
                {ownership.mallWriteBoundary}
              </p>
              <p>
                <span className="font-medium">ERP 边界：</span>
                {ownership.erpWriteBoundary}
              </p>
              {ownership.sealedAt ? (
                <p>
                  封存时间 {formatDateTime(ownership.sealedAt, "default")}
                  {ownership.finalWatermark
                    ? ` · 最终同步点 ${formatDateTime(ownership.finalWatermark, "default")}`
                    : ""}
                </p>
              ) : null}
              <p className="text-muted-foreground">
                无「编辑来源数据」「向商城回写商业修改」「手工标记同步成功」入口。
              </p>
            </div>
          }
        />
      ) : null}

      {sealed && (
        <div className="flex flex-wrap gap-2 text-sm">
          <Button variant="link" size="sm" render={<Link href="/commerce/execution-projections" />}>
            {workspaceLabel("W23")}
            <ExternalLinkIcon className="size-3.5" />
          </Button>
          <Button variant="link" size="sm" render={<Link href="/governance/integration-errors" />}>
            {workspaceLabel("W29")}
            <ExternalLinkIcon className="size-3.5" />
          </Button>
          {sealed && view !== "history" ? (
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => patchUrl({ view: "history" })}
            >
              进入历史只读
            </Button>
          ) : null}
        </div>
      )}

      {policyMissing ? (
        <Alert variant="warning">
          <TriangleAlertIcon />
          <AlertTitle>人工同步治理策略未配置</AlertTitle>
          <AlertDescription className="space-y-1">
            <p>
              「立即增量」「按单补拉」已禁用（界面与系统均拒绝）。
            </p>
            <p className="text-muted-foreground">
              {context?.scheduledIncrementalNote}
            </p>
          </AlertDescription>
        </Alert>
      ) : (
        <Alert>
          <AlertTitle>人工同步治理</AlertTitle>
          <AlertDescription>
            策略已配置 · 版本{" "}
            {policyState?.state === "CONFIGURED"
              ? policyState.policyVersion
              : "—"}{" "}
            · 模式{" "}
            {policyState?.state === "CONFIGURED"
              ? policyState.executionMode === "SINGLE_OPERATOR_REASON"
                ? "单人执行"
                : "双人复核"
              : "—"}
            。定时增量仍按调度独立运行。
          </AlertDescription>
        </Alert>
      )}

      {context?.sourceUnavailable ? (
        <Alert variant="destructive">
          <AlertTitle>来源商城不可用</AlertTitle>
          <AlertDescription>
            {context.sourceUnavailableMessage}
          </AlertDescription>
        </Alert>
      ) : null}

      {/* 来源系统列表 */}
      <SourceSystemsCard />

      <MetricStrip
        columns={Math.min(5, Math.max(2, context?.metrics.length ?? 4)) as 2 | 3 | 4 | 5}
        aria-label="商城同步指标"
      >
        {(context?.metrics ?? []).map((m) => (
          <MetricFilterItem
            key={m.key}
            label={m.label}
            value={m.count != null ? m.count : (m.value ?? "—")}
            detail={m.detail}
            active={view === m.targetView}
            onClick={() => {
              patchUrl({
                view: m.targetView,
              })
            }}
          />
        ))}
      </MetricStrip>

      <div className="sticky top-0 z-10 -mx-1 border-b bg-background/95 px-1 py-2 backdrop-blur">
        <Tabs
          value={view}
          onValueChange={(v) => {
            const next = parseView(v)
            patchUrl({
              view: next,
              // 保留对象 id 以便后退恢复；切换时不强制清除
            })
          }}
        >
          <TabsList variant="line" className="w-full justify-start overflow-x-auto">
            {VIEWS.map((v) => (
              <TabsTrigger key={v} value={v}>
                {VIEW_LABEL[v]}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <Input
          className="max-w-xs"
          placeholder={
            view === "snapshots" || view === "mapping"
              ? "商城单号 / ERP 单号 / 任务号"
              : view === "jobs"
                ? "任务号"
                : "搜索仅对来源数据、同步任务与映射任务生效"
          }
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          aria-label="搜索"
        />
        {q ? (
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => {
              setSearchInput("")
              patchUrl({ q: null })
            }}
          >
            清除筛选
          </Button>
        ) : null}
      </div>

      {result ? (
        <FormalActionResult
          status={
            result.status === "succeeded"
              ? "succeeded"
              : result.status === "unknown"
                ? "unknown"
                : result.status === "blocked"
                  ? "blocked"
                  : "rejected"
          }
          title={result.title}
          description={result.description}
          reference={result.reference}
          facts={result.facts}
          actions={
            result.status === "unknown" &&
            mappingTask?.reapplyOperation?.status === "UNKNOWN" ? (
              <Button
                type="button"
                size="sm"
                onClick={() => void handleResolveUnknownReapply()}
              >
                查询重新归集处理结果
              </Button>
            ) : undefined
          }
        />
      ) : null}

      {actionError ? (
        <Alert variant="destructive">
          <AlertTitle>动作失败</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {/* ── 子视图内容 ── */}
      {view === "overview" ? (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card size="sm">
            <CardHeader>
              <CardTitle>运行摘要</CardTitle>
              <CardDescription>
                同步进度仅证明来源数据已捕获，不证明映射或应收已成功。
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground">当前同步进度</span>
                <span className="num text-xs">
                  {context?.freshness.currentWatermark
                    ? formatDateTime(
                        context.freshness.currentWatermark,
                        "default"
                      )
                    : "—"}
                </span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground">最近成功</span>
                <span>{formatDateTime(context?.freshness.latestSuccessfulJobAt, "default")}</span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground">来源数据更新时间</span>
                <span>{formatDateTime(context?.freshness.sourceSafeTime, "default")}</span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground">主责数量</span>
                <span>
                  商城 {ownership?.mallOwnedOrderCount ?? "—"} · ERP{" "}
                  {ownership?.erpOwnedOrderCount ?? "—"}
                </span>
              </div>
              <Separator />
              <p className="text-muted-foreground">
                同步失败不阻塞商城销售/制卡/绑定/激活/消费；差异在 ERP
                侧处理，无「手工补建销售单」入口。
              </p>
            </CardContent>
          </Card>
          <Card size="sm">
            <CardHeader>
              <CardTitle>最近同步任务</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {(data?.jobs ?? []).slice(0, 4).map((job) => (
                <button
                  key={job.jobId}
                  type="button"
                  className="flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-sm hover:bg-accent/50"
                  onClick={() => patchUrl({ view: "jobs", jobId: job.jobId })}
                >
                  <span className="font-medium">{job.jobNo}</span>
                  <BusinessStatusBadge
                    context="list"
                    label={job.statusLabel}
                    tone={job.statusTone}
                  />
                </button>
              ))}
              {(data?.jobs ?? []).length === 0 ? (
                <p className="text-sm text-muted-foreground">暂无同步任务。</p>
              ) : null}
            </CardContent>
          </Card>
        </div>
      ) : null}

      {view === "jobs" ? (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.4fr)_minmax(18rem,1fr)]">
          <BusinessTableFrame
            title="同步任务"
            description="基线 / 增量 / 单号补拉。同步进度不因映射失败回退。"
            table={
              <DataTable
                data={pageJobs}
                columns={jobColumns}
                getRowId={(r) => r.jobId}
                rowCount={data?.jobs.length ?? 0}
                pagination={pagination}
                onPaginationChange={setPagination}
                layout="flush"
                density="compact"
              />
            }
          />
          {data?.selectedJob ? (
            <Card size="sm">
              <CardHeader>
                <CardTitle className="text-base">
                  {data.selectedJob.jobNo}
                </CardTitle>
                <CardDescription>
                  {data.selectedJob.jobTypeLabel} · {data.selectedJob.triggeredBy}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3 text-sm">
                <BusinessStatusBadge
                  context="detail"
                  label={data.selectedJob.statusLabel}
                  tone={data.selectedJob.statusTone}
                />
                {data.selectedJob.impactSummary ? (
                  <p>{data.selectedJob.impactSummary}</p>
                ) : null}
                {data.selectedJob.errorClass ? (
                  <p className="text-muted-foreground">
                    错误分类：
                    {JOB_ERROR_CLASS_LABEL[data.selectedJob.errorClass] ??
                      data.selectedJob.errorClass}
                  </p>
                ) : null}
                <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                  <span>
                    游标前{" "}
                    {data.selectedJob.cursorBefore
                      ? formatDateTime(data.selectedJob.cursorBefore, "default")
                      : "—"}
                  </span>
                  <span>
                    游标后{" "}
                    {data.selectedJob.cursorAfter
                      ? formatDateTime(data.selectedJob.cursorAfter, "default")
                      : "—"}
                  </span>
                </div>
                {data.selectedJob.allowedActions.includes("RETRY_FAILED_JOB") ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    disabled={retryJob.isPending}
                    onClick={() => setRetryConfirmOpen(true)}
                  >
                    重试失败任务
                  </Button>
                ) : null}
                {data.selectedJob.actionBlockers.map((b) => (
                  <p
                    key={b.code}
                    className="text-xs text-amber-700 dark:text-amber-400"
                  >
                    {b.message}
                  </p>
                ))}
              </CardContent>
            </Card>
          ) : null}
        </div>
      ) : null}

      {view === "snapshots" ? (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.3fr)_minmax(18rem,1fr)]">
          <BusinessTableFrame
            title="来源数据"
            description="仅白名单商业字段。不含玩法、卡号、卡密、绑定手机、连接或密钥。"
            table={
              <DataTable
                data={data?.snapshots ?? []}
                columns={snapshotColumns}
                getRowId={(r) => r.snapshotId}
                rowCount={(data?.snapshots ?? []).length}
                layout="flush"
                density="compact"
              />
            }
          />
          {data?.selectedSnapshot ? (
            <Card size="sm">
              <CardHeader>
                <CardTitle className="font-mono text-base">
                  {data.selectedSnapshot.externalOrderNo}
                </CardTitle>
                <CardDescription>
                  {versionText.version}{" "}
                  {data.selectedSnapshot.contentHashShort} · 任务{" "}
                  {data.selectedSnapshot.syncJobNo}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-2">
                <Badge variant="outline">
                  {data.selectedSnapshot.mappingStatusLabel}
                </Badge>
                {data.selectedSnapshot.conflictFlags.length > 0 ? (
                  <Alert variant="warning">
                    <AlertTitle>冲突标记</AlertTitle>
                    <AlertDescription>
                      {data.selectedSnapshot.conflictFlags.join("、")}
                    </AlertDescription>
                  </Alert>
                ) : null}
                <dl className="space-y-1.5 text-sm">
                  {data.selectedSnapshot.whitelistFields.map((f) => (
                    <div
                      key={f.field}
                      className="flex justify-between gap-2 border-b border-dashed border-border/60 py-1"
                    >
                      <dt className="text-muted-foreground">{f.label}</dt>
                      <dd className="text-right font-medium">{f.value}</dd>
                    </div>
                  ))}
                </dl>
              </CardContent>
            </Card>
          ) : (
            <BusinessEmptyState
              kind="no-data"
              title="选择结果"
              description="从左侧列表选择一条记录"
            />
          )}
        </div>
      ) : null}

      {view === "mapping" ? (
        <div className="space-y-4">
          {data?.emptyReason === "NO_TASKS" ||
          data?.emptyReason === "FILTER_NO_RESULT" ? (
            <BusinessEmptyState
              kind={
                data.emptyReason === "FILTER_NO_RESULT"
                  ? "filter"
                  : "no-tasks"
              }
              title={
                data.emptyReason === "NO_TASKS"
                  ? "当前没有待处理映射"
                  : "筛选无结果"
              }
              description={
                data.emptyReason === "FILTER_NO_RESULT"
                  ? "清除筛选后查看其它任务。"
                  : "新任务到达后刷新"
              }
            />
          ) : null}

          <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
            <BusinessTableFrame
              title="映射任务"
              description="映射状态与重新归集状态分列；责任未配置时不可执行。"
              table={
                <DataTable
                  data={data?.mappingTasks ?? []}
                  columns={mappingColumns}
                  getRowId={(r) => r.mappingTaskId}
                  rowCount={(data?.mappingTasks ?? []).length}
                  layout="flush"
                  density="compact"
                />
              }
            />

            {mappingTask ? (
              <div className="space-y-3">
                <Card size="sm">
                  <CardHeader className="space-y-2">
                    <div className="flex flex-wrap items-center gap-2">
                      <CardTitle className="text-base">
                        {mappingTask.mappingTypeLabel}
                      </CardTitle>
                      <BusinessStatusBadge
                        context="detail"
                        label={`映射 · ${mappingTask.mappingTaskStatusLabel}`}
                        tone={
                          mappingTask.mappingTaskStatus === "RESOLVED"
                            ? "success"
                            : "warning"
                        }
                      />
                      {mappingTask.reapplyOperation ? (
                        <BusinessStatusBadge
                          context="detail"
                          label={`归集 · ${mappingTask.reapplyOperation.statusLabel}`}
                          tone={
                            mappingTask.reapplyOperation.status === "UNKNOWN"
                              ? "destructive"
                              : mappingTask.reapplyOperation.status ===
                                  "SUCCEEDED"
                                ? "success"
                                : "info"
                          }
                        />
                      ) : (
                        <Badge variant="outline">归集 · 未开始</Badge>
                      )}
                    </div>
                    <CardDescription>
                      {mappingTask.externalOrderNo}
                      {mappingTask.ownerRoutingState === "CONFIGURED" ? (
                        <>
                          {" "}
                          · 责任 {mappingTask.ownerRoleLabel}
                        </>
                      ) : (
                        " · 待责任配置"
                      )}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    <Alert>
                      <AlertTitle>确认的是身份关系</AlertTitle>
                      <AlertDescription>
                        不是修改来源销售单；相似候选绝不自动确认/合并。
                      </AlertDescription>
                    </Alert>

                    {mappingTask.ownerRoutingState === "MISSING" ? (
                      <Alert variant="destructive">
                        <AlertTitle>责任归属未配置</AlertTitle>
                        <AlertDescription>
                          结算主体责任未配置唯一负责角色；领域差异已保存，确认禁用，不向销售与财务同时生成可完成待办。
                        </AlertDescription>
                      </Alert>
                    ) : null}

                    {mappingTask.hasConflict ? (
                      <Alert variant="warning">
                        <AlertTitle>映射冲突</AlertTitle>
                        <AlertDescription>
                          当前谱系与候选并存。请刷新候选并明确确认依据；冲突未解决前确认禁用。
                        </AlertDescription>
                      </Alert>
                    ) : null}

                    <p className="text-sm">
                      <span className="font-medium">业务影响：</span>
                      {mappingTask.impactSummary}
                    </p>

                    <div className="grid gap-3 md:grid-cols-2">
                      <div>
                        <h4 className="mb-2 text-sm font-semibold">
                          来源白名单记录
                        </h4>
                        <dl className="space-y-1 text-sm">
                          {mappingTask.sourceEvidence.map((e) => (
                            <div
                              key={e.field}
                              className="flex justify-between gap-2 border-b border-dashed py-1"
                            >
                              <dt className="text-muted-foreground">
                                {e.label}
                              </dt>
                              <dd className="text-right">
                                {e.sensitive ? "***" : e.value}
                              </dd>
                            </div>
                          ))}
                        </dl>
                      </div>
                      <div>
                        <h4 className="mb-2 text-sm font-semibold">
                          ERP 候选
                        </h4>
                        <ul className="space-y-2">
                          {mappingTask.candidateTargets.map((c) => (
                            <li key={c.objectId}>
                              <button
                                type="button"
                                disabled={
                                  c.eligibility !== "ELIGIBLE" ||
                                  mappingTask.mappingTaskStatus !== "PENDING" ||
                                  mappingTask.ownerRoutingState === "MISSING"
                                }
                                onClick={() =>
                                  setSelectedCandidateId(c.objectId)
                                }
                                className={cn(
                                  "w-full rounded-lg border px-3 py-2 text-left text-sm transition-colors",
                                  selectedCandidateId === c.objectId
                                    ? "border-primary bg-accent"
                                    : "hover:bg-muted/60",
                                  c.eligibility !== "ELIGIBLE" && "opacity-60"
                                )}
                              >
                                <div className="flex items-center justify-between gap-2">
                                  <span className="font-medium">
                                    {c.stableNo}
                                  </span>
                                  <Badge
                                    variant={
                                      c.eligibility === "ELIGIBLE"
                                        ? "secondary"
                                        : "outline"
                                    }
                                  >
                                    {c.eligibility === "ELIGIBLE"
                                      ? "可选"
                                      : "不可用"}
                                  </Badge>
                                </div>
                                <p>{c.label}</p>
                                <p className="text-xs text-muted-foreground">
                                  {c.reason}
                                </p>
                              </button>
                            </li>
                          ))}
                        </ul>
                      </div>
                    </div>

                    {mappingTask.currentTargets.length > 0 ? (
                      <div>
                        <h4 className="mb-2 text-sm font-semibold">
                          当前谱系
                        </h4>
                        <ul className="space-y-1 text-sm">
                          {mappingTask.currentTargets.map((t) => (
                            <li
                              key={`${t.objectId}-${t.validFrom}`}
                              className="rounded-md border px-2 py-1"
                            >
                              {t.stableNo} {t.label} · {t.relationRole} ·{" "}
                              {t.status}
                              {t.validTo ? ` · 至 ${t.validTo}` : ""}
                            </li>
                          ))}
                        </ul>
                      </div>
                    ) : null}

                    {selectedCandidateId &&
                    mappingTask.mappingTaskStatus === "PENDING" ? (
                      <BusinessDiffPanel
                        title="确认依据对照"
                        changes={[
                          {
                            id: "identity",
                            field: "身份关系",
                            before: "未确认 / 旧谱系",
                            after:
                              mappingTask.candidateTargets.find(
                                (c) => c.objectId === selectedCandidateId
                              )?.label ?? selectedCandidateId,
                            note: "确认后建立身份对应关系，不改动来源单",
                          },
                          {
                            id: "impact",
                            field: "业务影响",
                            before: "未归属",
                            after: "映射解决 → 待重新归集",
                            note: mappingTask.impactSummary,
                          },
                        ]}
                      />
                    ) : null}

                    {mappingTask.resolutionHistory.length > 0 ? (
                      <div>
                        <h4 className="mb-2 text-sm font-semibold">
                          处理历史
                        </h4>
                        <ul className="space-y-1 text-xs text-muted-foreground">
                          {mappingTask.resolutionHistory.map((h, i) => (
                            <li key={`${h.handledAt}-${i}`}>
                              {formatDateTime(h.handledAt, "default")} · {h.action} ·{" "}
                              {h.result} · {h.handledBy}
                            </li>
                          ))}
                        </ul>
                      </div>
                    ) : null}

                    {mappingTask.ownerRoutingState === "CONFIGURED" &&
                    mappingTask.mappingTaskStatus === "PENDING" ? (
                      <form
                        className="space-y-2"
                        onSubmit={(e) => {
                          e.preventDefault()
                          void confirmForm.handleSubmit()
                        }}
                      >
                        <confirmForm.AppField
                          name="evidenceNote"
                          children={(field) => (
                            <field.TextareaField
                              label="确认依据"
                              placeholder="说明选择该 ERP 对象的业务依据"
                            />
                          )}
                        />
                        <div className="flex flex-wrap gap-2">
                          <confirmForm.AppForm>
                            <confirmForm.SubmitButton
                              label="确认映射"
                              disabled={!canConfirmMapping}
                            />
                          </confirmForm.AppForm>
                          <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={leaseStatus !== "active"}
                            onClick={() => setDeferOpen(true)}
                          >
                            <PauseIcon className="size-4" />
                            先跳过
                          </Button>
                        </div>
                        {!selectedCandidateId ? (
                          <p className="text-xs text-muted-foreground">
                            请先选择左侧 ERP 候选后即可确认。
                          </p>
                        ) : mappingTask.hasConflict ? (
                          <p className="text-xs text-muted-foreground">
                            冲突未解决前确认禁用。
                          </p>
                        ) : leaseStatus !== "active" ? (
                          <p className="text-xs text-muted-foreground">
                            请先领取任务后确认。
                          </p>
                        ) : null}
                      </form>
                    ) : null}

                    {mappingTask.mappingTaskStatus === "RESOLVED" ? (
                      <div className="space-y-2 rounded-xl border p-3">
                        <p className="text-sm font-medium">
                          固定下一步：使用原数据重新归集
                        </p>
                        {mappingTask.reapplyOperation?.status === "UNKNOWN" ? (
                          <Alert variant="destructive">
                            <AlertTitle>重新归集结果未知</AlertTitle>
                            <AlertDescription>
                              映射结论保持已解决，不回滚、不自动下一项。
                            </AlertDescription>
                          </Alert>
                        ) : null}
                        {mappingTask.reapplyOperation?.status ===
                        "SUCCEEDED" ? (
                          <p className="text-sm">
                            已形成{" "}
                            <Link
                              className="text-primary hover:underline"
                              href={`/sales/orders/${mappingTask.reapplyOperation.salesOrderId}`}
                            >
                              {mappingTask.reapplyOperation.salesOrderNo}
                            </Link>
                            {mappingTask.reapplyOperation
                              .receivableResultReference
                              ? ` · 应收 ${mappingTask.reapplyOperation.receivableResultReference}`
                              : ""}
                          </p>
                        ) : (
                          <div className="flex flex-wrap gap-2">
                            <Button
                              type="button"
                              size="sm"
                              disabled={reapplyMutation.isPending}
                              onClick={() => void handleReapply()}
                            >
                              重新归集
                            </Button>
                            {mappingTask.reapplyOperation?.status ===
                            "UNKNOWN" ? (
                              <Button
                                type="button"
                                size="sm"
                                variant="secondary"
                                onClick={() => void handleResolveUnknownReapply()}
                              >
                                查询处理结果
                              </Button>
                            ) : null}
                          </div>
                        )}
                      </div>
                    ) : null}

                    {mappingTask.actionBlockers.map((b) => (
                      <p
                        key={`${b.action}-${b.code}`}
                        className="text-xs text-amber-700 dark:text-amber-400"
                      >
                        {b.message}
                      </p>
                    ))}
                  </CardContent>
                </Card>

                {mappingTask.ownerRoutingState === "CONFIGURED" &&
                mappingTask.mappingTaskStatus === "PENDING" ? (
                  <SequentialProcessBar
                    current={mappingIndex.current}
                    total={mappingIndex.total}
                    leaseStatus={leaseStatus}
                    processLabel="确认映射"
                    // 没有独立的「并打开下一条」路径：两个 handler 同义
                    showProcessNext={false}
                    processDisabled={!canConfirmMapping}
                    onBack={() =>
                      router.push(
                        `/workspace/tasks?queueContextId=${encodeURIComponent(queueContextId)}`
                      )
                    }
                    onProcess={() => {
                      if (canConfirmMapping) void confirmForm.handleSubmit()
                    }}
                    onProcessNext={() => {
                      if (canConfirmMapping) void confirmForm.handleSubmit()
                    }}
                    onReclaim={() => void handleClaim()}
                  />
                ) : null}
              </div>
            ) : (
              <BusinessEmptyState
                kind="no-data"
                title="选择映射任务"
                description="从左侧列表打开处理区"
              />
            )}
          </div>
        </div>
      ) : null}

      {view === "reconciliation" ? (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.3fr)_minmax(18rem,1fr)]">
          {data?.reconciliation ? (
            <>
              <div className="space-y-3">
                <Card size="sm">
                  <CardHeader>
                    <CardTitle>{data.reconciliation.jobNo}</CardTitle>
                    <CardDescription>
                      {data.reconciliation.boundaryLabel} · 商城{" "}
                      {data.reconciliation.mallCount} / ERP{" "}
                      {data.reconciliation.erpCount} · 差异{" "}
                      {data.reconciliation.differenceCount}
                    </CardDescription>
                  </CardHeader>
                </Card>
                <BusinessTableFrame
                  title="核对差异"
                  description="比较完整商业数据标识，只产生差异与任务，不直接覆盖记录。"
                  table={
                    <DataTable
                      data={data.reconciliation.differences}
                      columns={diffColumns}
                      getRowId={(r) => r.differenceId}
                      rowCount={data.reconciliation.differences.length}
                      layout="flush"
                      density="compact"
                    />
                  }
                />
              </div>
              {data.selectedDifference ? (
                <Card size="sm">
                  <CardHeader>
                    <CardTitle className="font-mono text-base">
                      {data.selectedDifference.externalOrderNo}
                    </CardTitle>
                    <CardDescription>
                      {data.selectedDifference.differenceTypeLabel}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-2 text-sm">
                    <BusinessStatusBadge
                      context="detail"
                      label={data.selectedDifference.statusLabel}
                      tone={data.selectedDifference.statusTone}
                    />
                    <p>{data.selectedDifference.impactSummary}</p>
                    {data.selectedDifference.erpSalesOrderNo ? (
                      <p>
                        ERP 销售单 {data.selectedDifference.erpSalesOrderNo}
                      </p>
                    ) : null}
                    {firstPhase && !policyMissing ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        onClick={() => {
                          setPullOpen(true)
                          pullForm.setFieldValue(
                            "externalOrderNo",
                            data.selectedDifference!.externalOrderNo
                          )
                        }}
                      >
                        按此单号补拉
                      </Button>
                    ) : null}
                  </CardContent>
                </Card>
              ) : null}
            </>
          ) : (
            <BusinessEmptyState
              kind="no-scope"
              title="当前无核对范围"
              description="当前没有可核对的差异；清除筛选后重试。"
            />
          )}
        </div>
      ) : null}

      {view === "history" ? (
        <div className="space-y-3">
          {sealed ? (
            <Alert>
              <AlertTitle>历史只读</AlertTitle>
              <AlertDescription>
                第一期同步已完成归档。请前往执行信息与对账工作区查看后续内容。
              </AlertDescription>
            </Alert>
          ) : null}
          {(data?.history ?? []).map((h) => (
            <Card key={h.id} size="sm">
              <CardHeader>
                <CardTitle className="text-base">{h.title}</CardTitle>
                <CardDescription>
                  {formatDateTime(h.recordedAt, "default")}
                  {h.watermark
                    ? ` · ${formatDateTime(h.watermark, "default")}`
                    : ""}
                  {h.reference ? ` · ${h.reference}` : ""}
                </CardDescription>
              </CardHeader>
              <CardContent className="text-sm text-muted-foreground">
                {h.summary}
              </CardContent>
            </Card>
          ))}
        </div>
      ) : null}

      {/* 立即增量 */}
      <Dialog open={incrementalOpen} onOpenChange={setIncrementalOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>立即执行增量</DialogTitle>
            <DialogDescription>
              不修改来源；范围由系统按当前同步进度计算。禁止页面改写同步进度。
            </DialogDescription>
          </DialogHeader>
          {policyMissing ? (
            <Alert variant="destructive">
              <AlertTitle>人工治理策略未配置</AlertTitle>
              <AlertDescription>
                策略未配置，动作禁用。定时增量说明仍可读：
                {context?.scheduledIncrementalNote}
              </AlertDescription>
            </Alert>
          ) : (
            <form
              className="space-y-3"
              onSubmit={(e) => {
                e.preventDefault()
                void incrementalForm.handleSubmit()
              }}
            >
              <p className="text-sm text-muted-foreground">
                同步至{" "}
                {context?.freshness.currentWatermark
                  ? formatDateTime(
                      context.freshness.currentWatermark,
                      "default"
                    )
                  : "—"}{" "}
                · 阶段 {STAGE_LABEL[stage]}
              </p>
              <incrementalForm.AppField
                name="reason"
                children={(field) => (
                  <field.TextField label="触发理由" />
                )}
              />
              <DialogFooter>
                <DialogClose render={<Button type="button" variant="outline" />}>
                  取消
                </DialogClose>
                <incrementalForm.AppForm>
                  <incrementalForm.SubmitButton label="创建增量任务" />
                </incrementalForm.AppForm>
              </DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>

      {/* 按单补拉 */}
      <Dialog open={pullOpen} onOpenChange={setPullOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>按单号补拉</DialogTitle>
            <DialogDescription>
              使用原来源身份；不创建第二张销售单。仅第一阶段（商城开单）且策略已配置时可用。
            </DialogDescription>
          </DialogHeader>
          {policyMissing || !firstPhase ? (
            <Alert variant="destructive">
              <AlertTitle>
                {policyMissing ? "人工治理策略未配置" : "阶段不可用"}
              </AlertTitle>
              <AlertDescription>
                {manualSyncDisabledReason}
              </AlertDescription>
            </Alert>
          ) : (
            <form
              className="space-y-3"
              onSubmit={(e) => {
                e.preventDefault()
                void pullForm.handleSubmit()
              }}
            >
              <pullForm.AppField
                name="externalOrderNo"
                children={(field) => (
                  <field.TextField label="商城销售单号" />
                )}
              />
              <pullForm.AppField
                name="reason"
                children={(field) => (
                  <field.TextField label="补拉理由" />
                )}
              />
              <DialogFooter>
                <DialogClose render={<Button type="button" variant="outline" />}>
                  取消
                </DialogClose>
                <pullForm.AppForm>
                  <pullForm.SubmitButton label="创建补拉任务" />
                </pullForm.AppForm>
              </DialogFooter>
            </form>
          )}
        </DialogContent>
      </Dialog>

      {/* 暂挂 */}
      <Dialog open={deferOpen} onOpenChange={setDeferOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>先跳过当前映射</DialogTitle>
            <DialogDescription>
              只记录结构化原因与当前处理位置；不会暂停或完成任务。
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault()
              void deferForm.handleSubmit()
            }}
          >
            <deferForm.AppField
              name="reasonCode"
              children={(field) => (
                <div className="space-y-1.5">
                  <Label>原因</Label>
                  <OptionCombobox
                    value={field.state.value}
                    onValueChange={(v) => {
                      if (v)
                        field.handleChange(
                          v as
                            | "WAITING_SOURCE"
                            | "NEED_CLARIFICATION"
                            | "WAITING_MASTER_DATA"
                            | "OTHER"
                        )
                    }}
                    options={DEFER_REASON_OPTIONS.map((o) => ({
                      value: o.value,
                      label: o.label,
                    }))}
                    allowClear={false}
                  />
                </div>
              )}
            />
            <deferForm.AppField
              name="note"
              children={(field) => (
                <field.TextareaField label="备注（可选）" />
              )}
            />
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                取消
              </DialogClose>
              <deferForm.AppForm>
                <deferForm.SubmitButton label="确认跳过" />
              </deferForm.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <FormalActionConfirmDialog
        open={retryConfirmOpen}
        onOpenChange={setRetryConfirmOpen}
        actionLabel="重试失败任务"
        title="确认重试失败任务"
        description="沿原任务范围与同步规则重试未成功部分；不回退已捕获的同步进度。"
        fromStatus={{ label: data?.selectedJob?.statusLabel ?? "失败", tone: "warning" }}
        toStatus={{ label: "重试中", tone: "info" }}
        effects={["仅重试未成功的分页", "不修改来源数据"]}
        irreversibleEffects={["重试记录进入任务审计"]}
        pending={retryJob.isPending}
        onConfirm={() => handleRetryJob()}
      />

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        actionLabel="确认映射"
        fromStatus={{ label: "待处理", tone: "warning" }}
        toStatus={{ label: "映射已解决", tone: "success" }}
        description="确认身份关系后，映射任务将标为已解决并完成待办；不立即形成销售版本。"
        effects={[
          "追加可审计映射目标",
          "完成当前任务",
          "不向商城回写",
          "重新归集为独立下一步",
        ]}
        irreversibleEffects={["映射结论进入不可变处理审计"]}
        pending={confirmMutation.isPending}
        onConfirm={() => handleConfirm()}
      />
    </div>
  )
}
