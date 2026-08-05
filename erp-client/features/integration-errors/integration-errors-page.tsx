"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  ExternalLinkIcon,
  PauseIcon,
  RefreshCwIcon,
  SearchIcon,
  ShieldAlertIcon,
  SkipForwardIcon,
} from "lucide-react"
import {
  AuditTimeline,
  BusinessDiffPanel,
  BusinessEmptyState,
  BusinessStatusBadge,
  DataFreshness,
  FormalActionResult,
  InterfaceErrorResolutionPanel,
  MetricItem,
  MetricStrip,
  OptionCombobox,
  PageHeader,
  SequentialProcessBar,
  WorkTaskItem,
  type InterfaceErrorClass,
  type InterfaceErrorResolutionActions,
  type InterfaceErrorStatus,
} from "@/components/business"
import { TRANSFER_ROLE_OPTIONS } from "@/lib/business-options"
import { formatDateTime } from "@/lib/datetime"
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
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { cn } from "@/lib/utils"

import {
  useClaimIntegrationMutation,
  useCloseIntegrationMutation,
  useDirectReconciliationMutation,
  useIntegrationActionMutation,
  useIntegrationItemQuery,
  useIntegrationQueueQuery,
  useResolveIntegrationMutation,
  useTransferIntegrationMutation,
} from "./queries"
import type {
  IntegrationFormalResult,
  IntegrationResolutionItemView,
  IntegrationView,
} from "./types"
import {
  ENV_LABEL,
  EVIDENCE_KIND_LABEL,
  ERROR_CLASS_LABEL,
  FUNDS_LABEL,
  MODE_LABEL,
  OWNER_LABEL,
  VIEW_LABEL,
} from "./types"
import {
  buildIntegrationSearchParams,
  parseIntegrationSearchParams,
  toResolutionQuery,
} from "./url-state"

type SessionLease = {
  workItemId: string
}

