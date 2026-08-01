"use client"

import * as React from "react"
import Link from "next/link"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { z } from "zod"
import {
  CheckIcon,
  HistoryIcon,
  PackageIcon,
  RotateCcwIcon,
  SaveIcon,
} from "lucide-react"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  DataFreshness,
  FormalActionConfirmDialog,
  FormalActionResult,
  MetricItem,
  MetricStrip,
  ValidationSummary,
  type ValidationIssue,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Field,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Textarea } from "@/components/ui/textarea"
import {
  demoRestoreAcceptancePermission,
  demoRevokeAcceptancePermission,
  fetchCustomerAcceptanceWorkspace,
  postCustomerAcceptanceWorkspace,
  reverseCustomerAcceptanceWorkspace,
  saveCustomerAcceptanceDraft,
} from "@/features/sales-orders/acceptance"
import {
  FACT_ONLY_NOTICE,
  FULFILLMENT_TYPE_LABEL,
  OVERALL_RESULT_LABEL,
  type AcceptanceDraftLine,
  type AcceptanceEligibleFact,
  type AcceptanceHistoryItem,
  type AcceptanceOverallResult,
  type CustomerAcceptanceWorkspaceView,
} from "@/features/sales-orders/acceptance-types"
import { salesOrderKeys } from "@/features/sales-orders/queries"
import { cn } from "@/lib/utils"

const draftHeaderSchema = z.object({
  acceptedAt: z.string().min(1, "请填写客户验收时间"),
  comment: z.string(),
})

type LineResultState = {
  acceptedQuantity: string
  shortQuantity: string
  rejectedQuantity: string
  reason: string
  /** 服务不通过：将结果写入 rejected 并标注 */
  serviceFail: boolean
}

type FormalResultState =
  | {
      kind: "post"
      status: "succeeded" | "unknown" | "failed"
      title: string
      description: string
      reference?: string
      facts: Array<{ label: string; value: string }>
    }
  | {
      kind: "reverse"
      status: "succeeded" | "failed"
      title: string
      description: string
      reference?: string
      facts: Array<{ label: string; value: string }>
    }

function parseQty(value: string): number {
  const n = Number(value)
  return Number.isFinite(n) ? n : 0
}

