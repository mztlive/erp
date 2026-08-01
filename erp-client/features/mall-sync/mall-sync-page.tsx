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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Switch } from "@/components/ui/switch"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import type {
  DemoRole,
  MallSnapshotRow,
  MallSyncJobRow,
  MallSyncViewName,
  MappingTaskView,
  OwnershipStage,
  ReconciliationDifference,
} from "@/features/mall-sync/types"
import {
  DEFER_REASON_OPTIONS,
  DEMO_ROLE_LABEL,
  DIRECTION_LABEL,
  STAGE_LABEL,
  VIEW_LABEL,
} from "@/features/mall-sync/types"
import {
  useAssignMappingMutation,
  useClaimMappingMutation,
  useConfirmMappingMutation,
  useDeferMappingMutation,
  useMallSyncDemoControls,
  useMallSyncPageQuery,
  useReapplyMutation,
  useResolveUnknownReapplyMutation,
  useRetryJobMutation,
  useTriggerIncrementalMutation,
  useTriggerSingleOrderMutation,
} from "@/features/mall-sync/queries"
import { cn } from "@/lib/utils"

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

function parseRole(raw: string | null): DemoRole {
  if (
    raw === "admin" ||
    raw === "sales" ||
    raw === "operations" ||
    raw === "finance"
  ) {
    return raw
  }
  return "admin"
}

function parseStage(raw: string | null): OwnershipStage | undefined {
  if (
    raw === "FIRST_PHASE_MALL_OWNED" ||
    raw === "MIGRATION_FROZEN" ||
    raw === "SECOND_PHASE_ERP_OWNED"
  ) {
    return raw
  }
  return undefined
}

type SessionLease = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expiresAt: string
}

type ResultState =
  | {
      status: "succeeded" | "failed" | "unknown" | "blocked"
      title: string
      description: string
      reference?: string
      stayOnItem?: boolean
      pendingIdempotencyKey?: string
    }
  | null

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

function formatTime(iso?: string) {
  if (!iso) return "—"
  try {
    return new Date(iso).toLocaleString("zh-CN", { hour12: false })
  } catch {
    return iso
  }
}