function newKey(prefix: string) {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`
}

const ACTION_LABEL: Record<string, string> = {
  QUERY_ORIGINAL_RESULT: "查询原结果",
  REPLAY_ORIGINAL: "重新提交",
  ADD_EVIDENCE: "补充证据",
  LINK_COMPENSATION: "关联补偿",
  REATTRIBUTE: "重新归集",
  TRANSFER: "转交",
  RESOLVE: "处理完成",
  DEFER: "先跳过",
  SKIP: "跳过当前项",
  CLOSE_DUPLICATE: "关闭重复",
  CLOSE_MISROUTED: "关闭错误路由",
  CONFIRM_NO_ERROR: "确认无误",
  CONFIRM_VALID_DIFFERENCE: "确认有效差异",
}

function severityTone(
  s: IntegrationResolutionItemView["classification"]["severity"]
): "destructive" | "warning" | "info" | "neutral" {
  if (s === "critical") return "destructive"
  if (s === "high") return "warning"
  if (s === "medium") return "info"
  return "neutral"
}

function mapPanelStatus(
  item: IntegrationResolutionItemView
): InterfaceErrorStatus {
  if (item.status.code === "AUTO_RETRYING") return "auto-retrying"
  if (item.status.label.includes("人工") || item.status.code === "MANUAL_REQUIRED")
    return "manual-required"
  if (item.status.code === "COMPLETED" || item.status.label.includes("已解决"))
    return "resolved"
  if (item.status.code === "CLOSED" || item.status.label.includes("关闭"))
    return "closed"
  return "pending"
}

function isPanelErrorClass(
  c: IntegrationResolutionItemView["classification"]["errorClass"]
): c is InterfaceErrorClass {
  return c !== "reconciliation-difference"
}

function formalStatus(
  s: IntegrationFormalResult["status"]
): "succeeded" | "blocked" | "rejected" | "unknown" | "processing" {
  if (s === "failed") return "rejected"
  return s
}

export function IntegrationErrorsPage({
  forcedTaskId,
  forcedDifferenceId,
}: {
  forcedTaskId?: string
  forcedDifferenceId?: string
} = {}) {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const urlState = React.useMemo(
    () => parseIntegrationSearchParams(searchParams),
    [searchParams]
  )

  const currentTaskId = forcedTaskId ?? urlState.currentTaskId
  const currentDifferenceId = forcedDifferenceId ?? urlState.currentDifferenceId
  const focusMode = Boolean(forcedTaskId || forcedDifferenceId)

  const query = React.useMemo(
    () =>
      toResolutionQuery({
        ...urlState,
        currentTaskId,
        currentDifferenceId,
      }),
    [urlState, currentTaskId, currentDifferenceId]
  )

  const queueQuery = useIntegrationQueueQuery(query)
  const claimMutation = useClaimIntegrationMutation()
  const actionMutation = useIntegrationActionMutation()
  const resolveMutation = useResolveIntegrationMutation()
  const closeMutation = useCloseIntegrationMutation()
  const transferMutation = useTransferIntegrationMutation()
  const directMutation = useDirectReconciliationMutation()

  const view = queueQuery.data
  const queueItems = view?.items ?? []
  const metrics = view?.metrics

  const detailItemQuery = useIntegrationItemQuery({
    itemType: forcedDifferenceId ? "RECONCILIATION_DIFFERENCE" : "ERROR_TASK",
    id: forcedTaskId ?? forcedDifferenceId ?? "",
    enabled: Boolean(forcedTaskId || forcedDifferenceId),
  })

  const item: IntegrationResolutionItemView | undefined = React.useMemo(() => {
    if (forcedTaskId || forcedDifferenceId) {
      return detailItemQuery.data ?? undefined
    }
    if (currentTaskId) {
      return (
        queueItems.find(
          (i) =>
            i.identity.itemType === "ERROR_TASK" &&
            i.identity.id === currentTaskId
        ) ?? queueItems.find((i) => i.identity.id === currentTaskId)
      )
    }
    if (currentDifferenceId) {
      return queueItems.find(
        (i) =>
          i.identity.itemType === "RECONCILIATION_DIFFERENCE" &&
          i.identity.id === currentDifferenceId
      )
    }
    return queueItems[0]
  }, [
    forcedTaskId,
    forcedDifferenceId,
    detailItemQuery.data,
    queueItems,
    currentTaskId,
    currentDifferenceId,
  ])

  const items =
    focusMode && item
      ? [item]
      : queueItems

  const currentIndex = item
    ? Math.max(
        0,
        items.findIndex((i) => i.identity.id === item.identity.id)
      )
    : 0

  const [lastResult, setLastResult] =
    React.useState<IntegrationFormalResult | null>(null)
  const [actionError, setActionError] = React.useState<string | null>(null)
  const [forceUnknownOnce, setForceUnknownOnce] = React.useState(false)
  const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
  const [replacementWi, setReplacementWi] = React.useState("wi_iet_orig_55102")
  const [transferRole, setTransferRole] = React.useState("研发运维")
  const [reconReasonId, setReconReasonId] = React.useState("")
  const [sessionAutoNext, setSessionAutoNext] = React.useState(urlState.autoNext)
  const [comment, setComment] = React.useState("")

  const leaseRef = React.useRef<SessionLease | null>(null)
  const [activeLease, setActiveLease] = React.useState<SessionLease | null>(null)
  const resultRef = React.useRef<HTMLDivElement>(null)
  const headingRef = React.useRef<HTMLHeadingElement>(null)

  const autoNext = urlState.autoNext

  const replaceUrl = React.useCallback(
    (patch: Record<string, string | null | undefined>) => {
      if (focusMode && (forcedTaskId || forcedDifferenceId)) {
        // detail routes keep path; still allow query prefs
        const params = new URLSearchParams(searchParams.toString())
        for (const [k, v] of Object.entries(patch)) {
          if (k === "taskId" || k === "differenceId" || k === "currentTaskId" || k === "currentDifferenceId")
            continue
          if (v == null || v === "") params.delete(k)
          else params.set(k, v)
        }
        const qs = params.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname, { scroll: false })
        return
      }
      const base = parseIntegrationSearchParams(searchParams)
      const next = {
        ...base,
        view: (patch.view as IntegrationView | undefined) ?? base.view,
        mode:
          (patch.mode as typeof base.mode | undefined) ??
          base.mode,
        environment:
          (patch.environment as typeof base.environment | undefined) ??
          base.environment,
        errorClass:
          patch.errorClass === null
            ? undefined
            : (patch.errorClass ?? base.errorClass),
        owner:
          (patch.owner as typeof base.owner | undefined) ?? base.owner,
        q: patch.q === null ? undefined : (patch.q ?? base.q),
        queueContextId:
          patch.queueContextId ?? base.queueContextId,
        resolveWorkItemId:
          patch.resolveWorkItemId === null
            ? undefined
            : (patch.resolveWorkItemId ?? base.resolveWorkItemId),
        currentTaskId:
          patch.taskId === null
            ? undefined
            : (patch.taskId ?? base.currentTaskId),
        currentDifferenceId:
          patch.differenceId === null
            ? undefined
            : (patch.differenceId ?? base.currentDifferenceId),
        autoNext:
          patch.autoNext === "0"
            ? false
            : patch.autoNext === "1"
              ? true
              : base.autoNext,
      }
      const params = buildIntegrationSearchParams(next)
      // clear resolve after apply
      if (patch.resolveWorkItemId === null) {
        params.delete("resolveWorkItemId")
      }
      const qs = params.toString()
      router.replace(
        qs ? `/governance/integration-errors?${qs}` : "/governance/integration-errors",
        { scroll: false }
      )
    },
    [focusMode, forcedDifferenceId, forcedTaskId, pathname, router, searchParams]
  )

  // resolveWorkItemId → domain detail replace
  React.useEffect(() => {
    if (!view?.resolvedEntry) return
    if (!urlState.resolveWorkItemId) return
    const entry = view.resolvedEntry
    if (entry.itemType === "ERROR_TASK") {
      router.replace(
        `/governance/integration-errors/errors/${entry.id}?queueContextId=${encodeURIComponent(urlState.queueContextId)}&view=${urlState.view}&autoNext=${autoNext ? "1" : "0"}`
      )
    } else {
      router.replace(
        `/governance/integration-errors/differences/${entry.id}?queueContextId=${encodeURIComponent(urlState.queueContextId)}&view=${urlState.view}&autoNext=${autoNext ? "1" : "0"}`
      )
    }
  }, [view?.resolvedEntry, urlState.resolveWorkItemId, urlState.queueContextId, urlState.view, autoNext, router])

  // URL defaults for current item
  React.useEffect(() => {
    if (queueQuery.isPending || !view || focusMode) return
    if (urlState.resolveWorkItemId) return
    const hasTask = searchParams.has("taskId")
    const hasDiff = searchParams.has("differenceId")
    const hasView = searchParams.has("view")
    const hasCtx = searchParams.has("queueContextId")
    if (hasView && hasCtx && (hasTask || hasDiff || items.length === 0)) return
    const params = buildIntegrationSearchParams({
      ...urlState,
      currentTaskId:
        item?.identity.itemType === "ERROR_TASK"
          ? item.identity.id
          : urlState.currentTaskId,
      currentDifferenceId:
        item?.identity.itemType === "RECONCILIATION_DIFFERENCE"
          ? item.identity.id
          : urlState.currentDifferenceId,
    })
    router.replace(
      `/governance/integration-errors?${params.toString()}`,
      { scroll: false }
    )
  }, [
    queueQuery.isPending,
    view,
    focusMode,
    urlState,
    item,
    items.length,
    searchParams,
    router,
  ])

  // Reset UI on item switch
  React.useEffect(() => {
    setActionError(null)
    if (item?.reconciliationReasonRegistry?.registeredReasons[0]) {
      setReconReasonId(
        item.reconciliationReasonRegistry.registeredReasons[0].registeredReasonId
      )
    }
  }, [item?.identity.id])

  // Auto-claim when work_item present
  React.useEffect(() => {
    if (!item?.workItem) return
    if (leaseRef.current?.workItemId === item.workItem.workItemId) return
    if (claimMutation.isPending) return
    let cancelled = false
    void claimMutation
      .mutateAsync({
        workItemId: item.workItem.workItemId,
        subjectVersion: item.workItem.subjectVersion ?? "v1",
      })
      .then((lease) => {
        if (cancelled) return
        const session: SessionLease = {
          workItemId: lease.workItemId,
        }
        leaseRef.current = session
        setActiveLease(session)
      })
      .catch(() => {
        /* keep unclaimed */
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- claim on task switch
  }, [item?.workItem?.workItemId])

  React.useEffect(() => {
    if (lastResult) resultRef.current?.focus()
    else if (item) headingRef.current?.focus()
  }, [item?.identity.id, lastResult?.status])

  const goToItem = React.useCallback(
    (next: IntegrationResolutionItemView | null | undefined) => {
      setLastResult(null)
      setActionError(null)
      if (!next) {
        replaceUrl({ taskId: null, differenceId: null })
        return
      }
      if (next.identity.itemType === "ERROR_TASK") {
        replaceUrl({
          taskId: next.identity.id,
          differenceId: null,
        })
      } else {
        replaceUrl({
          differenceId: next.identity.id,
          taskId: null,
        })
      }
    },
    [replaceUrl]
  )

  const neighbor = React.useCallback(
    (delta: number) => {
      const idx = currentIndex + delta
      if (idx < 0 || idx >= items.length) return null
      return items[idx] ?? null
    },
    [currentIndex, items]
  )

  const ensureLease = React.useCallback(async (): Promise<SessionLease> => {
    if (!item?.workItem) throw new Error("当前项无关联任务")
    const existing = leaseRef.current
    if (existing && existing.workItemId === item.workItem.workItemId) {
      return existing
    }
    const lease = await claimMutation.mutateAsync({
      workItemId: item.workItem.workItemId,
      subjectVersion: item.workItem.subjectVersion ?? "v1",
    })
    const session: SessionLease = {
      workItemId: lease.workItemId,
    }
    leaseRef.current = session
    setActiveLease(session)
    return session
  }, [claimMutation, item])

  const afterResult = React.useCallback(
    (result: IntegrationFormalResult) => {
      setLastResult(result)
      if (
        result.terminal &&
        !result.stayOnItem &&
        autoNext &&
        result.status === "succeeded"
      ) {
        const next = neighbor(1) ?? neighbor(-1)
        if (next) {
          window.setTimeout(() => goToItem(next), 400)
        }
      }
    },
    [autoNext, goToItem, neighbor]
  )

  const runTaskAction = async (
    kind:
      | "QUERY_ORIGINAL_RESULT"
      | "REPLAY_ORIGINAL"
      | "REATTRIBUTE"
      | "LINK_COMPENSATION"
      | "ADD_EVIDENCE"
      | "SKIP"
      | "DEFER"
  ) => {
    if (!item?.workItem) return
    try {
      await ensureLease()
      const result = await actionMutation.mutateAsync({
        itemType: item.identity.itemType,
        itemId: item.identity.id,
        workItemId: item.workItem.workItemId,
        expectedSubjectVersion: item.workItem.subjectVersion,
        expectedWorkItemVersion: item.workItem.workItemVersion,
        kind,
        operationId: newKey("op"),
        forceUnknown: kind === "QUERY_ORIGINAL_RESULT" ? forceUnknownOnce : false,
        comment: comment || undefined,
        evidenceRefs:
          kind === "ADD_EVIDENCE" || kind === "LINK_COMPENSATION"
            ? [
                {
                  kind:
                    kind === "LINK_COMPENSATION"
                      ? "COMPENSATION_RESULT"
                      : "BUSINESS_OBJECT_VERIFICATION",
                  recordId: newKey("evd"),
                  label:
                    kind === "LINK_COMPENSATION"
                      ? "补偿结果"
                      : "业务对象核验",
                },
              ]
            : undefined,
      })
      if (kind === "QUERY_ORIGINAL_RESULT") setForceUnknownOnce(false)
      if (kind === "DEFER") {
        leaseRef.current = null
        setActiveLease(null)
      }
      afterResult(result)
      if (kind === "SKIP" && !result.stayOnItem && result.status === "succeeded") {
        const next = neighbor(1)
        if (next) goToItem(next)
      }
    } catch (e) {
      setActionError(e instanceof Error ? e.message : "动作失败")
    }
  }

  const leaseActive =
    Boolean(item?.workItem) &&
    activeLease?.workItemId === item?.workItem?.workItemId

  const leaseStatus = !item?.workItem
    ? item && !item.hasWorkItem
      ? ("active" as const) // direct recon: no lease needed
      : ("unclaimed" as const)
    : leaseActive
      ? ("active" as const)
      : ("unclaimed" as const)

  const formalPending =
    actionMutation.isPending ||
    resolveMutation.isPending ||
    closeMutation.isPending ||
    transferMutation.isPending ||
    directMutation.isPending

  const can = (action: string) =>
    Boolean(item?.allowedActions.includes(action as never))

  const returnRefresh = () => {
    void queueQuery.refetch()
    setLastResult(null)
  }

  if (queueQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-16 animate-pulse rounded-2xl bg-muted" />
        <div className="grid gap-4 xl:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]">
          <div className="h-80 animate-pulse rounded-2xl bg-muted" />
          <div className="h-80 animate-pulse rounded-2xl bg-muted" />
        </div>
      </div>
    )
  }

  if (queueQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="接口错误与对账中心" description="加载失败" />
        <Button type="button" onClick={() => void queueQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  const panelErrorClass =
    item && isPanelErrorClass(item.classification.errorClass)
      ? item.classification.errorClass
      : null

  const buildResolutionActions = (): InterfaceErrorResolutionActions => {
    if (!item || !panelErrorClass) return { stage: "none" }

    if (panelErrorClass === "result-unknown" || panelErrorClass === "network-timeout") {
      if (can("REPLAY_ORIGINAL")) {
        return {
          stage: "safe-retry" as const,
          queryResult: "confirmed-no-result" as const,
          retrySameKey: (
            <Button
              type="button"
              size="sm"
              disabled={!leaseActive || formalPending}
              onClick={() => void runTaskAction("REPLAY_ORIGINAL")}
            >
              重新提交（保持原请求编号）
            </Button>
          ),
        }
      }
      return {
        stage: "query-original" as const,
        queryOriginal: (
          <Button
            type="button"
            size="sm"
            disabled={!leaseActive || formalPending || !can("QUERY_ORIGINAL_RESULT")}
            onClick={() => void runTaskAction("QUERY_ORIGINAL_RESULT")}
          >
            <SearchIcon data-icon="inline-start" aria-hidden />
            查询原结果
          </Button>
        ),
      }
    }

    if (
      panelErrorClass === "parameter-or-mapping" ||
      panelErrorClass === "business-rejected" ||
      panelErrorClass === "authentication-or-signature" ||
      panelErrorClass === "capability-unsupported" ||
      panelErrorClass === "out-of-order-callback"
    ) {
      return {
        stage: "manual" as const,
        manual: (
          <div className="flex flex-wrap gap-2">
            {can("TRANSFER") ? (
              <Button
                type="button"
                size="sm"
                variant="secondary"
                disabled={!leaseActive || formalPending}
                onClick={() => void handleTransfer()}
              >
                转交
              </Button>
            ) : null}
            {can("LINK_COMPENSATION") ? (
              <Button
                type="button"
                size="sm"
                disabled={!leaseActive || formalPending}
                onClick={() => void runTaskAction("LINK_COMPENSATION")}
              >
                关联补偿
              </Button>
            ) : null}
            {item.repairLinks[0] ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={<Link href={item.repairLinks[0].href} />}
              >
                去修复
              </Button>
            ) : null}
          </div>
        ),
      }
    }

    if (panelErrorClass === "duplicate-callback" && can("CLOSE_DUPLICATE")) {
      return {
        stage: "closable" as const,
        terminalBasis: "verified-terminal" as const,
        terminalEvidence: "已关联原消费记录核验",
        close: (
          <Button
            type="button"
            size="sm"
            disabled={!leaseActive || formalPending}
            onClick={() => void handleClose("CLOSE_DUPLICATE")}
          >
            关闭重复
          </Button>
        ),
      }
    }

    if (panelErrorClass === "rate-limited") {
      return {
        stage: "manual" as const,
        manual: (
          <Button type="button" size="sm" variant="secondary" disabled>
            等待退避（禁止高频重试）
          </Button>
        ),
      }
    }

    return { stage: "none" as const }
  }

  async function handleTransfer() {
    if (!item?.workItem) return
    try {
      await ensureLease()
      const result = await transferMutation.mutateAsync({
        itemType: item.identity.itemType,
        itemId: item.identity.id,
        workItemId: item.workItem.workItemId,
        expectedSubjectVersion: item.workItem.subjectVersion,
        expectedWorkItemVersion: item.workItem.workItemVersion,
        operationId: newKey("op"),
        targetRole: transferRole,
        reasonCode: "ROLE_MISMATCH",
        comment: comment || undefined,
      })
      afterResult(result)
    } catch (e) {
      setActionError(e instanceof Error ? e.message : "转交失败")
    }
  }

  async function handleClose(kind: "CLOSE_DUPLICATE" | "CLOSE_MISROUTED") {
    if (!item?.workItem) return
    try {
      await ensureLease()
      const result = await closeMutation.mutateAsync({
        itemType: item.identity.itemType,
        itemId: item.identity.id,
        workItemId: item.workItem.workItemId,
        expectedSubjectVersion: item.workItem.subjectVersion,
        expectedWorkItemVersion: item.workItem.workItemVersion,
        operationId: newKey("op"),
        kind,
        reasonCode: kind === "CLOSE_DUPLICATE" ? "DUPLICATE" : "MISROUTED",
        replacementWorkItemId:
          kind === "CLOSE_DUPLICATE" ? replacementWi : undefined,
        comment: comment || undefined,
      })
      afterResult(result)
    } catch (e) {
      setActionError(e instanceof Error ? e.message : "关闭失败")
    }
  }

  async function handleResolve() {
    if (!item?.workItem || !item.resolutionEvidencePolicy) return
    try {
      await ensureLease()
      const evidence =
        item.linkedEvidence.length > 0
          ? item.linkedEvidence
          : item.resolutionEvidencePolicy.requiredEvidenceKinds.map((k) => ({
              kind: k,
              recordId: newKey("pol"),
              label: EVIDENCE_KIND_LABEL[k],
            }))
      // ensure all required kinds present for mock resolve attempt
      const kinds = new Set(evidence.map((e) => e.kind))
      for (const k of item.resolutionEvidencePolicy.requiredEvidenceKinds) {
        if (!kinds.has(k)) {
          evidence.push({
            kind: k,
            recordId: newKey("pol"),
            label: EVIDENCE_KIND_LABEL[k],
          })
        }
      }
      // pre-link so server gate opens
      if (!can("RESOLVE")) {
        await actionMutation.mutateAsync({
          itemType: item.identity.itemType,
          itemId: item.identity.id,
          workItemId: item.workItem.workItemId,
          expectedSubjectVersion: item.workItem.subjectVersion,
          expectedWorkItemVersion: item.workItem.workItemVersion,
          kind: "ADD_EVIDENCE",
          operationId: newKey("op"),
          evidenceRefs: evidence,
        })
      }
      await ensureLease()
      const result = await resolveMutation.mutateAsync({
        itemType: item.identity.itemType,
        itemId: item.identity.id,
        workItemId: item.workItem.workItemId,
        expectedSubjectVersion: item.workItem.subjectVersion,
        expectedWorkItemVersion: item.workItem.workItemVersion,
        operationId: newKey("op"),
        evidencePolicyId: item.resolutionEvidencePolicy.evidencePolicyId,
        evidencePolicyVersion:
          item.resolutionEvidencePolicy.evidencePolicyVersion,
        policyKey: item.resolutionEvidencePolicy.key,
        evidenceRefs: evidence as [
          (typeof evidence)[number],
          ...(typeof evidence)[number][],
        ],
        comment: comment || undefined,
      })
      afterResult(result)
    } catch (e) {
      setActionError(e instanceof Error ? e.message : "解决失败")
    }
  }

  async function handleDirectTerminal(
    conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE"
  ) {
    if (!item || item.hasWorkItem) return
    const reg = item.reconciliationReasonRegistry
    if (!reg) return
    const reason =
      reg.registeredReasons.find((r) => r.registeredReasonId === reconReasonId) ??
      reg.registeredReasons.find((r) => r.conclusion === conclusion)
    if (!reason || reason.conclusion !== conclusion) {
      setActionError("请选择与结论匹配的注册原因")
      return
    }
    try {
      const evidence = reason.requiredEvidenceKinds.map((k) => ({
        kind: k,
        recordId: newKey("fin"),
        label: EVIDENCE_KIND_LABEL[k],
      }))
      const result = await directMutation.mutateAsync({
        differenceId: item.identity.id,
        expectedDifferenceVersion: item.objectVersion,
        expectedSubjectHash: item.identity.subjectHash,
        operationId: newKey("op"),
        decision: {
          kind: "TERMINAL_CONCLUSION",
          reasonRegistryId: reg.reasonRegistryId,
          reasonRegistryVersion: reg.reasonRegistryVersion,
          registeredReasonId: reason.registeredReasonId,
          conclusion,
          evidenceRefs: evidence as [
            (typeof evidence)[number],
            ...(typeof evidence)[number][],
          ],
          comment: comment || undefined,
        },
      })
      afterResult(result)
    } catch (e) {
      setActionError(e instanceof Error ? e.message : "对账确认失败")
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="接口错误与对账中心"
        breadcrumbs={[
          {
            id: "gov",
            label: "治理",
            href: "/governance/integration-errors",
          },
          {
            id: "ie",
            label: focusMode
              ? item?.identity.number ?? "详情"
              : "接口错误与对账",
            current: true,
          },
        ]}
        metadata={
          <DataFreshness
            state="fresh"
            label="集成更新时间"
            updatedAt={formatDateTime(view?.context.updatedAt, "default")}
            dateTime={view?.context.updatedAt}
          />
        }
      />

      {metrics ? (
        <MetricStrip>
          <MetricItem
            label="结果未知"
            value={metrics.resultUnknown}
            onClick={() =>
              replaceUrl({
                view: "result_unknown",
                taskId: null,
                differenceId: null,
              })
            }
          />
          <MetricItem label="待人工" value={metrics.manualRequired} />
          <MetricItem
            label="安全故障"
            value={metrics.securityFaults}
            onClick={() =>
              replaceUrl({
                view: "security",
                taskId: null,
                differenceId: null,
              })
            }
          />
          <MetricItem
            label="未解决差异"
            value={metrics.openDifferences}
            onClick={() =>
              replaceUrl({
                view: "reconciliation",
                mode: "reconciliation",
                taskId: null,
                differenceId: null,
              })
            }
          />
          <MetricItem label="最长滞留" value={metrics.longestAgeLabel} />
        </MetricStrip>
      ) : null}

      {!focusMode ? (
        <div className="flex flex-wrap items-center gap-2">
          <ToggleGroup
            value={[urlState.view]}
            onValueChange={(v) => {
              const next = (v[0] as IntegrationView | undefined) ?? "mine"
              replaceUrl({
                view: next,
                taskId: null,
                differenceId: null,
              })
            }}
            variant="outline"
            size="sm"
            spacing={0}
            aria-label="保存的视图"
          >
            {(Object.keys(VIEW_LABEL) as IntegrationView[]).map((v) => (
              <ToggleGroupItem key={v} value={v}>
                {VIEW_LABEL[v]}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>

          <OptionCombobox
            value={urlState.mode}
            onValueChange={(v) =>
              replaceUrl({
                mode: v ?? "all",
                taskId: null,
                differenceId: null,
              })
            }
            options={(
              Object.keys(MODE_LABEL) as (keyof typeof MODE_LABEL)[]
            ).map((m) => ({ value: m, label: MODE_LABEL[m] }))}
            className="w-[8rem]"
            size="sm"
            aria-label="模式"
            allowClear={false}
          />

          <OptionCombobox
            value={urlState.environment}
            onValueChange={(v) =>
              replaceUrl({
                environment: v ?? "production",
                taskId: null,
                differenceId: null,
              })
            }
            options={(Object.keys(ENV_LABEL) as (keyof typeof ENV_LABEL)[]).map(
              (e) => ({ value: e, label: ENV_LABEL[e] })
            )}
            className="w-[7rem]"
            size="sm"
            aria-label="环境"
            allowClear={false}
          />

          <OptionCombobox
            value={urlState.errorClass ?? "all"}
            onValueChange={(v) =>
              replaceUrl({
                errorClass: !v || v === "all" ? null : v,
                taskId: null,
                differenceId: null,
              })
            }
            options={[
              { value: "all", label: "全部类别" },
              ...Object.entries(ERROR_CLASS_LABEL).map(([k, label]) => ({
                value: k,
                label,
              })),
            ]}
            className="w-[10rem]"
            size="sm"
            aria-label="错误类别"
            placeholder="错误类别"
            allowClear={false}
          />

          <OptionCombobox
            value={urlState.owner}
            onValueChange={(v) =>
              replaceUrl({
                owner: v ?? "me",
                taskId: null,
                differenceId: null,
              })
            }
            options={(
              Object.keys(OWNER_LABEL) as (keyof typeof OWNER_LABEL)[]
            ).map((o) => ({ value: o, label: OWNER_LABEL[o] }))}
            className="w-[8rem]"
            size="sm"
            aria-label="责任人"
            allowClear={false}
          />

          <div className="flex items-center gap-2">
            <Label htmlFor="w29-auto-next" className="text-xs text-muted-foreground">
              自动下一项
            </Label>
            <Switch
              id="w29-auto-next"
              checked={autoNext}
              onCheckedChange={(on) => {
                setSessionAutoNext(on)
                replaceUrl({ autoNext: on ? "1" : "0" })
              }}
            />
          </div>

          <form
            className="ml-auto flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault()
              replaceUrl({
                q: searchDraft.trim() || null,
                taskId: null,
                differenceId: null,
              })
            }}
          >
            <Input
              value={searchDraft}
              onChange={(e) => setSearchDraft(e.target.value)}
              placeholder="任务号 / 业务单号 / 事件摘要"
              className="h-8 w-52"
              aria-label="搜索"
            />
            <Button type="submit" size="sm" variant="secondary">
              搜索
            </Button>
          </form>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant="outline"
            render={<Link href={`/governance/integration-errors?view=${urlState.view}&queueContextId=${encodeURIComponent(urlState.queueContextId)}`} />}
          >
            返回队列
          </Button>
          <Button type="button" size="sm" variant="secondary" onClick={returnRefresh}>
            <RefreshCwIcon data-icon="inline-start" aria-hidden />
            刷新当前任务
          </Button>
        </div>
      )}

      <p className="text-xs text-muted-foreground">
        筛选：{view?.context.filterSummary}
        {sessionAutoNext !== autoNext ? null : null}
      </p>

      <label className="flex items-center gap-2 text-xs text-muted-foreground">
        <input
          type="checkbox"
          className="size-3.5"
          checked={forceUnknownOnce}
          onChange={(e) => setForceUnknownOnce(e.target.checked)}
        />
        下次「查询原结果」模拟仍未知（结果仍未知 · 不自动下一项）
      </label>

      {lastResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={formalStatus(lastResult.status)}
            title={lastResult.title}
            description={lastResult.description}
            reference={lastResult.reference}
            facts={lastResult.facts}
            actions={
              lastResult.terminal && !autoNext ? (
                <Button
                  type="button"
                  size="sm"
                  onClick={() => {
                    const next = neighbor(1)
                    if (next) goToItem(next)
                  }}
                >
                  下一项
                </Button>
              ) : null
            }
          />
        </div>
      ) : null}

      {actionError ? (
        <Alert variant="destructive">
          <AlertTitle>操作失败</AlertTitle>
          <AlertDescription>{actionError}</AlertDescription>
        </Alert>
      ) : null}

      {items.length === 0 ? (
        <BusinessEmptyState
          kind="no-tasks"
          title="当前筛选项已处理完"
          description="可切换视图、清除筛选，或返回工作台。"
        />
      ) : (
        <div
          className={cn(
            "grid gap-4",
            focusMode
              ? "grid-cols-1"
              : "xl:grid-cols-[minmax(0,38fr)_minmax(0,62fr)]"
          )}
        >
          {!focusMode ? (
            <Card size="sm" className="min-h-[28rem]">
              <CardHeader className="border-b">
                <CardTitle>任务 / 差异队列</CardTitle>
                <CardDescription>
                  共 {items.length} 项 · 安全故障与结果未知优先
                </CardDescription>
              </CardHeader>
              <CardContent className="max-h-[70vh] space-y-2 overflow-y-auto pt-3">
                {items.map((row) => {
                  const selected = row.identity.id === item?.identity.id
                  const detailHref =
                    row.identity.itemType === "ERROR_TASK"
                      ? `/governance/integration-errors/errors/${row.identity.id}`
                      : `/governance/integration-errors/differences/${row.identity.id}`
                  return (
                    <button
                      key={row.identity.id}
                      type="button"
                      className={cn(
                        "w-full rounded-xl text-left transition-colors",
                        selected
                          ? "ring-2 ring-primary"
                          : "hover:bg-muted/40"
                      )}
                      onClick={() => goToItem(row)}
                    >
                      <WorkTaskItem
                        density="compact"
                        taskType={row.classification.label}
                        businessObject={row.businessObject.title}
                        counterparty={row.identity.number}
                        enteredAt={formatDateTime(row.createdAt, "default")}
                        enteredDateTime={row.createdAt}
                        dueAt={row.ageLabel}
                        responsibleParty={row.ownerUser ?? row.ownerRole}
                        reason={row.classification.code}
                        impact={row.fundsImpactLabel}
                        status={{
                          label: row.status.label,
                          tone: severityTone(row.classification.severity),
                        }}
                        nextAction={
                          <span className="flex flex-wrap items-center gap-1">
                            <Badge variant="outline">
                              {row.environmentLabel}
                            </Badge>
                            <Badge variant="outline">
                              {row.classification.severityLabel}
                            </Badge>
                            <Link
                              href={detailHref}
                              className="text-xs text-primary underline-offset-2 hover:underline"
                              onClick={(e) => e.stopPropagation()}
                            >
                              详情
                            </Link>
                          </span>
                        }
                      />
                    </button>
                  )
                })}
              </CardContent>
            </Card>
          ) : null}

          <div className="flex min-w-0 flex-col gap-3">
            {item ? (
              <>
                <SequentialProcessBar
                  current={currentIndex + 1}
                  total={items.length}
                  leaseStatus={leaseStatus}
                  leaseStatusLabel={
                    !item.hasWorkItem
                      ? "直接对账（无处理任务）"
                      : leaseActive
                        ? "已领取"
                        : "未领取"
                  }
                  processLabel="处理当前"
                  processNextLabel="下一项"
                  pending={formalPending}
                  processDisabled={leaseStatus !== "active"}
                  onBack={() => {
                    if (focusMode) {
                      router.push(
                        `/governance/integration-errors?view=${urlState.view}`
                      )
                    } else {
                      replaceUrl({ taskId: null, differenceId: null })
                    }
                  }}
                  onProcess={() => {
                    headingRef.current?.focus()
                  }}
                  onProcessNext={() => {
                    const next = neighbor(1)
                    if (next) goToItem(next)
                  }}
                  onReclaim={() => {
                    void ensureLease().catch((e) =>
                      setActionError(
                        e instanceof Error ? e.message : "领取失败"
                      )
                    )
                  }}
                />

                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    disabled={!can("DEFER") || !item.workItem || formalPending}
                    onClick={() => void runTaskAction("DEFER")}
                  >
                    <PauseIcon data-icon="inline-start" aria-hidden />
                    先跳过（保留队列）
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={!can("SKIP") || !item.workItem || formalPending}
                    onClick={() => void runTaskAction("SKIP")}
                  >
                    <SkipForwardIcon data-icon="inline-start" aria-hidden />
                    跳过
                  </Button>
                </div>

                <Card size="sm">
                  <CardHeader className="border-b">
                    <CardTitle
                      ref={headingRef}
                      tabIndex={-1}
                      className="outline-none"
                    >
                      {item.identity.number} · {item.businessObject.title}
                    </CardTitle>
                    <CardDescription>
                      {item.identity.itemType === "ERROR_TASK"
                        ? "错误任务"
                        : "对账差异"}
                      {item.workItem
                        ? " · 关联任务"
                        : " · 无关联任务（直接对账）"}
                    </CardDescription>
                    <div className="flex flex-wrap gap-2 pt-1">
                      <BusinessStatusBadge
                        context="detail"
                        label={item.classification.label}
                        tone={severityTone(item.classification.severity)}
                      />
                      <Badge variant="outline">
                        环境：{item.environmentLabel}
                      </Badge>
                      <Badge variant="outline">
                        严重度：{item.classification.severityLabel}
                      </Badge>
                      <Badge variant="outline">状态：{item.status.label}</Badge>
                      <Badge variant="outline">
                        {FUNDS_LABEL[item.fundsImpact]}
                      </Badge>
                      {item.compensationOpen ? (
                        <Badge variant="destructive">补偿未闭环</Badge>
                      ) : null}
                    </div>
                  </CardHeader>
                  <CardContent className="space-y-3 pt-4">
                    {item.classification.errorClass === "result-unknown" ? (
                      <Alert variant="destructive">
                        <ShieldAlertIcon aria-hidden />
                        <AlertTitle>结果未知</AlertTitle>
                        <AlertDescription>
                          主动作仅为「查询原结果」。禁止直接重新提交下单/取消/退款。
                          系统不得传入重复请求标识。
                        </AlertDescription>
                      </Alert>
                    ) : null}

                    {item.classification.errorClass ===
                      "authentication-or-signature" ||
                    item.classification.errorClass ===
                      "parameter-or-mapping" ||
                    item.classification.errorClass === "business-rejected" ? (
                      <Alert variant="warning">
                        <AlertTitle>
                          {item.classification.label} · 禁止无意义自动重试
                        </AlertTitle>
                        <AlertDescription>
                          页面不提供自动重试按钮；
                          {item.classification.errorClass ===
                          "authentication-or-signature"
                            ? "不展示密钥或完整签名材料。"
                            : item.classification.errorClass ===
                                "parameter-or-mapping"
                              ? "请先到供应商商品库/商城同步修复映射。"
                              : "请进入供应商订单售后/补偿路径。"}
                        </AlertDescription>
                      </Alert>
                    ) : null}

                    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                      <Fact label="滞留" value={item.ageLabel} />
                      <Fact label="责任" value={item.ownerUser ?? item.ownerRole} />
                      <Fact
                        label="方向"
                        value={item.message?.directionLabel ?? "—"}
                      />
                      {item.originalAction ? (
                        <>
                          <Fact
                            label="原动作"
                            value={item.originalAction.actionLabel}
                          />
                          <Fact
                            label="原任务号摘要"
                            value={
                              item.originalAction
                                .originalActionIdempotencyKeySummary
                            }
                            mono
                          />
                          <Fact label="请求编号" value="已保留" />
                        </>
                      ) : null}
                      {item.message ? (
                        <>
                          <Fact
                            label="事件摘要"
                            value={item.message.eventIdSummary}
                            mono
                          />
                          <Fact
                            label="载荷摘要"
                            value={item.message.maskedPayloadSummary}
                          />
                        </>
                      ) : null}
                    </div>

                    {item.difference ? (
                      <BusinessDiffPanel
                        title="对账左右证据"
                        caption={item.difference.differenceSummary}
                        changes={[
                          {
                            id: "side",
                            field: "侧别摘要",
                            before: item.difference.leftSummary,
                            after: item.difference.rightSummary,
                            note: `${item.difference.leftLabel} vs ${item.difference.rightLabel}`,
                          },
                          {
                            id: "boundary",
                            field: "边界",
                            before: item.difference.boundary,
                            after: item.difference.boundary,
                            note: "数据边界",
                          },
                          {
                            id: "watermark",
                            field: "更新时间",
                            before: formatDateTime(item.difference.watermark, "default"),
                            after: formatDateTime(item.difference.watermark, "default"),
                            note: item.difference.differenceType,
                          },
                          {
                            id: "summary",
                            field: "差异摘要",
                            before: "—",
                            after: item.difference.differenceSummary,
                            note: "只读证据，不可改数",
                          },
                        ]}
                      />
                    ) : null}

                    {item.repairLinks.length > 0 ? (
                      <div className="flex flex-wrap gap-2">
                        {item.repairLinks.map((link) => (
                          <Button
                            key={link.href}
                            type="button"
                            size="sm"
                            variant="outline"
                            render={<Link href={link.href} />}
                          >
                            <ExternalLinkIcon
                              data-icon="inline-start"
                              aria-hidden
                            />
                            {link.label}
                          </Button>
                        ))}
                        <Button
                          type="button"
                          size="sm"
                          variant="ghost"
                          onClick={returnRefresh}
                        >
                          返回后刷新当前任务
                        </Button>
                      </div>
                    ) : null}
                  </CardContent>
                </Card>

                {/* Evidence — append-only timelines */}
                <Card size="sm">
                  <CardHeader className="border-b">
                    <CardTitle>证据与尝试（追加式）</CardTitle>
                    <CardDescription>
                      消息、尝试与处理记录只追加，不提供覆盖控件
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-4 pt-4">
                    {item.attempts.length > 0 ? (
                      <div className="space-y-2">
                        <h4 className="text-sm font-medium">尝试历史</h4>
                        <ul className="space-y-2">
                          {item.attempts.map((a) => (
                            <li
                              key={`${a.attemptNumber}-${a.attemptedAt}`}
                              className="rounded-lg border bg-muted/30 px-3 py-2 text-sm"
                            >
                              <div className="font-medium">
                                第 {a.attemptNumber} 次 · {a.result}
                              </div>
                              <div className="text-xs text-muted-foreground">
                                {formatDateTime(a.attemptedAt, "default")}
                                {a.requestSummary
                                  ? ` · 请求 ${a.requestSummary}`
                                  : ""}
                                {a.responseSummary
                                  ? ` · 响应 ${a.responseSummary}`
                                  : ""}
                              </div>
                            </li>
                          ))}
                        </ul>
                      </div>
                    ) : null}

                    <div>
                      <h4 className="mb-2 text-sm font-medium">证据时间线</h4>
                      <AuditTimeline
                        entries={item.evidenceTimeline.map((e) => ({
                          id: e.id,
                          action: e.action,
                          operator: e.actor,
                          occurredAt: e.at,
                          occurredAtLabel: formatDateTime(e.at, "default"),
                          source: "证据",
                          note: e.detail,
                        }))}
                        emptyMessage="暂无证据记录"
                      />
                    </div>

                    <div>
                      <h4 className="mb-2 text-sm font-medium">处理审计</h4>
                      <AuditTimeline
                        entries={item.auditTrail.map((e) => ({
                          id: e.id,
                          action: ACTION_LABEL[e.action] ?? e.action,
                          operator: e.actor,
                          occurredAt: e.at,
                          occurredAtLabel: formatDateTime(e.at, "default"),
                          source: "处理",
                          note: e.detail,
                        }))}
                        emptyMessage="暂无审计记录"
                      />
                    </div>

                    {item.linkedEvidence.length > 0 ? (
                      <div className="space-y-1">
                        <h4 className="text-sm font-medium">已关联强类型证据</h4>
                        <ul className="text-sm">
                          {item.linkedEvidence.map((e) => (
                            <li key={e.recordId}>
                              {EVIDENCE_KIND_LABEL[e.kind]} ·{" "}
                              <span className="num font-mono">{e.recordId}</span>
                              {" · "}
                              {e.label}
                            </li>
                          ))}
                        </ul>
                      </div>
                    ) : null}

                    {item.resolutionEvidencePolicy ? (
                      <Alert variant="info">
                        <AlertTitle>解决证据策略</AlertTitle>
                        <AlertDescription>
                          {item.resolutionEvidencePolicy.evidencePolicyId}@v
                          {item.resolutionEvidencePolicy.evidencePolicyVersion}
                          {" · 需要 "}
                          {item.resolutionEvidencePolicy.requiredEvidenceKinds
                            .map((k) => EVIDENCE_KIND_LABEL[k])
                            .join("、")}
                          {" · 岗位分离 "}
                          {item.resolutionEvidencePolicy.reviewerSeparation}
                        </AlertDescription>
                      </Alert>
                    ) : item.hasWorkItem ? (
                      <Alert variant="warning">
                        <AlertTitle>解决证据规则尚未配置</AlertTitle>
                        <AlertDescription>
                          处理完成已从可操作范围排除；只允许补证、先跳过或转交。
                        </AlertDescription>
                      </Alert>
                    ) : null}
                  </CardContent>
                </Card>

                {panelErrorClass ? (
                  <InterfaceErrorResolutionPanel
                    errorClass={panelErrorClass}
                    status={mapPanelStatus(item)}
                    businessImpact={
                      <span>
                        {item.businessObject.title} · {item.fundsImpactLabel}
                        {item.compensationOpen ? " · 补偿未完成" : ""}
                      </span>
                    }
                    latestAttempt={{
                      attemptNumber: item.attempts[0]?.attemptNumber ?? 0,
                      attemptedAt: {
                        dateTime: item.attempts[0]?.attemptedAt ?? item.createdAt,
                        label: formatDateTime(
                          item.attempts[0]?.attemptedAt ?? item.createdAt,
                          "default"
                        ),
                      },
                      result: item.attempts[0]?.result ?? "尚无尝试",
                      requestSummary: item.attempts[0]?.requestSummary,
                      responseSummary: item.attempts[0]?.responseSummary,
                      nextRetryAt: item.attempts[0]?.nextRetryAt
                        ? {
                            dateTime: item.attempts[0].nextRetryAt,
                            label: formatDateTime(item.attempts[0].nextRetryAt, "default"),
                          }
                        : undefined,
                    }}
                    errorCode={item.classification.code}
                    actions={buildResolutionActions() as never}
                  />
                ) : null}

                {/* Action zone */}
                <Card size="sm">
                  <CardHeader className="border-b">
                    <CardTitle>处理动作</CardTitle>
                    <CardDescription>
                      仅展示可操作范围；阻断原因见下方说明
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="space-y-3 pt-4">
                    {item.actionBlockers.length > 0 ? (
                      <ul className="space-y-1 text-xs text-muted-foreground">
                        {item.actionBlockers.map((b) => (
                          <li key={`${b.action}-${b.code}`}>
                            <span className="font-medium text-foreground">
                              {ACTION_LABEL[b.action] ?? b.action}
                            </span>
                            ：{b.message}
                          </li>
                        ))}
                      </ul>
                    ) : null}

                    <div className="space-y-1">
                      <Label htmlFor="w29-comment">处理说明</Label>
                      <Textarea
                        id="w29-comment"
                        rows={2}
                        value={comment}
                        onChange={(e) => setComment(e.target.value)}
                        placeholder="可选说明（不覆盖业务证据）"
                      />
                    </div>

                    <div className="flex flex-wrap gap-2">
                      {can("QUERY_ORIGINAL_RESULT") && item.workItem ? (
                        <Button
                          type="button"
                          disabled={!leaseActive || formalPending}
                          onClick={() => void runTaskAction("QUERY_ORIGINAL_RESULT")}
                        >
                          查询原结果
                        </Button>
                      ) : null}
                      {can("REPLAY_ORIGINAL") && item.workItem ? (
                        <Button
                          type="button"
                          variant="secondary"
                          disabled={!leaseActive || formalPending}
                          onClick={() => void runTaskAction("REPLAY_ORIGINAL")}
                        >
                          重新提交
                        </Button>
                      ) : null}
                      {can("ADD_EVIDENCE") && item.workItem ? (
                        <Button
                          type="button"
                          variant="outline"
                          disabled={!leaseActive || formalPending}
                          onClick={() => void runTaskAction("ADD_EVIDENCE")}
                        >
                          补充证据
                        </Button>
                      ) : null}
                      {can("LINK_COMPENSATION") && item.workItem ? (
                        <Button
                          type="button"
                          variant="outline"
                          disabled={!leaseActive || formalPending}
                          onClick={() => void runTaskAction("LINK_COMPENSATION")}
                        >
                          关联补偿
                        </Button>
                      ) : null}
                      {can("REATTRIBUTE") && item.workItem ? (
                        <Button
                          type="button"
                          variant="outline"
                          disabled={!leaseActive || formalPending}
                          onClick={() => void runTaskAction("REATTRIBUTE")}
                        >
                          重新归集
                        </Button>
                      ) : null}
                      {can("TRANSFER") && item.workItem ? (
                        <div className="flex flex-wrap items-center gap-2">
                          <OptionCombobox
                            value={transferRole}
                            onValueChange={(v) =>
                              setTransferRole(v ?? "研发运维")
                            }
                            options={TRANSFER_ROLE_OPTIONS}
                            className="w-32"
                            size="sm"
                            allowClear={false}
                            aria-label="转交目标角色"
                            placeholder="目标角色"
                          />
                          <Button
                            type="button"
                            variant="secondary"
                            disabled={!leaseActive || formalPending}
                            onClick={() => void handleTransfer()}
                          >
                            转交
                          </Button>
                        </div>
                      ) : null}
                      {can("RESOLVE") && item.workItem ? (
                        <Button
                          type="button"
                          disabled={!leaseActive || formalPending}
                          onClick={() => void handleResolve()}
                        >
                          标记已解决
                        </Button>
                      ) : null}
                      {can("CLOSE_DUPLICATE") && item.workItem ? (
                        <div className="flex w-full flex-wrap items-end gap-2 rounded-lg border p-2">
                          <div className="space-y-1">
                            <Label className="text-xs">替代任务</Label>
                            <Input
                              className="h-8 w-40"
                              value={replacementWi}
                              onChange={(e) =>
                                setReplacementWi(e.target.value)
                              }
                            />
                          </div>
                          <Button
                            type="button"
                            size="sm"
                            disabled={!leaseActive || formalPending}
                            onClick={() => void handleClose("CLOSE_DUPLICATE")}
                          >
                            关闭重复
                          </Button>
                        </div>
                      ) : null}
                    </div>

                    {/* Direct reconciliation */}
                    {!item.hasWorkItem ? (
                      <div className="space-y-3 rounded-xl border border-dashed p-3">
                        <p className="text-sm font-medium">
                          直接对账（无关联任务）
                        </p>
                        <p className="text-xs text-muted-foreground">
                          终结只能「确认无误 / 确认有效差异」，引用原因注册表与受控证据；不得伪造任务已关闭。
                        </p>
                        {item.reconciliationReasonRegistry ? (
                          <>
                            <OptionCombobox
                              value={reconReasonId || null}
                              onValueChange={(v) => setReconReasonId(v ?? "")}
                              options={item.reconciliationReasonRegistry.registeredReasons.map(
                                (r) => ({
                                  value: r.registeredReasonId,
                                  label: r.label,
                                })
                              )}
                              className="w-full max-w-md"
                              size="sm"
                              aria-label="注册原因"
                              placeholder="选择注册原因"
                              allowClear={false}
                            />
                            <div className="flex flex-wrap gap-2">
                              <Button
                                type="button"
                                size="sm"
                                disabled={
                                  !can("CONFIRM_NO_ERROR") || formalPending
                                }
                                onClick={() =>
                                  void handleDirectTerminal("CONFIRM_NO_ERROR")
                                }
                              >
                                确认无误
                              </Button>
                              <Button
                                type="button"
                                size="sm"
                                variant="secondary"
                                disabled={
                                  !can("CONFIRM_VALID_DIFFERENCE") ||
                                  formalPending
                                }
                                onClick={() =>
                                  void handleDirectTerminal(
                                    "CONFIRM_VALID_DIFFERENCE"
                                  )
                                }
                              >
                                确认有效差异
                              </Button>
                              <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={formalPending}
                                onClick={() => {
                                  void directMutation
                                    .mutateAsync({
                                      differenceId: item.identity.id,
                                      expectedDifferenceVersion:
                                        item.objectVersion,
                                      expectedSubjectHash:
                                        item.identity.subjectHash,
                                      operationId: newKey("op"),
                                      decision: {
                                        kind: "NON_TERMINAL_ACTION",
                                        action: "ADD_EVIDENCE",
                                        evidenceRefs: [
                                          {
                                            kind: "FINANCIAL_RECONCILIATION",
                                            recordId: newKey("fin"),
                                            label: "补充财务对账证据",
                                          },
                                        ],
                                        comment:
                                          comment ||
                                          undefined,
                                      },
                                    })
                                    .then(afterResult)
                                }}
                              >
                                补充证据（非终结）
                              </Button>
                            </div>
                          </>
                        ) : (
                          <Alert variant="warning">
                            <AlertTitle>原因注册表未配置</AlertTitle>
                            <AlertDescription>
                              确认无误/有效差异均禁用；只能补证或转为任务。
                            </AlertDescription>
                          </Alert>
                        )}
                      </div>
                    ) : null}

                    <p className="text-xs text-muted-foreground">
                      当前可操作范围：
                      {item.allowedActions
                        .map((a) => ACTION_LABEL[a] ?? a)
                        .join("、") || "（无）"}
                    </p>
                  </CardContent>
                </Card>
              </>
            ) : (
              <BusinessEmptyState
                kind="filter"
                title="未选择处理项"
                description="从左侧队列选择任务或差异。"
              />
            )}
          </div>
        </div>
      )}
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