function todayLocalDateTimeInput(): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function formatOccurredAt(iso: string): string {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      dateStyle: "medium",
      timeStyle: "short",
      timeZone: "Asia/Shanghai",
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

function deriveOverall(
  lines: LineResultState[]
): AcceptanceOverallResult {
  let hasReject = false
  let hasShort = false
  let hasServiceFail = false
  for (const line of lines) {
    if (line.serviceFail) hasServiceFail = true
    if (parseQty(line.rejectedQuantity) > 0) hasReject = true
    if (parseQty(line.shortQuantity) > 0) hasShort = true
  }
  if (hasServiceFail) return "SERVICE_FAIL"
  if (hasReject) return "REJECT"
  if (hasShort) return "SHORT"
  return "PASS"
}

function buildDraftLines(
  selected: Map<string, { fact: AcceptanceEligibleFact; qty: string }>,
  lineResults: Map<string, LineResultState>
): AcceptanceDraftLine[] {
  const bySalesLine = new Map<
    string,
    Array<{ fulfillmentLineId: string; allocatedQuantity: string }>
  >()
  for (const [id, entry] of selected) {
    const list = bySalesLine.get(entry.fact.salesOrderLineId) ?? []
    list.push({
      fulfillmentLineId: id,
      allocatedQuantity: entry.qty,
    })
    bySalesLine.set(entry.fact.salesOrderLineId, list)
  }

  const lines: AcceptanceDraftLine[] = []
  for (const [salesOrderLineId, allocations] of bySalesLine) {
    const result = lineResults.get(salesOrderLineId) ?? {
      acceptedQuantity: "0",
      shortQuantity: "0",
      rejectedQuantity: "0",
      reason: "",
      serviceFail: false,
    }
    lines.push({
      salesOrderLineId,
      acceptedQuantity: result.acceptedQuantity || "0",
      shortQuantity: result.shortQuantity || "0",
      rejectedQuantity: result.rejectedQuantity || "0",
      reason: result.reason,
      allocations,
    })
  }
  return lines
}

function collectValidationIssues(
  selected: Map<string, { fact: AcceptanceEligibleFact; qty: string }>,
  lineResults: Map<string, LineResultState>
): ValidationIssue[] {
  const issues: ValidationIssue[] = []
  if (selected.size === 0) {
    issues.push({
      id: "no-source",
      label: "履约来源",
      message: "请至少选择一条可验收履约事实",
      targetId: "acceptance-fact-pool",
    })
    return issues
  }

  const lines = buildDraftLines(selected, lineResults)
  for (const line of lines) {
    const accepted = parseQty(line.acceptedQuantity)
    const short = parseQty(line.shortQuantity)
    const rejected = parseQty(line.rejectedQuantity)
    const total = accepted + short + rejected
    const alloc = line.allocations.reduce(
      (s, a) => s + parseQty(a.allocatedQuantity),
      0
    )
    if (total <= 0) {
      issues.push({
        id: `line-empty-${line.salesOrderLineId}`,
        label: "验收数量",
        message: "通过、短少与拒收合计须大于 0",
        targetId: `line-result-${line.salesOrderLineId}`,
      })
    }
    if (Math.abs(total - alloc) > 1e-9) {
      issues.push({
        id: `line-balance-${line.salesOrderLineId}`,
        label: "数量守恒",
        message: `结果合计 ${total} 与分配合计 ${alloc} 不一致`,
        targetId: `line-result-${line.salesOrderLineId}`,
      })
    }
    if ((short > 0 || rejected > 0) && !line.reason.trim()) {
      issues.push({
        id: `line-reason-${line.salesOrderLineId}`,
        label: "客户反馈",
        message: "短少、拒收或服务不通过时原因必填",
        targetId: `line-reason-${line.salesOrderLineId}`,
      })
    }
    for (const allocItem of line.allocations) {
      const fact = selected.get(allocItem.fulfillmentLineId)?.fact
      if (!fact) continue
      if (parseQty(allocItem.allocatedQuantity) > parseQty(fact.eligibleQuantity)) {
        issues.push({
          id: `alloc-cap-${allocItem.fulfillmentLineId}`,
          label: fact.fulfillmentNo,
          message: `分配不可超过净可验收 ${fact.eligibleQuantity} ${fact.unitCode}`,
          targetId: `alloc-qty-${allocItem.fulfillmentLineId}`,
        })
      }
      if (parseQty(allocItem.allocatedQuantity) <= 0) {
        issues.push({
          id: `alloc-zero-${allocItem.fulfillmentLineId}`,
          label: fact.fulfillmentNo,
          message: "分配数量必须大于 0",
          targetId: `alloc-qty-${allocItem.fulfillmentLineId}`,
        })
      }
    }
  }
  return issues
}

function autoFillLineResult(
  salesOrderLineId: string,
  selected: Map<string, { fact: AcceptanceEligibleFact; qty: string }>,
  prev: LineResultState | undefined
): LineResultState {
  let allocSum = 0
  for (const entry of selected.values()) {
    if (entry.fact.salesOrderLineId === salesOrderLineId) {
      allocSum += parseQty(entry.qty)
    }
  }
  if (prev && (parseQty(prev.shortQuantity) > 0 || parseQty(prev.rejectedQuantity) > 0 || prev.serviceFail)) {
    return prev
  }
  return {
    acceptedQuantity: String(allocSum),
    shortQuantity: prev?.shortQuantity ?? "0",
    rejectedQuantity: prev?.rejectedQuantity ?? "0",
    reason: prev?.reason ?? "",
    serviceFail: prev?.serviceFail ?? false,
  }
}

export function AcceptanceWorkspace({
  salesOrderId,
}: {
  salesOrderId: string
}) {
  const queryClient = useQueryClient()
  const workItemId =
    typeof window !== "undefined"
      ? new URLSearchParams(window.location.search).get("workItemId")
      : null

  const [remainingOnly, setRemainingOnly] = React.useState(true)
  const [selected, setSelected] = React.useState<
    Map<string, { fact: AcceptanceEligibleFact; qty: string }>
  >(() => new Map())
  const [lineResults, setLineResults] = React.useState<
    Map<string, LineResultState>
  >(() => new Map())
  const [confirmOpen, setConfirmOpen] = React.useState(false)
  const [reverseTarget, setReverseTarget] =
    React.useState<AcceptanceHistoryItem | null>(null)
  const [reverseReason, setReverseReason] = React.useState("")
  const [formalResult, setFormalResult] =
    React.useState<FormalResultState | null>(null)
  const [idempotencyKey, setIdempotencyKey] = React.useState(
    () => `acc-${salesOrderId}-${crypto.randomUUID()}`
  )
  const [draftSavedAt, setDraftSavedAt] = React.useState<string | null>(null)
  const [clientIssues, setClientIssues] = React.useState<ValidationIssue[]>([])
  const resultRef = React.useRef<HTMLDivElement>(null)
  const restoredDraftRef = React.useRef(false)

  const workspaceQuery = useQuery({
    queryKey: salesOrderKeys.acceptance(salesOrderId, {
      remainingOnly,
      workItemId,
    }),
    queryFn: () =>
      fetchCustomerAcceptanceWorkspace({
        salesOrderId,
        remainingOnly,
        workItemId,
      }),
  })

  const view = workspaceQuery.data

  const form = useAppForm({
    defaultValues: {
      acceptedAt: todayLocalDateTimeInput(),
      comment: "",
    },
    validators: { onChange: draftHeaderSchema },
    onSubmit: async () => {
      const issues = collectValidationIssues(selected, lineResults)
      setClientIssues(issues)
      if (issues.length > 0) return
      setConfirmOpen(true)
    },
  })

  // 恢复草稿（刷新后 session-state 仍在）
  React.useEffect(() => {
    if (!view?.draft || restoredDraftRef.current) return
    restoredDraftRef.current = true
    form.setFieldValue("acceptedAt", view.draft.acceptedAt.slice(0, 16))
    form.setFieldValue("comment", view.draft.comment)
    setDraftSavedAt(view.draft.updatedAt)

    const nextSelected = new Map<
      string,
      { fact: AcceptanceEligibleFact; qty: string }
    >()
    const nextResults = new Map<string, LineResultState>()
    const factIndex = new Map<string, AcceptanceEligibleFact>()
    for (const line of view.salesLines) {
      for (const fact of line.fulfillmentFacts) {
        factIndex.set(fact.fulfillmentLineId, fact)
      }
    }
    // 草稿中的来源可能因 remainingOnly 被隐藏——仍尝试从历史全量恢复需要重取
    for (const line of view.draft.lines) {
      nextResults.set(line.salesOrderLineId, {
        acceptedQuantity: line.acceptedQuantity,
        shortQuantity: line.shortQuantity,
        rejectedQuantity: line.rejectedQuantity,
        reason: line.reason,
        serviceFail: false,
      })
      for (const alloc of line.allocations) {
        const fact = factIndex.get(alloc.fulfillmentLineId)
        if (fact) {
          nextSelected.set(alloc.fulfillmentLineId, {
            fact,
            qty: alloc.allocatedQuantity,
          })
        }
      }
    }
    setSelected(nextSelected)
    setLineResults(nextResults)
  }, [view, form])

  React.useEffect(() => {
    if (formalResult) {
      resultRef.current?.focus()
    }
  }, [formalResult])

  // 快捷键 ⌘S 保存草稿、⌘↵ 打开确认
  React.useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      const meta = event.metaKey || event.ctrlKey
      if (!meta) return
      if (event.key === "s") {
        event.preventDefault()
        void handleSaveDraft()
      }
      if (event.key === "Enter") {
        event.preventDefault()
        void form.handleSubmit()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
     
  })

  const saveDraftMutation = useMutation({
    mutationFn: saveCustomerAcceptanceDraft,
    onSuccess: async (draft) => {
      setDraftSavedAt(draft.updatedAt)
      await queryClient.invalidateQueries({
        queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
      })
    },
  })

  const postMutation = useMutation({
    mutationFn: postCustomerAcceptanceWorkspace,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        setFormalResult({
          kind: "post",
          status: "succeeded",
          title: "客户验收已过账",
          description: `${result.factOnlyNotice} 剩余待验收 ${result.remainingEligibleCount} 批 · 约 ${result.remainingEligibleQuantityLabel}。`,
          reference: result.acceptanceNo,
          facts: [
            {
              label: "总体结果",
              value: OVERALL_RESULT_LABEL[result.overallResult],
            },
            {
              label: "剩余待验收",
              value: `${result.remainingEligibleCount} 批`,
            },
          ],
        })
        setSelected(new Map())
        setLineResults(new Map())
        form.reset()
        setIdempotencyKey(`acc-${salesOrderId}-${crypto.randomUUID()}`)
        restoredDraftRef.current = false
      } else if (result.status === "unknown") {
        setFormalResult({
          kind: "post",
          status: "unknown",
          title: "正式结果未知",
          description: `${result.message} 未确认前不关闭草稿、不按成功处理；可用同一幂等键查询。`,
          facts: [{ label: "幂等键", value: result.idempotencyKey }],
        })
      } else {
        setFormalResult({
          kind: "post",
          status: "failed",
          title: "验收过账失败",
          description: result.message,
          facts: [],
        })
      }
      await queryClient.invalidateQueries({
        queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
      })
      await queryClient.invalidateQueries({
        queryKey: salesOrderKeys.detail(salesOrderId),
      })
    },
    onError: () => {
      setFormalResult({
        kind: "post",
        status: "unknown",
        title: "正式结果未知",
        description:
          "请求超时或网络中断。不得按成功处理；请查询最终结果或使用同一幂等键重试，避免重复过账。",
        facts: [{ label: "幂等键", value: idempotencyKey }],
      })
    },
  })

  const reverseMutation = useMutation({
    mutationFn: reverseCustomerAcceptanceWorkspace,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        setFormalResult({
          kind: "reverse",
          status: "succeeded",
          title: "误录验收已冲正",
          description: `已新增反向验收事实与 REVERSE 分配；原验收 ${result.originalAcceptanceNo} 保留可追溯。`,
          reference: result.reverseAcceptanceNo,
          facts: [
            { label: "原验收单号", value: result.originalAcceptanceNo },
            { label: "冲正单号", value: result.reverseAcceptanceNo },
          ],
        })
        setReverseTarget(null)
        setReverseReason("")
      } else {
        setFormalResult({
          kind: "reverse",
          status: "failed",
          title: "冲正失败",
          description: result.message,
          facts: [],
        })
      }
      await queryClient.invalidateQueries({
        queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
      })
      await queryClient.invalidateQueries({
        queryKey: salesOrderKeys.detail(salesOrderId),
      })
    },
  })

  async function handleSaveDraft() {
    if (!view) return
    if (!view.permissions.allowedActions.includes("SAVE_DRAFT")) return
    const values = form.state.values
    const lines = buildDraftLines(selected, lineResults)
    await saveDraftMutation.mutateAsync({
      salesOrderId,
      acceptanceDraftId: view.draft?.acceptanceDraftId,
      expectedDraftVersion: view.draft?.draftVersion,
      acceptedAt: values.acceptedAt
        ? new Date(values.acceptedAt).toISOString()
        : new Date().toISOString(),
      comment: values.comment,
      lines,
    })
  }

  function toggleFact(fact: AcceptanceEligibleFact, checked: boolean) {
    setSelected((prev) => {
      const next = new Map(prev)
      if (checked) {
        next.set(fact.fulfillmentLineId, {
          fact,
          qty: fact.eligibleQuantity,
        })
      } else {
        next.delete(fact.fulfillmentLineId)
      }
      setLineResults((prevResults) => {
        const results = new Map(prevResults)
        results.set(
          fact.salesOrderLineId,
          autoFillLineResult(fact.salesOrderLineId, next, results.get(fact.salesOrderLineId))
        )
        // 清理已无分配的行
        for (const lineId of results.keys()) {
          let has = false
          for (const entry of next.values()) {
            if (entry.fact.salesOrderLineId === lineId) has = true
          }
          if (!has) results.delete(lineId)
        }
        return results
      })
      return next
    })
  }

  function setAllocQty(fulfillmentLineId: string, qty: string) {
    setSelected((prev) => {
      const entry = prev.get(fulfillmentLineId)
      if (!entry) return prev
      const next = new Map(prev)
      next.set(fulfillmentLineId, { ...entry, qty })
      setLineResults((prevResults) => {
        const results = new Map(prevResults)
        results.set(
          entry.fact.salesOrderLineId,
          autoFillLineResult(
            entry.fact.salesOrderLineId,
            next,
            results.get(entry.fact.salesOrderLineId)
          )
        )
        return results
      })
      return next
    })
  }

  function updateLineResult(
    salesOrderLineId: string,
    patch: Partial<LineResultState>
  ) {
    setLineResults((prev) => {
      const next = new Map(prev)
      const current = next.get(salesOrderLineId) ?? {
        acceptedQuantity: "0",
        shortQuantity: "0",
        rejectedQuantity: "0",
        reason: "",
        serviceFail: false,
      }
      next.set(salesOrderLineId, { ...current, ...patch })
      return next
    })
  }

  if (workspaceQuery.isPending) {
    return (
      <div
        className="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,62fr)_minmax(18rem,38fr)]"
        aria-busy="true"
        aria-label="正在加载客户验收工作区"
      >
        <Card size="sm" className="min-h-64 animate-pulse bg-muted/40" />
        <Card size="sm" className="min-h-64 animate-pulse bg-muted/40" />
      </div>
    )
  }

  if (workspaceQuery.isError || !view) {
    return (
      <BusinessEmptyState
        kind="no-data"
        title="验收工作区加载失败"
        description="无法获取可验收履约事实。请重试或返回销售单概览。"
        action={
          <Button type="button" onClick={() => void workspaceQuery.refetch()}>
            重试
          </Button>
        }
      />
    )
  }

  const canPost = view.permissions.allowedActions.includes("POST_ACCEPTANCE")
  const canSave = view.permissions.allowedActions.includes("SAVE_DRAFT")
  const canCreate = view.permissions.allowedActions.includes("CREATE_ACCEPTANCE")
  const postBlocker = view.permissions.actionBlockers.find(
    (b) => b.action === "POST_ACCEPTANCE" || b.action === "CREATE_ACCEPTANCE"
  )
  const isCard = view.salesOrder.businessType === "CARD_VOUCHER"
  const eligibleLabel =
    view.metrics.eligibleQuantityByUnit.length > 0
      ? view.metrics.eligibleQuantityByUnit
          .map((u) => `${u.quantity} ${u.unitCode}`)
          .join(" · ")
      : "0"

  const selectedLines = [...selected.values()].reduce<
    Map<string, AcceptanceEligibleFact[]>
  >((map, entry) => {
    const list = map.get(entry.fact.salesOrderLineId) ?? []
    list.push(entry.fact)
    map.set(entry.fact.salesOrderLineId, list)
    return map
  }, new Map())

  const overallPreview = deriveOverall([...lineResults.values()])
  const hasExceptionResult =
    overallPreview === "SHORT" ||
    overallPreview === "REJECT" ||
    overallPreview === "SERVICE_FAIL"

  return (
    <div className="flex min-w-0 flex-col gap-4">
      {view.workItemConfigBlocker ? (
        <Alert variant="warning" role="alert">
          <AlertTitle>任务入口未启用（fail-closed）</AlertTitle>
          <AlertDescription>{view.workItemConfigBlocker}</AlertDescription>
        </Alert>
      ) : null}

      {formalResult ? (
        <div ref={resultRef} tabIndex={-1} className="outline-none">
          <FormalActionResult
            status={
              formalResult.status === "failed"
                ? "rejected"
                : formalResult.status === "unknown"
                  ? "unknown"
                  : "succeeded"
            }
            title={formalResult.title}
            description={formalResult.description}
            reference={formalResult.reference}
            facts={formalResult.facts}
            actions={
              formalResult.status === "unknown" ? (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    setFormalResult(null)
                    setConfirmOpen(true)
                  }}
                >
                  用同一幂等键重试
                </Button>
              ) : (
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => setFormalResult(null)}
                >
                  继续登记
                </Button>
              )
            }
          />
        </div>
      ) : null}

      <div className="flex flex-wrap items-center justify-between gap-2">
        <MetricStrip columns={4} aria-label="待验收摘要（服务端净事实）">
          <MetricItem
            label="待验收批次"
            value={String(view.metrics.eligibleFulfillmentCount)}
            detail="净可验收履约事实"
          />
          <MetricItem
            label="待验收数量"
            value={eligibleLabel}
            detail="按单位分组 · 不跨单位相加"
          />
          <MetricItem
            label="履约进度投影"
            value={view.salesOrder.fulfillmentProgress}
            detail="服务端销售履约投影"
          />
          <MetricItem
            label="数据水位"
            value={
              <DataFreshness
                updatedAt="刚刚"
                dateTime={view.freshness.factsUpdatedAt}
                state={view.freshness.state}
                label="履约/验收事实"
              />
            }
          />
        </MetricStrip>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-sm">
        <span className="text-muted-foreground">履约事实筛选</span>
        <Button
          type="button"
          size="sm"
          variant={remainingOnly ? "secondary" : "ghost"}
          onClick={() => setRemainingOnly(true)}
        >
          仅待验收
        </Button>
        <Button
          type="button"
          size="sm"
          variant={!remainingOnly ? "secondary" : "ghost"}
          onClick={() => setRemainingOnly(false)}
        >
          全部历史事实
        </Button>
        <span className="ms-auto flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="text-muted-foreground"
            onClick={() => {
              demoRevokeAcceptancePermission()
              void queryClient.invalidateQueries({
                queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
              })
            }}
          >
            演示：收回权限
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="text-muted-foreground"
            onClick={() => {
              demoRestoreAcceptancePermission()
              void queryClient.invalidateQueries({
                queryKey: salesOrderKeys.acceptanceRoot(salesOrderId),
              })
            }}
          >
            演示：恢复权限
          </Button>
        </span>
      </div>

      {isCard || !canCreate ? (
        <BusinessEmptyState
          kind="no-data"
          title={isCard ? "卡券单不适用客户验收" : "当前不可登记验收"}
          description={
            postBlocker?.message ??
            "请确认业务类型与权限后从有权入口继续。"
          }
          action={
            <Button
              render={<Link href={`/sales/orders/${salesOrderId}`} />}
              variant="outline"
            >
              返回概览
            </Button>
          }
        />
      ) : view.metrics.eligibleFulfillmentCount === 0 &&
        view.history.length === 0 ? (
        <BusinessEmptyState
          kind="no-data"
          title="当前没有待验收的履约事实"
          description="请先在 W09 完成仓发、代发、电子交付或服务履约过账；也可查看历史验收。"
          action={
            <Button render={<Link href="/fulfillment" />} variant="outline">
              去履约作业
            </Button>
          }
        />
      ) : (
        <>
          {/* 1440：约 62/38；1024 以下单列 */}
          <div className="grid min-w-0 gap-4 lg:grid-cols-1 xl:grid-cols-[minmax(0,62fr)_minmax(20rem,38fr)]">
            <Card
              size="sm"
              className="min-w-0"
              id="acceptance-fact-pool"
            >
              <CardHeader className="border-b">
                <CardTitle className="flex items-center gap-2">
                  <PackageIcon className="size-4" aria-hidden="true" />
                  可验收履约事实
                </CardTitle>
                <CardDescription>
                  数量来自服务端净成功履约与净验收分配（APPLY − REVERSE），前端不按表头状态推断。
                  一次验收可勾选多个批次；同一批次可在多次验收中分摊。
                </CardDescription>
              </CardHeader>
              <CardContent className="max-h-[min(32rem,70vh)] space-y-4 overflow-y-auto pt-4">
                {view.salesLines.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    筛选条件下无履约事实。可切换「全部历史事实」或去 W09 查看。
                  </p>
                ) : (
                  view.salesLines.map((line) => (
                    <section
                      key={line.salesOrderLineId}
                      aria-labelledby={`line-h-${line.salesOrderLineId}`}
                      className="space-y-2"
                    >
                      <h3
                        id={`line-h-${line.salesOrderLineId}`}
                        className="text-sm font-semibold"
                      >
                        明细 {line.lineNo} · {line.itemSnapshot}
                      </h3>
                      <p className="text-xs text-muted-foreground">
                        销售要求 {line.requiredQuantity} {line.unitCode} ·
                        净已验收 {line.netAcceptedQuantity} {line.unitCode}
                        <span className="ms-2 text-[0.65rem] uppercase tracking-wide opacity-70">
                          来源 sales_order_line / 正式投影
                        </span>
                      </p>
                      <ul className="space-y-2" role="list">
                        {line.fulfillmentFacts.map((fact) => {
                          const eligible = parseQty(fact.eligibleQuantity)
                          const checked = selected.has(fact.fulfillmentLineId)
                          const disabled = eligible <= 0
                          return (
                            <li
                              key={fact.fulfillmentLineId}
                              className={cn(
                                "rounded-lg border border-border px-3 py-2",
                                checked && "border-primary/50 bg-primary/5"
                              )}
                            >
                              <div className="flex flex-wrap items-start gap-3">
                                <div className="flex items-center gap-2 pt-0.5">
                                  <Checkbox
                                    id={`fact-${fact.fulfillmentLineId}`}
                                    checked={checked}
                                    disabled={disabled || !canPost}
                                    onCheckedChange={(value) =>
                                      toggleFact(fact, value === true)
                                    }
                                    aria-describedby={`fact-meta-${fact.fulfillmentLineId}`}
                                  />
                                  <Label
                                    htmlFor={`fact-${fact.fulfillmentLineId}`}
                                    className="cursor-pointer font-medium"
                                  >
                                    {FULFILLMENT_TYPE_LABEL[fact.fulfillmentFactType]}{" "}
                                    <span className="num font-mono text-xs">
                                      {fact.fulfillmentNo}
                                    </span>
                                  </Label>
                                </div>
                                <div
                                  id={`fact-meta-${fact.fulfillmentLineId}`}
                                  className="min-w-0 flex-1 text-xs text-muted-foreground"
                                >
                                  <div>
                                    发生 {formatOccurredAt(fact.occurredAt)} ·
                                    有效履约 {fact.netSuccessfulQuantity}{" "}
                                    {fact.unitCode} · 已验收{" "}
                                    {fact.netAcceptedAllocatedQuantity}{" "}
                                    {fact.unitCode}
                                  </div>
                                  <div className="num mt-0.5 font-medium text-foreground">
                                    本次最多可验收 {fact.eligibleQuantity}{" "}
                                    {fact.unitCode}
                                    {disabled ? " · 已验完" : ""}
                                  </div>
                                  {fact.trackingNo ? (
                                    <div>
                                      {fact.carrier} {fact.trackingNo}
                                    </div>
                                  ) : null}
                                  <div className="mt-0.5 text-[0.65rem] uppercase tracking-wide opacity-70">
                                    来源履约事实 · 可验收量=服务端净事实
                                  </div>
                                </div>
                                {checked ? (
                                  <Field className="w-28 shrink-0">
                                    <FieldLabel
                                      htmlFor={`alloc-qty-${fact.fulfillmentLineId}`}
                                    >
                                      分配
                                    </FieldLabel>
                                    <Input
                                      id={`alloc-qty-${fact.fulfillmentLineId}`}
                                      className="num"
                                      inputMode="decimal"
                                      value={
                                        selected.get(fact.fulfillmentLineId)
                                          ?.qty ?? ""
                                      }
                                      aria-describedby={`alloc-unit-${fact.fulfillmentLineId}`}
                                      onChange={(e) =>
                                        setAllocQty(
                                          fact.fulfillmentLineId,
                                          e.target.value
                                        )
                                      }
                                    />
                                    <FieldDescription
                                      id={`alloc-unit-${fact.fulfillmentLineId}`}
                                    >
                                      {fact.unitCode}
                                    </FieldDescription>
                                  </Field>
                                ) : null}
                              </div>
                            </li>
                          )
                        })}
                      </ul>
                    </section>
                  ))
                )}
              </CardContent>
            </Card>

            <div className="space-y-4 xl:sticky xl:top-4 xl:self-start">
              <Card size="sm">
                <CardHeader className="border-b">
                  <CardTitle>本次验收</CardTitle>
                  <CardDescription>
                    销售单 {view.salesOrder.salesOrderNo} ·{" "}
                    {view.salesOrder.customerLabel}
                    <span className="ms-2 text-[0.65rem] uppercase tracking-wide opacity-70">
                      销售版本投影
                    </span>
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4 pt-4">
                  <form
                    id="acceptance-form"
                    className="space-y-4"
                    onSubmit={(e) => {
                      e.preventDefault()
                      void form.handleSubmit()
                    }}
                  >
                    <form.AppField name="acceptedAt">
                      {(field) => (
                        <field.TextField
                          label="客户验收时间"
                          type="datetime-local"
                          disabled={!canPost}
                        />
                      )}
                    </form.AppField>

                    <div className="rounded-lg border border-border bg-muted/30 px-3 py-2 text-sm">
                      <div className="text-xs text-muted-foreground">
                        总体结果（由明细约束）
                      </div>
                      <div className="mt-1 font-medium">
                        {OVERALL_RESULT_LABEL[overallPreview]}
                        {selected.size > 0
                          ? ` · 已选 ${selected.size} 个履约批次`
                          : " · 尚未选择来源"}
                      </div>
                    </div>

                    {[...selectedLines.entries()].map(([lineId, facts]) => {
                      const result = lineResults.get(lineId) ?? {
                        acceptedQuantity: "0",
                        shortQuantity: "0",
                        rejectedQuantity: "0",
                        reason: "",
                        serviceFail: false,
                      }
                      const unit = facts[0]?.unitCode ?? ""
                      const hasService = facts.some(
                        (f) => f.fulfillmentFactType === "SERVICE"
                      )
                      return (
                        <fieldset
                          key={lineId}
                          id={`line-result-${lineId}`}
                          className="space-y-3 rounded-lg border border-border p-3"
                        >
                          <legend className="px-1 text-sm font-semibold">
                            {facts[0]?.itemSnapshot ?? lineId}
                          </legend>
                          <p className="text-xs text-muted-foreground">
                            来源分配：
                            {facts
                              .map(
                                (f) =>
                                  `${f.fulfillmentNo} ${selected.get(f.fulfillmentLineId)?.qty ?? 0}`
                              )
                              .join(" · ")}
                          </p>
                          <div className="grid gap-3 sm:grid-cols-3">
                            <Field>
                              <FieldLabel htmlFor={`acc-${lineId}`}>
                                通过数量
                              </FieldLabel>
                              <Input
                                id={`acc-${lineId}`}
                                className="num"
                                inputMode="decimal"
                                value={result.acceptedQuantity}
                                disabled={!canPost}
                                onChange={(e) =>
                                  updateLineResult(lineId, {
                                    acceptedQuantity: e.target.value,
                                  })
                                }
                              />
                              <FieldDescription>{unit}</FieldDescription>
                            </Field>
                            <Field>
                              <FieldLabel htmlFor={`short-${lineId}`}>
                                短少数量
                              </FieldLabel>
                              <Input
                                id={`short-${lineId}`}
                                className="num"
                                inputMode="decimal"
                                value={result.shortQuantity}
                                disabled={!canPost}
                                onChange={(e) =>
                                  updateLineResult(lineId, {
                                    shortQuantity: e.target.value,
                                  })
                                }
                              />
                            </Field>
                            <Field>
                              <FieldLabel htmlFor={`rej-${lineId}`}>
                                {hasService ? "拒收/不通过" : "拒收数量"}
                              </FieldLabel>
                              <Input
                                id={`rej-${lineId}`}
                                className="num"
                                inputMode="decimal"
                                value={result.rejectedQuantity}
                                disabled={!canPost}
                                onChange={(e) =>
                                  updateLineResult(lineId, {
                                    rejectedQuantity: e.target.value,
                                  })
                                }
                              />
                            </Field>
                          </div>
                          {hasService ? (
                            <label className="flex items-center gap-2 text-sm">
                              <Checkbox
                                checked={result.serviceFail}
                                disabled={!canPost}
                                onCheckedChange={(v) =>
                                  updateLineResult(lineId, {
                                    serviceFail: v === true,
                                  })
                                }
                              />
                              标记为服务不通过
                            </label>
                          ) : null}
                          <Field>
                            <FieldLabel htmlFor={`line-reason-${lineId}`}>
                              客户反馈 / 原因
                            </FieldLabel>
                            <Textarea
                              id={`line-reason-${lineId}`}
                              rows={2}
                              value={result.reason}
                              disabled={!canPost}
                              placeholder="短少、拒收或服务不通过时必填"
                              onChange={(e) =>
                                updateLineResult(lineId, {
                                  reason: e.target.value,
                                })
                              }
                            />
                          </Field>
                        </fieldset>
                      )
                    })}

                    {hasExceptionResult ? (
                      <Alert variant="warning" role="status">
                        <AlertTitle>
                          {OVERALL_RESULT_LABEL[overallPreview]}：仅记录验收事实
                        </AlertTitle>
                        <AlertDescription>{FACT_ONLY_NOTICE}</AlertDescription>
                      </Alert>
                    ) : null}

                    <form.AppField name="comment">
                      {(field) => (
                        <field.TextareaField
                          label="内部备注"
                          placeholder="不写联系人手机号等无关敏感信息"
                          rows={2}
                          disabled={!canPost}
                        />
                      )}
                    </form.AppField>
                  </form>

                  {clientIssues.length > 0 ? (
                    <ValidationSummary
                      issues={clientIssues}
                      title={`提交前请处理 ${clientIssues.length} 项`}
                    />
                  ) : null}

                  {postBlocker ? (
                    <p className="text-sm text-destructive" role="alert">
                      {postBlocker.message}
                    </p>
                  ) : null}

                  {draftSavedAt ? (
                    <p className="text-xs text-muted-foreground" aria-live="polite">
                      草稿已保存 ·{" "}
                      {formatOccurredAt(draftSavedAt)}
                      {view.draft
                        ? ` · v${view.draft.draftVersion}`
                        : null}
                    </p>
                  ) : null}
                </CardContent>
                <CardFooter className="flex flex-wrap justify-end gap-2 border-t">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!canSave || saveDraftMutation.isPending}
                    onClick={() => void handleSaveDraft()}
                  >
                    <SaveIcon data-icon="inline-start" aria-hidden="true" />
                    {saveDraftMutation.isPending ? "保存中…" : "保存草稿"}
                  </Button>
                  <Button
                    type="submit"
                    form="acceptance-form"
                    size="sm"
                    disabled={!canPost || postMutation.isPending}
                  >
                    <CheckIcon data-icon="inline-start" aria-hidden="true" />
                    确认并过账验收
                  </Button>
                </CardFooter>
              </Card>

              <Card size="sm">
                <CardHeader className="border-b">
                  <CardTitle className="flex items-center gap-2 text-base">
                    <HistoryIcon className="size-4" aria-hidden="true" />
                    验收历史
                  </CardTitle>
                  <CardDescription>
                    已过账不可编辑；误录通过新反向事实与 REVERSE 分配纠正。
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                  {view.history.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                      暂无历史过账记录。
                    </p>
                  ) : (
                    <ul className="space-y-3" role="list">
                      {view.history.map((item) => (
                        <li
                          key={item.acceptanceId}
                          className="rounded-lg border border-border px-3 py-2 text-sm"
                        >
                          <div className="flex flex-wrap items-center justify-between gap-2">
                            <div>
                              <span className="num font-mono font-medium">
                                {item.acceptanceNo}
                              </span>
                              <BusinessStatusBadge
                                context="preview"
                                label={
                                  item.reversalOfAcceptanceId
                                    ? "冲正事实"
                                    : item.status === "REVERSED"
                                      ? "已冲正"
                                      : "已过账"
                                }
                                tone={
                                  item.status === "REVERSED"
                                    ? "void"
                                    : item.reversalOfAcceptanceId
                                      ? "warning"
                                      : "success"
                                }
                                className="ms-2"
                              />
                            </div>
                            <span className="text-xs text-muted-foreground">
                              {OVERALL_RESULT_LABEL[item.overallResult]}
                            </span>
                          </div>
                          <p className="mt-1 text-xs text-muted-foreground">
                            {item.lines
                              .map(
                                (l) =>
                                  `通过 ${l.acceptedQuantity} / 短少 ${l.shortQuantity} / 拒收 ${l.rejectedQuantity}`
                              )
                              .join("；")}
                          </p>
                          {(item.overallResult === "SHORT" ||
                            item.overallResult === "REJECT" ||
                            item.overallResult === "SERVICE_FAIL") &&
                          !item.reversalOfAcceptanceId ? (
                            <p className="mt-1 text-xs text-warning-soft-foreground">
                              {item.factOnlyNotice}
                            </p>
                          ) : null}
                          {item.status === "POSTED" &&
                          !item.reversalOfAcceptanceId &&
                          view.permissions.allowedActions.includes(
                            "REVERSE_ACCEPTANCE"
                          ) ? (
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              className="mt-2"
                              onClick={() => {
                                setReverseTarget(item)
                                setReverseReason("")
                              }}
                            >
                              <RotateCcwIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                              />
                              冲正误录
                            </Button>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  )}
                </CardContent>
              </Card>
            </div>
          </div>

          {/* 底栏：校验与主动作（桌面同屏） */}
          <div className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-border bg-card/95 px-4 py-3 shadow-sm backdrop-blur supports-backdrop-filter:bg-card/80">
            <p className="text-sm text-muted-foreground">
              {view.salesOrder.salesOrderNo} · 已选 {selected.size} 个来源 ·
              结果 {OVERALL_RESULT_LABEL[overallPreview]}
              {hasExceptionResult ? " · 仅记录事实" : ""}
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                render={
                  <Link href={`/sales/orders/${salesOrderId}`} />
                }
              >
                退出登记
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={!canSave || saveDraftMutation.isPending}
                onClick={() => void handleSaveDraft()}
              >
                保存草稿
              </Button>
              <Button
                type="submit"
                form="acceptance-form"
                size="sm"
                disabled={!canPost || postMutation.isPending}
              >
                确认并过账验收
              </Button>
            </div>
          </div>
        </>
      )}

      <FormalActionConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title="确认过账客户验收"
        actionLabel="过账验收"
        confirmLabel="确认过账"
        fromStatus={{ label: "草稿", tone: "warning" }}
        toStatus={{
          label: OVERALL_RESULT_LABEL[overallPreview],
          tone:
            overallPreview === "PASS"
              ? "success"
              : overallPreview === "SHORT"
                ? "warning"
                : "destructive",
        }}
        lockedFields={[
          "履约事实版本",
          "净可验收量（服务端）",
          "销售单 lockVersion",
        ]}
        effects={[
          "形成 customer_acceptance / 验收行",
          "写入 acceptance_fulfillment_allocation（APPLY）",
          "更新销售履约投影",
          ...(hasExceptionResult
            ? ["不扣库存、不改应收、不自动退货（仅验收事实）"]
            : []),
        ]}
        nextDepartment={
          hasExceptionResult ? "变更与异常 / 销售协同" : "销售与财务协同"
        }
        onConfirm={async () => {
          if (!view) return
          // 先确保草稿版本
          const values = form.state.values
          const lines = buildDraftLines(selected, lineResults)
          const draft = await saveDraftMutation.mutateAsync({
            salesOrderId,
            acceptanceDraftId: view.draft?.acceptanceDraftId,
            expectedDraftVersion: view.draft?.draftVersion,
            acceptedAt: values.acceptedAt
              ? new Date(values.acceptedAt).toISOString()
              : new Date().toISOString(),
            comment: values.comment,
            lines,
          })
          await postMutation.mutateAsync({
            salesOrderId,
            acceptanceDraftId: draft.acceptanceDraftId,
            expectedDraftVersion: draft.draftVersion,
            expectedSalesOrderLockVersion: view.salesOrder.lockVersion,
            idempotencyKey,
            acceptedAt: draft.acceptedAt,
            comment: draft.comment,
            lines: draft.lines,
          })
        }}
      />

      <FormalActionConfirmDialog
        open={Boolean(reverseTarget)}
        onOpenChange={(open) => {
          if (!open) {
            setReverseTarget(null)
            setReverseReason("")
          }
        }}
        title="确认冲正误录验收"
        actionLabel="冲正"
        confirmLabel="确认冲正"
        fromStatus={{ label: "已过账", tone: "success" }}
        toStatus={{ label: "已冲正+反向事实", tone: "warning" }}
        lockedFields={["原验收单号", "原 APPLY 分配"]}
        effects={[
          "新增反向验收事实",
          "写入 REVERSE 分配（不修改原行）",
          "恢复对应履约批次净可验收量",
        ]}
        nextDepartment="销售"
        onConfirm={async () => {
          if (!reverseTarget) return
          if (!reverseReason.trim()) {
            setFormalResult({
              kind: "reverse",
              status: "failed",
              title: "冲正失败",
              description: "请填写冲正理由",
              facts: [],
            })
            return
          }
          await reverseMutation.mutateAsync({
            salesOrderId,
            acceptanceId: reverseTarget.acceptanceId,
            expectedAcceptanceVersion: reverseTarget.version,
            reasonText: reverseReason.trim(),
            idempotencyKey: `rev-${reverseTarget.acceptanceId}-${crypto.randomUUID()}`,
          })
        }}
      />

      {reverseTarget ? (
        <Card size="sm" className="border-warning-border">
          <CardHeader className="border-b">
            <CardTitle>冲正理由 · {reverseTarget.acceptanceNo}</CardTitle>
            <CardDescription>
              冲正将新增反向事实，不会删除或改写原验收行。
            </CardDescription>
          </CardHeader>
          <CardContent className="pt-4">
            <Field>
              <FieldLabel htmlFor="reverse-reason">冲正理由</FieldLabel>
              <Textarea
                id="reverse-reason"
                rows={3}
                value={reverseReason}
                onChange={(e) => setReverseReason(e.target.value)}
                placeholder="说明误录原因，供审计追溯"
              />
            </Field>
          </CardContent>
        </Card>
      ) : null}
    </div>
  )
}