export function MallSyncPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const view = parseView(searchParams.get("view"))
  const demoRole = parseRole(
    searchParams.get("demoRole") ?? searchParams.get("role")
  )
  const demoStage = parseStage(searchParams.get("demoStage"))
  const policyParam = searchParams.get("policy")
  const policy: "missing" | "configured" | undefined =
    policyParam === "configured"
      ? "configured"
      : policyParam === "missing"
        ? "missing"
        : undefined
  const sourceUnavailableParam = searchParams.get("sourceUnavailable")
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
    searchParams.get("queueContextId") ?? `queue:W17:mall-sync:${demoRole}`

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
  const [forceUnknown, setForceUnknown] = React.useState(false)
  const [actionError, setActionError] = React.useState<string | null>(null)

  const queryInput = React.useMemo(
    () => ({
      view,
      demoRole,
      demoStage,
      policy,
      sourceUnavailable:
        sourceUnavailableParam === "1"
          ? true
          : sourceUnavailableParam === "0"
            ? false
            : undefined,
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
      demoRole,
      demoStage,
      policy,
      sourceUnavailableParam,
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
  const demoControls = useMallSyncDemoControls()
  const triggerInc = useTriggerIncrementalMutation()
  const triggerSo = useTriggerSingleOrderMutation()
  const retryJob = useRetryJobMutation()
  const claimMutation = useClaimMappingMutation()
  const confirmMutation = useConfirmMappingMutation()
  const deferMutation = useDeferMappingMutation()
  const reapplyMutation = useReapplyMutation()
  const resolveReapply = useResolveUnknownReapplyMutation()
  const assignMutation = useAssignMappingMutation()

  const data = pageQuery.data
  const context = data?.context
  const ownership = context?.ownership
  const policyState = context?.manualGovernancePolicy
  const policyMissing = policyState?.state === "MISSING"
  const stage = ownership?.stage ?? "FIRST_PHASE_MALL_OWNED"
  const firstPhase = stage === "FIRST_PHASE_MALL_OWNED"
  const frozen = stage === "MIGRATION_FROZEN"
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
    const next = new URLSearchParams(searchParams.toString())
    for (const [key, value] of Object.entries(patch)) {
      if (value == null || value === "") next.delete(key)
      else next.set(key, value)
    }
    if (!next.get("view")) next.set("view", view)
    const qs = next.toString()
    const href = qs ? `${pathname}?${qs}` : pathname
    if (options?.replace) router.replace(href)
    else router.push(href)
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
      const key = `idem_so_${Date.now()}`
      const res = await triggerSo.mutateAsync({
        externalOrderNo: value.externalOrderNo,
        reason: value.reason,
        demoRole,
        policyConfigured: !policyMissing,
        stage,
        idempotencyKey: key,
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
      const key = `idem_inc_${Date.now()}`
      const res = await triggerInc.mutateAsync({
        reason: value.reason,
        demoRole,
        policyConfigured: !policyMissing,
        stage,
        idempotencyKey: key,
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
        subjectHash: mappingTask.workItem.subjectHash,
        subjectVersion: mappingTask.workItem.subjectVersion,
      })
      setSessionLease({
        workItemId: lease.workItemId,
        claimToken: lease.claimToken,
        leaseVersion: lease.leaseVersion,
        expiresAt: lease.expiresAt,
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
    const key = `idem_confirm_${mappingTask.mappingTaskId}_${Date.now()}`
    const res = await confirmMutation.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      workItemId: mappingTask.workItem.workItemId,
      claimToken: sessionLease.claimToken,
      leaseVersion: sessionLease.leaseVersion,
      expectedSubjectHash: mappingTask.workItem.subjectHash,
      expectedLockVersion: mappingTask.lockVersion,
      targetObjectId: candidate.objectId,
      targetLabel: `${candidate.stableNo} ${candidate.label}`,
      evidenceNote,
      demoRole,
      idempotencyKey: key,
      forceUnknown,
    })
    setConfirmOpen(false)
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "映射已确认",
        description: res.message,
        reference: res.externalIdentityMapId,
      })
      setSessionLease(null)
      void pageQuery.refetch()
    } else if (res.status === "unknown") {
      setResult({
        status: "unknown",
        title: "确认结果未知",
        description: res.message,
        stayOnItem: true,
        pendingIdempotencyKey: res.idempotencyKey,
      })
    } else {
      setActionError(res.message)
    }
  }

  async function handleDefer(reasonCode: string, note?: string) {
    if (mappingTask?.ownerRoutingState !== "CONFIGURED" || !sessionLease) {
      setActionError("请先领取任务")
      return
    }
    const key = `idem_defer_${mappingTask.mappingTaskId}_${Date.now()}`
    const res = await deferMutation.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      workItemId: mappingTask.workItem.workItemId,
      claimToken: sessionLease.claimToken,
      leaseVersion: sessionLease.leaseVersion,
      expectedSubjectHash: mappingTask.workItem.subjectHash,
      reasonCode,
      note,
      queueContextId,
      demoRole,
      idempotencyKey: key,
    })
    setDeferOpen(false)
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "已暂挂（任务仍在队列）",
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

  async function handleReapply(forceUnk = false) {
    if (!mappingTask) return
    const key = `idem_reapply_${mappingTask.mappingTaskId}_${Date.now()}`
    const res = await reapplyMutation.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      sourceSnapshotId: mappingTask.sourceSnapshotId,
      demoRole,
      idempotencyKey: key,
      forceUnknown: forceUnk,
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

  async function handleResolveUnknownReapply() {
    if (!mappingTask?.reapplyOperation) return
    const res = await resolveReapply.mutateAsync({
      mappingTaskId: mappingTask.mappingTaskId,
      operationId: mappingTask.reapplyOperation.operationId,
      settle: true,
    })
    if (res.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "重新归集终态已确认",
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
    demoRole === "admin" && firstPhase && !policyMissing && !context?.sourceUnavailable
  const manualSyncDisabledReason = !firstPhase
    ? frozen
      ? "迁移冻结：普通立即增量/按单补拉禁用，请从 W24 批次执行"
      : "已封存：无第一期写动作"
    : policyMissing
      ? "MANUAL_GOVERNANCE_POLICY_MISSING：人工策略未配置"
      : demoRole !== "admin"
        ? "仅管理员可触发"
        : context?.sourceUnavailable
          ? "来源不可用时不新建推进任务（可安全重试既有失败）"
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
        header: "页/数据/错",
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
        header: "水位",
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {row.original.watermarkAdvanced ? "已安全推进" : "未推进"}
          </span>
        ),
      },
      {
        id: "started",
        header: "开始",
        cell: ({ row }) => (
          <span className="text-sm tabular-nums">
            {formatTime(row.original.startedAt)}
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
        header: "数据标识",
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
        header: "正式待办",
        cell: ({ row }) =>
          row.original.ownerRoutingState === "CONFIGURED" ? (
            <span className="font-mono text-xs">
              {row.original.workItem.workItemId}
            </span>
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
        header: "指纹",
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
    demoRole === "admin"
      ? "released"
      : mappingTask?.ownerRoutingState !== "CONFIGURED"
        ? "released"
        : mappingTask.mappingTaskStatus === "RESOLVED"
          ? "released"
          : sessionLease?.workItemId === mappingTask.workItem.workItemId
            ? "active"
            : "unclaimed"

  const canConfirmMapping =
    demoRole !== "admin" &&
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
          { id: "gov", label: "治理", href: "/governance/mall-sync", current: false },
          { id: "sync", label: "商城同步与映射", current: true },
        ]}
        metadata={
          <div className="flex flex-wrap items-center gap-3">
            <DataFreshness
              updatedAt="刚刚"
              dateTime={context?.freshness.viewProjectedAt}
              state={context?.sourceUnavailable ? "stale" : "fresh"}
              label="同步数据"
            />
            <Badge variant="outline">
              {context?.sourceSystem.name} · {context?.sourceSystem.environmentLabel}
            </Badge>
            <Badge variant="secondary">{context?.viewerRoleLabel}</Badge>
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
          tone={
            sealed ? "info" : frozen ? "warning" : "info"
          }
          icon={sealed || frozen ? ShieldAlertIcon : undefined}
          title={
            sealed
              ? `第一期已封存 · ${DIRECTION_LABEL[ownership.syncDirection]}`
              : frozen
                ? `迁移维护窗口已冻结 · 商城主责 ${ownership.mallOwnedOrderCount ?? 0} / ERP 主责 ${ownership.erpOwnedOrderCount ?? 0}`
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
                  封存时间 {formatTime(ownership.sealedAt)}
                  {ownership.finalWatermark
                    ? ` · 最终水位 ${ownership.finalWatermark}`
                    : ""}
                </p>
              ) : null}
              {ownership.writeFrozenAt ? (
                <p>写入冻结自 {formatTime(ownership.writeFrozenAt)}</p>
              ) : null}
              <p className="text-muted-foreground">
                无「编辑来源数据」「向商城回写商业修改」「手工标记同步成功」入口。
              </p>
            </div>
          }
          action={
            frozen || sealed
              ? {
                  label: "前往迁移 / 执行信息 / 错误中心",
                  onClick: () => {
                    router.push("/governance/ownership-migrations")
                  },
                }
              : undefined
          }
        />
      ) : null}

      {(frozen || sealed) && (
        <div className="flex flex-wrap gap-2 text-sm">
          <Button variant="link" size="sm" render={<Link href="/commerce/execution-projections" />}>
            W23 执行信息
            <ExternalLinkIcon className="size-3.5" />
          </Button>
          <Button variant="link" size="sm" render={<Link href="/governance/ownership-migrations" />}>
            W24 主责迁移
            <ExternalLinkIcon className="size-3.5" />
          </Button>
          <Button variant="link" size="sm" render={<Link href="/governance/integration-errors" />}>
            W29 接口错误与对账
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
              代码 <code className="text-xs">MANUAL_GOVERNANCE_POLICY_MISSING</code>
              ：「立即增量」「按单补拉」已禁用（界面与服务端均拒绝）。
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
                ? "单人理由"
                : "双人授权"
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

      {/* 演示控制：角色 / 阶段 / 策略 / 来源（mock） */}
      <Card size="sm">
        <CardHeader className="border-b py-3">
          <CardTitle className="text-sm">演示控制（会话 mock）</CardTitle>
          <CardDescription>
            切换角色与主责阶段以验证分权、冻结与封存；不写入服务端配置。
          </CardDescription>
        </CardHeader>
        <CardContent className="flex flex-wrap items-end gap-4 pt-3">
          <div className="space-y-1">
            <Label className="text-xs">演示角色</Label>
            <Select
              value={demoRole}
              onValueChange={(v) => {
                const role = parseRole(v)
                const defaultView =
                  role === "admin" ? "overview" : "mapping"
                patchUrl({
                  demoRole: role,
                  view: view === "overview" && role !== "admin" ? defaultView : view,
                })
              }}
            >
              <SelectTrigger className="w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(Object.keys(DEMO_ROLE_LABEL) as DemoRole[]).map((r) => (
                  <SelectItem key={r} value={r}>
                    {DEMO_ROLE_LABEL[r]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label className="text-xs">主责阶段</Label>
            <Select
              value={stage}
              onValueChange={(v) => {
                const s = parseStage(v) ?? "FIRST_PHASE_MALL_OWNED"
                void demoControls.setStage(s)
                patchUrl({
                  demoStage: s,
                  view: s === "SECOND_PHASE_ERP_OWNED" ? "history" : view,
                })
              }}
            >
              <SelectTrigger className="w-48">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="FIRST_PHASE_MALL_OWNED">
                  第一阶段 · 商城主责
                </SelectItem>
                <SelectItem value="MIGRATION_FROZEN">迁移冻结</SelectItem>
                <SelectItem value="SECOND_PHASE_ERP_OWNED">
                  已封存 · ERP 主责
                </SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="flex items-center gap-2">
            <Switch
              id="policy"
              checked={!policyMissing}
              onCheckedChange={(checked) => {
                void demoControls.setPolicy(checked)
                patchUrl({ policy: checked ? "configured" : "missing" })
              }}
            />
            <Label htmlFor="policy" className="text-xs">
              人工治理策略已配置
            </Label>
          </div>
          <div className="flex items-center gap-2">
            <Switch
              id="src-down"
              checked={!!context?.sourceUnavailable}
              onCheckedChange={(checked) => {
                void demoControls.setSourceUnavailable(checked)
                patchUrl({ sourceUnavailable: checked ? "1" : "0" })
              }}
            />
            <Label htmlFor="src-down" className="text-xs">
              来源不可用
            </Label>
          </div>
        </CardContent>
      </Card>

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
                ...(m.targetFilter ?? {}),
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
          placeholder="商城单号 / ERP 单号 / 任务号"
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
          actions={
            result.status === "unknown" &&
            mappingTask?.reapplyOperation?.status === "UNKNOWN" ? (
              <Button
                type="button"
                size="sm"
                onClick={() => void handleResolveUnknownReapply()}
              >
                查询重新归集终态
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
                水位证明来源白名单数据已安全捕获，不证明映射或应收已成功。
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground">当前水位</span>
                <span className="font-mono text-xs">
                  {context?.freshness.currentWatermark ?? "—"}
                </span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground">最近成功</span>
                <span>{formatTime(context?.freshness.latestSuccessfulJobAt)}</span>
              </div>
              <div className="flex justify-between gap-2">
                <span className="text-muted-foreground">来源安全时间</span>
                <span>{formatTime(context?.freshness.sourceSafeTime)}</span>
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
            </CardContent>
          </Card>
        </div>
      ) : null}

      {view === "jobs" ? (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.4fr)_minmax(18rem,1fr)]">
          <BusinessTableFrame
            title="同步任务"
            description="基线 / 增量 / 单号补拉。水位不因异步映射失败回退。"
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
                    错误分类：{data.selectedJob.errorClass}
                  </p>
                ) : null}
                <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                  <span>游标前 {data.selectedJob.cursorBefore ?? "—"}</span>
                  <span>游标后 {data.selectedJob.cursorAfter ?? "—"}</span>
                </div>
                {demoRole === "admin" &&
                data.selectedJob.allowedActions.includes("RETRY_FAILED_JOB") ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    disabled={retryJob.isPending}
                    onClick={async () => {
                      const res = await retryJob.mutateAsync({
                        jobId: data.selectedJob!.jobId,
                        reason: "安全重试部分失败分页",
                        demoRole,
                        stage,
                        idempotencyKey: `retry_${data.selectedJob!.jobId}_${Date.now()}`,
                      })
                      if (res.status === "succeeded") {
                        setResult({
                          status: "succeeded",
                          title: "重试已创建",
                          description: res.message,
                          reference: res.jobNo,
                        })
                      } else {
                        setActionError(res.message)
                      }
                    }}
                  >
                    重试失败任务
                  </Button>
                ) : null}
                {data.selectedJob.actionBlockers.map((b) => (
                  <p key={b.code} className="text-xs text-amber-700 dark:text-amber-400">
                    {b.code}：{b.message}
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
                  指纹 {data.selectedSnapshot.contentHashShort} · 任务{" "}
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
              description="从列表打开白名单 detail"
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
                  ? "清除筛选或切换角色查看其它责任范围"
                  : "新任务到达后刷新"
              }
            />
          ) : null}

          <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)]">
            <BusinessTableFrame
              title="映射任务"
              description="映射状态与重新归集状态分列；责任路由 MISSING 时不可执行。"
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
                          · 责任 {mappingTask.ownerRoleLabel} · 待办{" "}
                          <span className="font-mono text-xs">
                            {mappingTask.workItem.workItemId}
                          </span>
                        </>
                      ) : (
                        " · 待责任配置（无 workItem）"
                      )}
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    <Alert>
                      <AlertTitle>确认的是身份关系</AlertTitle>
                      <AlertDescription>
                        不是修改来源销售单；相似候选绝不自动确认/合并。
                        {demoRole === "admin"
                          ? " 管理员可指派/排障，不能替业务确认。"
                          : ""}
                      </AlertDescription>
                    </Alert>

                    {mappingTask.ownerRoutingState === "MISSING" ? (
                      <Alert variant="destructive">
                        <AlertTitle>OWNER_ROUTING_MISSING</AlertTitle>
                        <AlertDescription>
                          结算主体责任未配置唯一 ownerRole；领域差异已保存，确认禁用，不向销售与财务同时生成可完成待办。
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
                                  demoRole === "admin" ||
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
                            note: "确认后写 external_identity_map，不改来源单",
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
                              {formatTime(h.handledAt)} · {h.action} ·{" "}
                              {h.result} · {h.handledBy}
                            </li>
                          ))}
                        </ul>
                      </div>
                    ) : null}

                    {demoRole !== "admin" &&
                    mappingTask.ownerRoutingState === "CONFIGURED" &&
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
                        <div className="flex items-center gap-2">
                          <Switch
                            id="force-unk"
                            checked={forceUnknown}
                            onCheckedChange={setForceUnknown}
                          />
                          <Label htmlFor="force-unk" className="text-xs">
                            演示：提交后结果未知
                          </Label>
                        </div>
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
                            暂挂
                          </Button>
                        </div>
                      </form>
                    ) : null}

                    {demoRole === "admin" &&
                    mappingTask.ownerRoutingState === "CONFIGURED" &&
                    mappingTask.mappingTaskStatus === "PENDING" ? (
                      <div className="flex flex-wrap gap-2">
                        <Button
                          type="button"
                          size="sm"
                          variant="secondary"
                          disabled={assignMutation.isPending}
                          onClick={async () => {
                            const res = await assignMutation.mutateAsync({
                              mappingTaskId: mappingTask.mappingTaskId,
                              targetOwnerRole: mappingTask.ownerRole,
                              reason: "管理员指派",
                              demoRole,
                            })
                            if (res.status === "succeeded") {
                              setResult({
                                status: "succeeded",
                                title: "已指派",
                                description: res.message,
                              })
                            } else {
                              setActionError(res.message)
                            }
                          }}
                        >
                          指派业务责任人
                        </Button>
                        <p className="w-full text-xs text-muted-foreground">
                          管理员不能确认映射（CONFIRM_TARGET 已阻断）。
                        </p>
                      </div>
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
                            {demoRole !== "admin" ? (
                              <>
                                <Button
                                  type="button"
                                  size="sm"
                                  disabled={reapplyMutation.isPending}
                                  onClick={() => void handleReapply(false)}
                                >
                                  重新归集
                                </Button>
                                <Button
                                  type="button"
                                  size="sm"
                                  variant="outline"
                                  onClick={() => void handleReapply(true)}
                                >
                                  演示结果未知
                                </Button>
                              </>
                            ) : null}
                            {mappingTask.reapplyOperation?.status ===
                            "UNKNOWN" ? (
                              <Button
                                type="button"
                                size="sm"
                                variant="secondary"
                                onClick={() => void handleResolveUnknownReapply()}
                              >
                                查询终态
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
                        {b.action} · {b.code}：{b.message}
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
                    {demoRole === "admin" && firstPhase && !policyMissing ? (
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
              title="当前角色无核对范围"
              description="按数据范围过滤后的 mock 演示"
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
                第一期轮询已封存。当前执行信息、迁移与通用对账请前往 W23 / W24 /
                W29。
              </AlertDescription>
            </Alert>
          ) : null}
          {(data?.history ?? []).map((h) => (
            <Card key={h.id} size="sm">
              <CardHeader>
                <CardTitle className="text-base">{h.title}</CardTitle>
                <CardDescription>
                  {formatTime(h.recordedAt)}
                  {h.watermark ? ` · ${h.watermark}` : ""}
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
              不修改来源；范围由服务端按安全水位计算。禁止客户端移动高水位。
            </DialogDescription>
          </DialogHeader>
          {policyMissing ? (
            <Alert variant="destructive">
              <AlertTitle>MANUAL_GOVERNANCE_POLICY_MISSING</AlertTitle>
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
                当前水位 {context?.freshness.currentWatermark ?? "—"} · 阶段{" "}
                {STAGE_LABEL[stage]}
              </p>
              <incrementalForm.AppField
                name="reason"
                children={(field) => (
                  <field.TextField label="触发理由（单人模式）" />
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
              使用原来源身份；不创建第二张销售单。仅 FIRST_PHASE_MALL_OWNED 且策略已配置。
            </DialogDescription>
          </DialogHeader>
          {policyMissing || !firstPhase ? (
            <Alert variant="destructive">
              <AlertTitle>
                {policyMissing
                  ? "MANUAL_GOVERNANCE_POLICY_MISSING"
                  : "阶段不可用"}
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
            <DialogTitle>暂挂当前映射</DialogTitle>
            <DialogDescription>
              只记录结构化原因与队列上下文；不改 mappingTaskStatus、不写
              paused、不完成任务。
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
                  <Select
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
                  >
                    <SelectTrigger>
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {DEFER_REASON_OPTIONS.map((o) => (
                        <SelectItem key={o.value} value={o.value}>
                          {o.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
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
                <deferForm.SubmitButton label="确认暂挂" />
              </deferForm.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        actionLabel="确认映射"
        fromStatus={{ label: "待处理", tone: "warning" }}
        toStatus={{ label: "映射已解决", tone: "success" }}
        description="确认身份关系后，mappingTaskStatus 置为已解决并完成正式待办；不立即形成销售版本。"
        effects={[
          "追加可审计映射目标",
          "完成当前 work_item",
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
