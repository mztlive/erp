"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { z } from "zod"
import {
  ArrowLeftIcon,
  FilePenLineIcon,
  ShieldCheckIcon,
} from "lucide-react"

import {
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  DocumentHeader,
  DocumentSection,
  DocumentTotals,
  FormalActionConfirmDialog,
  FormalActionResult,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
  PrepaymentGate,
  QuantityValue,
  StatusTrackSummary,
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
import {
  DescriptionDetails,
  DescriptionItem,
  DescriptionList,
  DescriptionTerm,
} from "@/components/ui/description-list"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  useAcquireDraftTokenMutation,
  usePurchaseOrderCenterQuery,
  useReviewPurchaseOrderMutation,
  useSavePurchaseOrderDraftMutation,
  useStartPurchaseChangeMutation,
  useSubmitPurchaseOrderMutation,
} from "@/features/purchase-orders/queries"
import type { ViewerRole } from "@/features/purchase-orders/types"
import {
  FULFILLMENT_RESPONSIBILITY_LABEL,
  PAYMENT_TERM_OPTIONS,
  PURCHASE_TYPE_LABEL,
  REJECT_REASON_LABEL,
} from "@/features/purchase-orders/types"
import { leaseText } from "@/lib/ui-text"

type SectionId =
  | "overview"
  | "lines"
  | "fulfillment"
  | "payable"
  | "changes"
  | "audit"

type PageMode = "view" | "edit" | "review"

function resolveSection(section?: string): SectionId {
  if (
    section === "lines" ||
    section === "fulfillment" ||
    section === "payable" ||
    section === "changes" ||
    section === "audit"
  ) {
    return section
  }
  return "overview"
}

function resolveMode(mode?: string): PageMode {
  if (mode === "edit" || mode === "review") return mode
  return "view"
}

const draftSchema = z.object({
  paymentTermCode: z.string().min(1),
  note: z.string(),
})

const reviewSchema = z.object({
  reasonCode: z.string().min(1, "请选择驳回原因"),
  comment: z.string().trim().min(2, "请填写说明"),
})

/** demo 审核经办人固定为财务 · 周敏；岗位分离判定使用真实提交人字段 */
const W08_REVIEWER_NAME = "周敏"

const VIEWER_ROLES: readonly ViewerRole[] = [
  "procurement",
  "finance",
  "sales",
  "warehouse",
]

const positiveDecimal = (value: string | undefined) =>
  value === undefined ||
  value === "" ||
  (/^\d+(?:\.\d+)?$/.test(value) && Number(value) > 0)

const taxRateValid = (value: string) =>
  value === "" ||
  (/^\d+(?:\.\d+)?$/.test(value) && Number(value) > 0 && Number(value) < 1)

export function PurchaseOrderDetailPage({
  purchaseOrderId,
  section,
  mode: modeParam,
  demoRole: demoRoleParam,
  maskCost: maskCostParam,
}: {
  purchaseOrderId: string
  section?: string
  mode?: string
  demoRole?: string
  maskCost?: string
}) {
  const router = useRouter()
  const [viewerRole] = React.useState<ViewerRole>(() => {
    if (maskCostParam === "1") return "sales"
    if (demoRoleParam && (VIEWER_ROLES as readonly string[]).includes(demoRoleParam)) {
      return demoRoleParam as ViewerRole
    }
    return "procurement"
  })
  const query = usePurchaseOrderCenterQuery(purchaseOrderId, viewerRole)
  const acquireToken = useAcquireDraftTokenMutation()
  const saveMutation = useSavePurchaseOrderDraftMutation()
  const submitMutation = useSubmitPurchaseOrderMutation()
  const reviewMutation = useReviewPurchaseOrderMutation()
  const changeMutation = useStartPurchaseChangeMutation()

  const activeSection = resolveSection(section)
  const mode = resolveMode(modeParam)

  const [draftEditToken, setDraftEditToken] = React.useState<string | null>(
    null
  )
  const [lineEdits, setLineEdits] = React.useState<
    Record<
      string,
      { quantity?: string; unitCostGross?: string; inputTaxRate: string }
    >
  >({})
  const [leaveGuardOpen, setLeaveGuardOpen] = React.useState(false)
  const [pendingLeave, setPendingLeave] = React.useState<(() => void) | null>(
    null
  )
  const [result, setResult] = React.useState<{
    status: "succeeded" | "rejected" | "blocked" | "unknown"
    title: string
    description: string
    reference?: string
    facts?: { label: string; value: React.ReactNode }[]
  } | null>(null)
  const [submitConfirmOpen, setSubmitConfirmOpen] = React.useState(false)
  const [changeConfirmOpen, setChangeConfirmOpen] = React.useState(false)
  const [approveConfirmOpen, setApproveConfirmOpen] = React.useState(false)
  const titleRef = React.useRef<HTMLHeadingElement>(null)

  const order = query.data

  const draftForm = useAppForm({
    defaultValues: {
      paymentTermCode: order?.header.paymentTermCode ?? "POSTPAY_NET15",
      note: "",
    },
    validators: { onChange: draftSchema },
    onSubmit: async () => {
      await handleSave(false)
    },
  })

  const reviewForm = useAppForm({
    defaultValues: {
      reasonCode: "COST_TAX",
      comment: "",
    },
    validators: { onChange: reviewSchema },
    onSubmit: async ({ value }) => {
      await handleReject(value.reasonCode, value.comment)
    },
  })

  React.useEffect(() => {
    titleRef.current?.focus()
  }, [purchaseOrderId, mode])

  // 编辑态脏检测：行级数量/单价/税率或付款条件与当前内容不一致
  const editDirty = React.useMemo(() => {
    if (mode !== "edit" || !order) return false
    if (draftForm.state.values.paymentTermCode !== order.header.paymentTermCode)
      return true
    if (draftForm.state.values.note.trim()) return true
    return order.currentContent.lines.some((line) => {
      const edit = lineEdits[line.lineId]
      if (!edit) return false
      return (
        (edit.quantity ?? line.quantity) !== line.quantity ||
        (edit.unitCostGross ?? line.unitCostGross) !== line.unitCostGross ||
        edit.inputTaxRate !== line.inputTaxRate
      )
    })
  }, [
    draftForm.state.values.note,
    draftForm.state.values.paymentTermCode,
    lineEdits,
    mode,
    order,
  ])

  // 编辑态刷新/关页守卫
  React.useEffect(() => {
    if (mode !== "edit" || !editDirty) return
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault()
      event.returnValue = ""
    }
    window.addEventListener("beforeunload", onBeforeUnload)
    return () => window.removeEventListener("beforeunload", onBeforeUnload)
  }, [editDirty, mode])

  /** 编辑态离开前弹「保存并离开 / 放弃修改 / 继续编辑」确认 */
  const requestLeave = React.useCallback(
    (go: () => void) => {
      if (mode === "edit" && editDirty) {
        setPendingLeave(() => go)
        setLeaveGuardOpen(true)
        return
      }
      go()
    },
    [editDirty, mode]
  )

  React.useEffect(() => {
    if (!order || mode !== "edit") return
    if (draftEditToken) return
    if (!order.allowedActions.includes("EDIT")) return
    void acquireToken.mutateAsync(purchaseOrderId).then((res) => {
      setDraftEditToken(res.draftEditToken)
    }).catch((error: Error) => {
      setResult({
        status: "blocked",
        title: leaseText.cannotEdit,
        description: error.message,
      })
    })
    // init line edits
    const next: typeof lineEdits = {}
    for (const line of order.currentContent.lines) {
      next[line.lineId] = {
        quantity: line.quantity,
        unitCostGross: line.unitCostGross,
        inputTaxRate: line.inputTaxRate,
      }
    }
    setLineEdits(next)
    draftForm.setFieldValue("paymentTermCode", order.header.paymentTermCode)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only when entering edit
  }, [order?.identity.purchaseOrderId, mode])

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (mode !== "edit") return
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
        event.preventDefault()
        void handleSave(false)
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault()
        setSubmitConfirmOpen(true)
      }
    }
    window.addEventListener("keydown", onKey)
    return () => window.removeEventListener("keydown", onKey)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, draftEditToken, lineEdits, order])

  async function handleSave(simulateConflict: boolean): Promise<boolean> {
    if (!order || !draftEditToken) return false
    // 行内即时校验：数量/单价为正数，税率为 0-1 小数
    const invalidLine = order.currentContent.lines.find((line) => {
      const edit = lineEdits[line.lineId]
      if (!edit) return false
      return (
        !positiveDecimal(edit.quantity ?? line.quantity) ||
        !positiveDecimal(edit.unitCostGross ?? line.unitCostGross) ||
        !taxRateValid(edit.inputTaxRate)
      )
    })
    if (invalidLine) {
      setResult({
        status: "rejected",
        title: "保存失败",
        description: `「${invalidLine.itemName}」数量与含税单价须为正数，税率须为 0 到 1 的十进制数（如 0.13 表示 13%）。`,
      })
      return false
    }
    const paymentTermCode = draftForm.state.values.paymentTermCode
    const paymentTermLabel =
      PAYMENT_TERM_OPTIONS.find((option) => option.value === paymentTermCode)
        ?.label ?? order.header.paymentTermLabel

    const response = await saveMutation.mutateAsync({
      purchaseOrderId,
      expectedLockVersion: order.identity.lockVersion,
      draftEditToken,
      paymentTermCode,
      paymentTermLabel,
      lines: order.currentContent.lines.map((line) => ({
        lineId: line.lineId,
        lineType: line.lineType,
        quantity: lineEdits[line.lineId]?.quantity ?? line.quantity,
        unitCostGross:
          lineEdits[line.lineId]?.unitCostGross ?? line.unitCostGross,
        inputTaxRate:
          lineEdits[line.lineId]?.inputTaxRate ?? line.inputTaxRate,
        logisticsFeeReason: line.logisticsFeeReason,
      })),
      idempotencyKey: `save-${purchaseOrderId}-${Date.now()}`,
      simulateConflict,
    })

    if (response.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "草稿已保存",
        description: `金额已按系统规范计算：含税 ${response.data.totals.gross} / 不含税 ${response.data.totals.net} / 税额 ${response.data.totals.tax}`,
        reference: response.reference,
        facts: [
          { label: "数据版本", value: `v${response.data.lockVersion}` },
        ],
      })
      await query.refetch()
    } else if (response.status === "unknown") {
      setResult({
        status: "unknown",
        title: "保存结果未知",
        description: `${response.message} 输入已保留，未切换状态。`,
        reference: response.idempotencyKey,
      })
    } else {
      setResult({
        status: "rejected",
        title: "保存失败",
        description: `${response.message} 输入已保留。`,
      })
    }
    return response.status === "succeeded"
  }

  async function handleSubmit() {
    if (!order || !draftEditToken) return
    // ensure latest save hash
    const saveRes = await saveMutation.mutateAsync({
      purchaseOrderId,
      expectedLockVersion: order.identity.lockVersion,
      draftEditToken,
      paymentTermCode: draftForm.state.values.paymentTermCode,
      paymentTermLabel: order.header.paymentTermLabel,
      lines: order.currentContent.lines.map((line) => ({
        lineId: line.lineId,
        lineType: line.lineType,
        quantity: lineEdits[line.lineId]?.quantity ?? line.quantity,
        unitCostGross:
          lineEdits[line.lineId]?.unitCostGross ?? line.unitCostGross,
        inputTaxRate:
          lineEdits[line.lineId]?.inputTaxRate ?? line.inputTaxRate,
      })),
      idempotencyKey: `save-before-submit-${purchaseOrderId}-${Date.now()}`,
    })
    if (saveRes.status !== "succeeded") {
      setSubmitConfirmOpen(false)
      setResult({
        status: saveRes.status === "unknown" ? "unknown" : "rejected",
        title: "提交前保存未成功",
        description: saveRes.message,
      })
      return
    }

    const refreshed = await query.refetch()
    const lockVersion =
      refreshed.data?.identity.lockVersion ?? saveRes.data.lockVersion

    const response = await submitMutation.mutateAsync({
      purchaseOrderId,
      expectedLockVersion: lockVersion,
      expectedDraftContentHash: saveRes.data.draftContentHash,
      draftEditToken,
      idempotencyKey: `submit-${purchaseOrderId}-${Date.now()}`,
    })
    setSubmitConfirmOpen(false)
    if (response.status === "succeeded") {
      setDraftEditToken(null)
      setResult({
        status: "succeeded",
        title: "已提交财务审核",
        description:
          "已形成不可修改的采购提交与采购审核任务；编辑已结束。",
        reference: response.reference,
        facts: [
          { label: "单据编号", value: response.data.purchaseNo },
          { label: "提交记录", value: `第 ${response.data.submissionNo} 次提交` },
          { label: "数据版本", value: `v${response.data.lockVersion}` },
          { label: "审核任务", value: "已创建" },
        ],
      })
      router.replace(
        `/procurement/orders/${purchaseOrderId}?mode=review`
      )
    } else if (response.status === "unknown") {
      setResult({
        status: "unknown",
        title: "提交结果未知",
        description: response.message,
        reference: response.idempotencyKey,
      })
    } else {
      setResult({
        status: "rejected",
        title: "提交失败",
        description: response.message,
      })
    }
  }

  async function handleApprove() {
    if (!order?.reviewWorkItem || !order.identity.currentSubmissionId) return
    const response = await reviewMutation.mutateAsync({
      purchaseOrderId,
      submissionId: order.identity.currentSubmissionId,
      workItemId: order.reviewWorkItem.workItemId,
      expectedLockVersion: order.identity.lockVersion,
      reviewResult: "APPROVED",
      idempotencyKey: `approve-${purchaseOrderId}-${Date.now()}`,
    })
    setApproveConfirmOpen(false)
    if (response.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "财务审核已通过",
        description:
          "已形成采购生效版本与应付原始分录；审核任务完成。实际付款独立推进。",
        reference: response.reference,
        facts: [
          { label: "版本", value: `v${response.data.revisionNo ?? 1}` },
          {
            label: "应付未结",
            value: response.data.payableOpenAmount ?? "—",
          },
        ],
      })
      await query.refetch()
      router.replace(`/procurement/orders/${purchaseOrderId}`)
    } else if (response.status === "unknown") {
      setResult({
        status: "unknown",
        title: "审核结果未知",
        description: response.message,
        reference: response.idempotencyKey,
      })
    } else {
      setResult({
        status: "rejected",
        title: "通过失败",
        description: response.message,
      })
    }
  }

  async function handleReject(reasonCode: string, comment: string) {
    if (!order?.reviewWorkItem || !order.identity.currentSubmissionId) return
    const response = await reviewMutation.mutateAsync({
      purchaseOrderId,
      submissionId: order.identity.currentSubmissionId,
      workItemId: order.reviewWorkItem.workItemId,
      expectedLockVersion: order.identity.lockVersion,
      reviewResult: "REJECTED",
      reasonCode,
      comment,
      idempotencyKey: `reject-${purchaseOrderId}-${Date.now()}`,
    })
    if (response.status === "succeeded") {
      setResult({
        status: "rejected",
        title: "财务已驳回",
        description:
          "已记录驳回结论并完成当前审核任务；不创建替代任务。采购可改草稿后重新提交。",
        reference: response.reference,
        facts: [
          {
            label: "原因",
            value: REJECT_REASON_LABEL[reasonCode] ?? reasonCode,
          },
          { label: "说明", value: comment },
        ],
      })
      await query.refetch()
      router.replace(
        `/procurement/orders/${purchaseOrderId}?mode=edit`
      )
    } else if (response.status === "unknown") {
      setResult({
        status: "unknown",
        title: "驳回结果未知",
        description: response.message,
        reference: response.idempotencyKey,
      })
    } else {
      setResult({
        status: "rejected",
        title: "驳回失败",
        description: response.message,
      })
    }
  }

  async function handleStartChange() {
    if (!order) return
    const response = await changeMutation.mutateAsync({
      purchaseOrderId,
      expectedLockVersion: order.identity.lockVersion,
      idempotencyKey: `change-${purchaseOrderId}-${Date.now()}`,
    })
    setChangeConfirmOpen(false)
    if (response.status === "succeeded") {
      setResult({
        status: "succeeded",
        title: "已创建采购变更工作副本",
        description:
          "生效字段锁定；不覆盖已发生付款、发票或履约记录。变更以基准版本创建目标提交。",
        reference: response.reference,
        facts: [
          { label: "变更记录", value: "已创建" },
          {
            label: "基准版本",
            value: `v${response.data.baseRevisionNo}`,
          },
        ],
      })
      await query.refetch()
    } else {
      setResult({
        status: "blocked",
        title: "无法发起变更",
        description:
          response.status === "failed" ? response.message : "未知错误",
      })
    }
  }

  if (query.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="采购单" description="正在加载详情…" />
      </div>
    )
  }

  if (query.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="采购单" description="详情加载失败" />
        <BusinessFailureState
          kind="system"
          title="详情加载失败"
          description="未能加载采购单详情。请重试；若持续失败，可返回列表稍后再来。"
          onRetry={() => void query.refetch()}
          action={
            <Button
              variant="outline"
              size="sm"
              render={<Link href="/procurement/orders" />}
            >
              返回列表
            </Button>
          }
        />
      </div>
    )
  }

  if (!order) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="采购单不存在" description="单据可能已删除或编号有误" />
        <BusinessEmptyState
          kind="no-data"
          title="未找到该采购单"
          description="该采购单可能已删除或不在当前数据范围内。"
          action={
            <Button render={<Link href="/procurement/orders" />}>
              返回列表
            </Button>
          }
        />
      </div>
    )
  }

  const baseHref = `/procurement/orders/${order.identity.purchaseOrderId}`
  const w12PayHref = `/finance/supplier-accounts?view=payable&session=payment&from=W08&purchaseOrderId=${encodeURIComponent(order.identity.purchaseOrderId)}&supplierId=${encodeURIComponent(order.header.supplierId)}&returnTo=${encodeURIComponent(baseHref)}`
  const w27SettleHref = `/supplier-api/settlements?supplierId=${encodeURIComponent(order.header.supplierId)}&returnTo=${encodeURIComponent(baseHref)}`
  const displayNo =
    order.identity.purchaseNo ??
    order.identity.draftLabel ??
    "采购单（未编号）"
  const costMasked = order.currentContent.costMasked
  const gate = order.progress.prepaymentGate
  const canEdit = order.allowedActions.includes("EDIT")
  const canSubmit = order.allowedActions.includes("SUBMIT")
  const canReview = order.allowedActions.includes("REVIEW")
  const canChange = order.allowedActions.includes("START_CHANGE")
  const canFulfill = order.allowedActions.includes("FULFILL")
  const canPay = order.allowedActions.includes("PAY")
  const fulfillBlocker = order.actionBlockers.find(
    (b) => b.action === "FULFILL"
  )
  const changeBlocker = order.actionBlockers.find(
    (b) => b.action === "START_CHANGE"
  )

  const navItems: {
    id: SectionId
    label: string
    href: string
  }[] = [
    { id: "overview", label: "概览", href: baseHref },
    {
      id: "lines",
      label: "明细与分配",
      href: `${baseHref}?section=lines${mode !== "view" ? `&mode=${mode}` : ""}`,
    },
    {
      id: "fulfillment",
      label: "履约",
      href: `${baseHref}?section=fulfillment${mode !== "view" ? `&mode=${mode}` : ""}`,
    },
    {
      id: "payable",
      label: "应付与票款",
      href: `${baseHref}?section=payable${mode !== "view" ? `&mode=${mode}` : ""}`,
    },
    {
      id: "changes",
      label: "变更与异常",
      href: `${baseHref}?section=changes${mode !== "view" ? `&mode=${mode}` : ""}`,
    },
    {
      id: "audit",
      label: "审计",
      href: `${baseHref}?section=audit${mode !== "view" ? `&mode=${mode}` : ""}`,
    },
  ]

  const modeLabel =
    mode === "edit"
      ? order.identity.reviewStatus === "REJECTED"
        ? "被驳回待修改"
        : "采购草稿编辑"
      : mode === "review"
        ? "财务审核（只读）"
        : "详情"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          { id: "proc", label: "采购与履约", href: "/procurement/confirm" },
          { id: "orders", label: "采购单", href: "/procurement/orders" },
          { id: "current", label: displayNo, current: true },
        ]}
        metadata={
          <span className="inline-flex items-center gap-2">
            <span
              ref={titleRef}
              tabIndex={-1}
              className="outline-none font-medium text-foreground"
            >
              {modeLabel}
            </span>
          </span>
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "back",
                label: "返回列表",
                icon: ArrowLeftIcon,
                variant: "outline",
                onClick: () => requestLeave(() => router.push("/procurement/orders")),
              },
              ...(canPay
                ? [
                    {
                      actionKey: "pay",
                      label: "去供应商往来",
                      variant: "outline" as const,
                      onClick: () =>
                        router.push(w12PayHref),
                    },
                    {
                      actionKey: "settle",
                      label: "去对账结算",
                      variant: "outline" as const,
                      onClick: () =>
                        router.push(w27SettleHref),
                    },
                  ]
                : []),
              ...(canFulfill
                ? [
                    {
                      actionKey: "fulfill",
                      label: "去交付",
                      variant: "outline" as const,
                      onClick: () =>
                        router.push(
                          `/fulfillment?lane=procurement&scope=mine&purchaseOrderId=${encodeURIComponent(order.identity.purchaseOrderId)}&from=W08&returnTo=${encodeURIComponent(baseHref)}`
                        ),
                    },
                  ]
                : []),
              ...(canEdit && mode !== "edit"
                ? [
                    {
                      actionKey: "edit",
                      label: "编辑草稿",
                      icon: FilePenLineIcon,
                      onClick: () =>
                        router.push(`${baseHref}?mode=edit`),
                    },
                  ]
                : []),
              ...(canReview && mode !== "review"
                ? [
                    {
                      actionKey: "review",
                      label: "打开审核",
                      icon: ShieldCheckIcon,
                      onClick: () =>
                        router.push(`${baseHref}?mode=review`),
                    },
                  ]
                : []),
              ...(canChange
                ? [
                    {
                      actionKey: "change",
                      label: "发起采购变更",
                      variant: "outline" as const,
                      onClick: () => setChangeConfirmOpen(true),
                    },
                  ]
                : []),
            ]}
          />
        }
      />

      {result ? (
        <FormalActionResult
          status={result.status}
          title={result.title}
          description={result.description}
          reference={result.reference}
          facts={result.facts}
          actions={
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setResult(null)}
            >
              关闭
            </Button>
          }
        />
      ) : null}

      <DocumentHeader
        density="compact"
        title={order.header.supplierSnapshot || "采购单"}
        documentNumber={displayNo}
        primaryStatus={{
          label: order.identity.statusLabel,
          tone: order.identity.statusTone,
        }}
        version={
          order.identity.revisionNo
            ? order.identity.revisionNo
            : "草稿"
        }
        meta={
          <span className="inline-flex flex-wrap items-center gap-x-1.5">
            <span>来源 {order.header.salesOrderNo}</span>
          </span>
        }
        statuses={[
          {
            id: "type",
            label: "类型",
            status: {
              label: PURCHASE_TYPE_LABEL[order.header.purchaseType],
              tone: "neutral",
            },
          },
          {
            id: "resp",
            label: "履约",
            status: {
              label:
                FULFILLMENT_RESPONSIBILITY_LABEL[
                  order.header.fulfillmentResponsibility
                ],
              tone: "neutral",
            },
          },
        ]}
      />

      <StatusTrackSummary
        tracks={[
          {
            id: "review",
            label: "审核",
            status: {
              label: order.identity.reviewLabel,
              tone:
                order.identity.reviewStatus === "PENDING"
                  ? "warning"
                  : order.identity.reviewStatus === "APPROVED"
                    ? "success"
                    : order.identity.reviewStatus === "REJECTED"
                      ? "destructive"
                      : "neutral",
            },
          },
          {
            id: "payment",
            label: "付款",
            status: {
              label: order.progress.payment,
              tone:
                order.progress.payment === "已付"
                  ? "success"
                  : order.progress.payment === "部分"
                    ? "info"
                    : "neutral",
            },
          },
          {
            id: "invoice",
            label: "进项票",
            status: {
              label: order.progress.invoice,
              tone:
                order.progress.invoice === "完成"
                  ? "success"
                  : order.progress.invoice === "部分"
                    ? "info"
                    : "neutral",
            },
          },
          {
            id: "fulfillment",
            label: "履约",
            status: {
              label: order.progress.fulfillment,
              tone: order.fulfillmentSummary.progressTone,
            },
          },
        ]}
      />

      <nav
        className="flex flex-wrap gap-1 border-b border-border pb-2"
        aria-label="详情子区"
      >
        {navItems.map((item) => (
          <Button
            key={item.id}
            type="button"
            size="sm"
            variant={activeSection === item.id ? "secondary" : "ghost"}
            render={<Link href={item.href} />}
          >
            {item.label}
          </Button>
        ))}
      </nav>

      {mode === "review" && canReview ? (
        <ReviewSurface
          order={order}
          // @ts-expect-error useAppForm generic variance vs surface prop
          reviewForm={reviewForm}
          pending={reviewMutation.isPending}
          onApprove={() => setApproveConfirmOpen(true)}
          costMasked={costMasked}
        />
      ) : null}

      {mode === "edit" && canEdit ? (
        <EditSurface
          order={order}
          // @ts-expect-error useAppForm generic variance vs surface prop
          draftForm={draftForm}
          lineEdits={lineEdits}
          setLineEdits={setLineEdits}
          draftEditToken={draftEditToken}
          canSubmit={canSubmit}
          savePending={saveMutation.isPending}
          onSave={() => void handleSave(false)}
          onSubmitOpen={() => setSubmitConfirmOpen(true)}
        />
      ) : null}

      {(mode === "view" || activeSection !== "overview") ? (
        <div className="grid gap-4">
          {activeSection === "overview" && mode === "view" ? (
            <DocumentSection title="概览">
              <DescriptionList columns="three">
                <DescriptionItem>
                  <DescriptionTerm>供应商</DescriptionTerm>
                  <DescriptionDetails>
                    {order.header.supplierSnapshot}
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>来源销售单</DescriptionTerm>
                  <DescriptionDetails>
                    <Link
                      href={`/sales/orders/${order.header.salesOrderId}`}
                      className="num text-primary underline-offset-2 hover:underline"
                    >
                      {order.header.salesOrderNo}
                    </Link>
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>付款条件</DescriptionTerm>
                  <DescriptionDetails>
                    {order.header.paymentTermLabel}
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>内容来源</DescriptionTerm>
                  <DescriptionDetails>
                    {order.currentContent.source === "DRAFT"
                      ? "草稿"
                      : order.currentContent.source === "SUBMISSION"
                        ? "已提交内容"
                        : "生效版本"}
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>最近预计交期</DescriptionTerm>
                  <DescriptionDetails className="num">
                    {order.header.expectedDate ?? "—"}
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>负责人</DescriptionTerm>
                  <DescriptionDetails>
                    {order.header.ownerName}
                  </DescriptionDetails>
                </DescriptionItem>
              </DescriptionList>
              {gate.state !== "NOT_APPLICABLE" ? (
                <div className="mt-4">
                  <PrepaymentGate
                    condition={{
                      kind: "amount",
                      required: costMasked ? "•••" : gate.required,
                      description: gate.message,
                    }}
                    allocated={costMasked ? "•••" : gate.allocated}
                    gap={costMasked ? "•••" : gate.gap}
                    updatedAt={{ dateTime: gate.updatedAt, label: gate.updatedAt }}
                    allowed={gate.state === "SATISFIED"}
                    paymentAction={
                      canPay ? (
                        <Button
                          type="button"
                          size="sm"
                          render={
                            <Link href={w12PayHref} />
                          }
                        >
                          去供应商往来
                        </Button>
                      ) : undefined
                    }
                  />
                </div>
              ) : null}
              <DocumentTotals
                className="mt-4 max-w-md"
                title="系统合计"
                items={[
                  {
                    id: "g",
                    label: "含税",
                    value: costMasked ? (
                      "•••"
                    ) : (
                      <MoneyValue value={order.currentContent.totals.gross} />
                    ),
                    basis: "含税",
                  },
                  {
                    id: "n",
                    label: "不含税",
                    value: costMasked ? (
                      "•••"
                    ) : (
                      <MoneyValue value={order.currentContent.totals.net} />
                    ),
                    basis: "不含税",
                  },
                  {
                    id: "t",
                    label: "税额",
                    value: costMasked ? (
                      "•••"
                    ) : (
                      <MoneyValue value={order.currentContent.totals.tax} />
                    ),
                  },
                ]}
              />
            </DocumentSection>
          ) : null}

          {activeSection === "lines" ? (
            <DocumentSection title="明细与分配">
              <LinesTable order={order} costMasked={costMasked} />
              <DocumentTotals
                className="mt-4 max-w-md ml-auto"
                title="系统合计"
                items={[
                  {
                    id: "g",
                    label: "含税",
                    value: costMasked ? (
                      "•••"
                    ) : (
                      <MoneyValue value={order.currentContent.totals.gross} />
                    ),
                    basis: "含税",
                  },
                  {
                    id: "n",
                    label: "不含税",
                    value: costMasked ? (
                      "•••"
                    ) : (
                      <MoneyValue value={order.currentContent.totals.net} />
                    ),
                    basis: "不含税",
                  },
                  {
                    id: "t",
                    label: "税额",
                    value: costMasked ? (
                      "•••"
                    ) : (
                      <MoneyValue value={order.currentContent.totals.tax} />
                    ),
                  },
                ]}
              />
            </DocumentSection>
          ) : null}

          {activeSection === "fulfillment" ? (
            <DocumentSection title="履约">
              <DescriptionList columns="three">
                <DescriptionItem>
                  <DescriptionTerm>进度</DescriptionTerm>
                  <DescriptionDetails>
                    <BusinessStatusBadge
                      context="detail"
                      label={order.fulfillmentSummary.progressLabel}
                      tone={order.fulfillmentSummary.progressTone}
                    />
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>入库</DescriptionTerm>
                  <DescriptionDetails className="num">
                    {order.fulfillmentSummary.inboundQty}
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>发货</DescriptionTerm>
                  <DescriptionDetails className="num">
                    {order.fulfillmentSummary.shippedQty}
                  </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                  <DescriptionTerm>剩余</DescriptionTerm>
                  <DescriptionDetails className="num">
                    {order.fulfillmentSummary.remainingQty}
                  </DescriptionDetails>
                </DescriptionItem>
              </DescriptionList>
              {order.fulfillmentSummary.note ? (
                <p className="mt-2 text-sm text-muted-foreground">
                  {order.fulfillmentSummary.note}
                </p>
              ) : null}
              <div className="mt-4 flex flex-wrap gap-2">
                {canFulfill ? (
                  <Button
                    type="button"
                    render={
                      <Link
                        href={`/fulfillment?lane=procurement&scope=mine&purchaseOrderId=${encodeURIComponent(order.identity.purchaseOrderId)}&from=W08&returnTo=${encodeURIComponent(baseHref)}`}
                      />
                    }
                  >
                    去交付与代发
                  </Button>
                ) : (
                  <div className="space-y-1">
                    <Button type="button" disabled>
                      履约入口未开放
                    </Button>
                    <p className="text-xs text-muted-foreground">
                      {fulfillBlocker?.message ??
                        "当前状态下不能进入交付，可先完成前置条件。"}
                    </p>
                  </div>
                )}
                {fulfillBlocker?.code === "PREPAYMENT_GATE" ? (
                  <Button
                    type="button"
                    variant="outline"
                    render={<Link href={w12PayHref} />}
                  >
                    去供应商往来
                  </Button>
                ) : null}
              </div>
              {gate.state === "BLOCKED" ? (
                <div className="mt-4">
                  <PrepaymentGate
                    condition={{
                      kind: "amount",
                      required: costMasked ? "•••" : gate.required,
                      description: gate.message,
                    }}
                    allocated={costMasked ? "•••" : gate.allocated}
                    gap={costMasked ? "•••" : gate.gap}
                    updatedAt={{ dateTime: gate.updatedAt, label: gate.updatedAt }}
                    allowed={false}
                    paymentAction={
                      <Button
                        type="button"
                        size="sm"
                        render={<Link href={w12PayHref} />}
                      >
                        去供应商往来
                      </Button>
                    }
                  />
                </div>
              ) : null}
            </DocumentSection>
          ) : null}

          {activeSection === "payable" ? (
            <DocumentSection title="应付与票款">
              {order.payableSummary ? (
                <DescriptionList columns="three">
                  <DescriptionItem>
                    <DescriptionTerm>应付未结</DescriptionTerm>
                    <DescriptionDetails>
                      {costMasked ? (
                        "•••"
                      ) : (
                        <MoneyValue
                          value={order.payableSummary.payableOpenAmount}
                        />
                      )}
                    </DescriptionDetails>
                  </DescriptionItem>
                  <DescriptionItem>
                    <DescriptionTerm>已付并核销</DescriptionTerm>
                    <DescriptionDetails>
                      {costMasked ? (
                        "•••"
                      ) : (
                        <MoneyValue
                          value={order.payableSummary.paidAllocatedAmount}
                        />
                      )}
                    </DescriptionDetails>
                  </DescriptionItem>
                  <DescriptionItem>
                    <DescriptionTerm>已收票并核销</DescriptionTerm>
                    <DescriptionDetails>
                      {costMasked ? (
                        "•••"
                      ) : (
                        <MoneyValue
                          value={
                            order.payableSummary.purchaseInvoiceAllocatedAmount
                          }
                        />
                      )}
                    </DescriptionDetails>
                  </DescriptionItem>
                </DescriptionList>
              ) : (
                <p className="text-sm text-muted-foreground">
                  尚未形成应付（需财务审核通过）。
                </p>
              )}
              <div className="mt-4">
                <Button
                  type="button"
                  variant="outline"
                  disabled={!canPay}
                  render={<Link href={w12PayHref} />}
                >
                  去供应商往来
                </Button>
              </div>
            </DocumentSection>
          ) : null}

          {activeSection === "changes" ? (
            <DocumentSection title="变更与异常">
              {order.changes.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  暂无采购变更。生效后变化须走变更，不得在本版本直接覆写付款/发票/履约记录。
                </p>
              ) : (
                <ul className="space-y-2">
                  {order.changes.map((change) => (
                    <li
                      key={change.changeId}
                      className="flex items-center justify-between rounded-lg border border-border px-3 py-2 text-sm"
                    >
                      <span>
                        {change.label}
                        {change.baseRevisionNo != null
                          ? ` · 基准 v${change.baseRevisionNo}`
                          : ""}
                      </span>
                      <BusinessStatusBadge
                        context="list"
                        label={change.statusLabel}
                        tone={change.tone}
                      />
                    </li>
                  ))}
                </ul>
              )}
              <div className="mt-4 flex flex-wrap gap-2">
                {canChange ? (
                  <Button
                    type="button"
                    onClick={() => setChangeConfirmOpen(true)}
                  >
                    发起采购变更
                  </Button>
                ) : (
                  <div className="space-y-1">
                    <Button type="button" disabled>
                      发起采购变更
                    </Button>
                    <p className="text-xs text-muted-foreground">
                      {changeBlocker?.message ??
                        "当前状态下不能发起变更，可先完成前置条件。"}
                    </p>
                  </div>
                )}
              </div>
            </DocumentSection>
          ) : null}

          {activeSection === "audit" ? (
            <DocumentSection title="审计">
              <ul className="space-y-2">
                {order.workflow.map((item) => (
                  <li
                    key={item.id}
                    className="rounded-lg border border-border px-3 py-2 text-sm"
                  >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                      <span className="font-medium">{item.actionLabel}</span>
                      <span className="num text-xs text-muted-foreground">
                        {item.at}
                      </span>
                    </div>
                    <div className="mt-0.5 text-xs text-muted-foreground">
                      {item.actorLabel}
                      {item.comment ? ` · ${item.comment}` : ""}
                    </div>
                  </li>
                ))}
              </ul>
            </DocumentSection>
          ) : null}

          {activeSection === "overview" && mode === "view" ? (
            <DocumentSection title="明细摘要">
              <LinesTable order={order} costMasked={costMasked} />
            </DocumentSection>
          ) : null}
        </div>
      ) : null}

      <FormalActionConfirmDialog
        open={submitConfirmOpen}
        onOpenChange={setSubmitConfirmOpen}
        title="提交财务审核"
        actionLabel="提交"
        confirmLabel="确认提交"
        fromStatus={{ label: "草稿", tone: "neutral" }}
        toStatus={{ label: "待财务审核", tone: "warning" }}
        lockedFields={[
          "供应商 / 采购类型 / 履约责任 / 付款条件",
          "商品行（二次确认分行）与物流费用",
          "销售分配与系统金额",
        ]}
        effects={[
          "形成不可修改的采购提交与数据版本",
          "创建采购审核任务",
          "结束草稿编辑；中心转等待审核态",
        ]}
        nextDepartment="财务审核"
        pending={submitMutation.isPending || saveMutation.isPending}
        onConfirm={() => void handleSubmit()}
      />

      <FormalActionConfirmDialog
        open={approveConfirmOpen}
        onOpenChange={setApproveConfirmOpen}
        title="财务审核通过"
        actionLabel="通过"
        confirmLabel="确认通过"
        fromStatus={{ label: "待财务审核", tone: "warning" }}
        toStatus={{ label: "已生效", tone: "success" }}
        lockedFields={[
          `本次审核的提交内容（销售单 ${order.header.salesOrderNo}）`,
          "不可变提交头行与销售分配",
        ]}
        effects={[
          "形成采购版本与应付原始分录",
          "完成当前审核任务",
          "不登记实际付款；履约受先款门禁约束",
        ]}
        nextDepartment="履约 / 付款"
        pending={reviewMutation.isPending}
        onConfirm={() => void handleApprove()}
      />

      <FormalActionConfirmDialog
        open={changeConfirmOpen}
        onOpenChange={setChangeConfirmOpen}
        title="发起采购变更"
        actionLabel="创建变更"
        confirmLabel="创建工作副本"
        fromStatus={{
          label: order.identity.statusLabel,
          tone: order.identity.statusTone,
        }}
        toStatus={{ label: "变更工作副本", tone: "warning" }}
        lockedFields={[
          `基准版本 v${order.identity.revisionNo ?? 1}`,
          "已发生入库/发货/付款/发票记录不回退",
        ]}
        effects={[
          "创建采购变更工作副本（同对象页签）",
          "不得在原版本表单直接覆写",
        ]}
        pending={changeMutation.isPending}
        onConfirm={() => void handleStartChange()}
      />

      <Dialog open={leaveGuardOpen} onOpenChange={setLeaveGuardOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>有未保存的修改</DialogTitle>
            <DialogDescription>
              当前编辑内容尚未保存，离开后修改将丢失。建议先保存草稿。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              继续编辑
            </DialogClose>
            <Button
              type="button"
              variant="outline"
              disabled={saveMutation.isPending}
              onClick={async () => {
                const ok = await handleSave(false)
                if (!ok) return
                setLeaveGuardOpen(false)
                pendingLeave?.()
              }}
            >
              保存并离开
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => {
                setLeaveGuardOpen(false)
                pendingLeave?.()
              }}
            >
              放弃修改并离开
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function LinesTable({
  order,
  costMasked,
}: {
  order: NonNullable<
    ReturnType<typeof usePurchaseOrderCenterQuery>["data"]
  >
  costMasked: boolean
}) {
  return (
    <div className="overflow-hidden rounded-lg border border-border">
      <Table data-density="compact">
        <TableHeader>
          <TableRow>
            <TableHead>项目</TableHead>
            <TableHead>类型</TableHead>
            <TableHead data-align="end">数量</TableHead>
            <TableHead data-align="end">含税单价</TableHead>
            <TableHead data-align="end">税率</TableHead>
            <TableHead data-align="end">交期</TableHead>
            <TableHead data-align="end">行含税</TableHead>
            <TableHead data-align="end">税额</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {order.currentContent.lines.map((line) => (
            <TableRow key={line.lineId}>
              <TableCell className="max-w-[16rem] whitespace-normal">
                <div className="font-medium">{line.itemName}</div>
                {line.procurementConfirmationLineId ? (
                  <div className="text-[11px] text-muted-foreground">
                    {line.salesAllocationLabel ??
                      `确认分行 · ${line.itemName}`}
                  </div>
                ) : null}
              </TableCell>
              <TableCell className="text-xs text-muted-foreground">
                {line.lineType === "LOGISTICS_FEE" ? "物流费用" : "商品/服务"}
              </TableCell>
              <TableCell data-align="end">
                {line.lineType === "LOGISTICS_FEE" ? (
                  "—"
                ) : (
                  <QuantityValue
                    value={line.quantity ?? "0"}
                    unit={line.unit}
                  />
                )}
              </TableCell>
              <TableCell data-align="end">
                {costMasked ? (
                  "•••"
                ) : (
                  <MoneyValue value={line.unitCostGross} />
                )}
              </TableCell>
              <TableCell data-align="end" className="num text-xs">
                {(Number(line.inputTaxRate) * 100).toFixed(0)}%
              </TableCell>
              <TableCell data-align="end" className="num text-xs">
                {line.expectedDeliveryDate ?? "—"}
              </TableCell>
              <TableCell data-align="end">
                {costMasked ? (
                  "•••"
                ) : (
                  <MoneyValue value={line.grossAmount} taxBasis="gross" />
                )}
              </TableCell>
              <TableCell data-align="end">
                {costMasked ? "•••" : <MoneyValue value={line.taxAmount} />}
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  )
}

function EditSurface({
  order,
  draftForm,
  lineEdits,
  setLineEdits,
  draftEditToken,
  canSubmit,
  savePending,
  onSave,
  onSubmitOpen,
}: {
  order: NonNullable<
    ReturnType<typeof usePurchaseOrderCenterQuery>["data"]
  >
  draftForm: ReturnType<typeof useAppForm>
  lineEdits: Record<
    string,
    { quantity?: string; unitCostGross?: string; inputTaxRate: string }
  >
  setLineEdits: React.Dispatch<
    React.SetStateAction<
      Record<
        string,
        { quantity?: string; unitCostGross?: string; inputTaxRate: string }
      >
    >
  >
  draftEditToken: string | null
  canSubmit: boolean
  savePending: boolean
  onSave: () => void
  onSubmitOpen: () => void
}) {
  return (
    <Card>
      <CardHeader className="border-b border-border">
        <CardTitle>
          {order.identity.reviewStatus === "REJECTED"
            ? "被驳回待修改"
            : "采购草稿"}
        </CardTitle>
        <CardDescription>
          来源销售 {order.header.salesOrderNo}
          {order.header.creationBasisId
            ? ` · 来自采购二次确认（销售单 ${order.header.salesOrderNo}）`
            : ""}
          。⌘S 保存 · ⌘↵ 打开提交确认。拆单维度（供应商、类型、付款条件、履约责任）已固定，不能修改。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 pt-4">
        {!draftEditToken ? (
          <Alert variant="warning">
            <AlertTitle>正在进入编辑…</AlertTitle>
            <AlertDescription>
              编辑内容仅保存在当前页面；刷新或关闭将丢失，请及时保存。
            </AlertDescription>
          </Alert>
        ) : (
          <p className="text-xs text-muted-foreground">正在编辑中</p>
        )}

        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label>供应商（只读）</Label>
            <div className="rounded-md border border-border bg-muted/30 px-3 py-2 text-sm">
              {order.header.supplierSnapshot}
            </div>
          </div>
          <div className="space-y-1.5">
            <Label>采购类型 / 履约责任（只读）</Label>
            <div className="rounded-md border border-border bg-muted/30 px-3 py-2 text-sm">
              {PURCHASE_TYPE_LABEL[order.header.purchaseType]} ·{" "}
              {
                FULFILLMENT_RESPONSIBILITY_LABEL[
                  order.header.fulfillmentResponsibility
                ]
              }
            </div>
          </div>
          <div className="space-y-1.5 sm:col-span-2">
            <Label htmlFor="payment-term">付款条件</Label>
            <draftForm.AppField name="paymentTermCode">
              {(field) => (
                <OptionCombobox
                  id="payment-term"
                  className="w-full"
                  value={String(field.state.value ?? "")}
                  onValueChange={(v) =>
                    field.handleChange(v ?? String(field.state.value ?? ""))
                  }
                  options={
                    PAYMENT_TERM_OPTIONS.some(
                      (option) => option.label === order.header.paymentTermLabel
                    )
                      ? [...PAYMENT_TERM_OPTIONS]
                      : [
                          {
                            value: order.header.paymentTermCode,
                            label: order.header.paymentTermLabel,
                          },
                          ...PAYMENT_TERM_OPTIONS,
                        ]
                  }
                  allowClear={false}
                  aria-label="付款条件"
                  placeholder="付款条件"
                />
              )}
            </draftForm.AppField>
          </div>
        </div>

        <Separator />

        <div className="space-y-2">
          <h3 className="text-sm font-semibold">明细（系统计算）</h3>
          <div className="overflow-hidden rounded-lg border border-border">
            <Table data-density="compact">
              <TableHeader>
                <TableRow>
                  <TableHead>项目</TableHead>
                  <TableHead data-align="end">数量</TableHead>
                  <TableHead data-align="end">含税单价</TableHead>
                  <TableHead data-align="end">税率</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {order.currentContent.lines.map((line) => (
                  <TableRow key={line.lineId}>
                    <TableCell className="whitespace-normal">
                      <div className="font-medium">{line.itemName}</div>
                      <div className="text-[11px] text-muted-foreground">
                        {line.lineType === "LOGISTICS_FEE"
                          ? "物流费用"
                          : line.procurementConfirmationLineId
                            ? (line.salesAllocationLabel ??
                              `确认分行 · ${line.itemName}`)
                            : "商品/服务"}
                      </div>
                    </TableCell>
                    <TableCell data-align="end">
                      {line.lineType === "LOGISTICS_FEE" ? (
                        "—"
                      ) : (
                        <>
                          <input
                            className="num w-20 rounded border border-border bg-background px-2 py-1 text-right text-sm"
                            value={lineEdits[line.lineId]?.quantity ?? ""}
                            onChange={(event) =>
                              setLineEdits((prev) => ({
                                ...prev,
                                [line.lineId]: {
                                  ...prev[line.lineId],
                                  inputTaxRate:
                                    prev[line.lineId]?.inputTaxRate ??
                                    line.inputTaxRate,
                                  quantity: event.target.value,
                                },
                              }))
                            }
                            aria-label={`${line.itemName} 数量`}
                          />
                          {!positiveDecimal(
                            lineEdits[line.lineId]?.quantity ?? line.quantity
                          ) ? (
                            <span className="block text-[11px] text-destructive">
                              须为正数
                            </span>
                          ) : null}
                        </>
                      )}
                    </TableCell>
                    <TableCell data-align="end">
                      <>
                        <input
                          className="num w-28 rounded border border-border bg-background px-2 py-1 text-right text-sm"
                          value={lineEdits[line.lineId]?.unitCostGross ?? ""}
                          onChange={(event) =>
                            setLineEdits((prev) => ({
                              ...prev,
                              [line.lineId]: {
                                ...prev[line.lineId],
                                inputTaxRate:
                                  prev[line.lineId]?.inputTaxRate ??
                                  line.inputTaxRate,
                                unitCostGross: event.target.value,
                              },
                            }))
                          }
                          aria-label={`${line.itemName} 含税单价`}
                        />
                        {!positiveDecimal(
                          lineEdits[line.lineId]?.unitCostGross ??
                            line.unitCostGross
                        ) ? (
                          <span className="block text-[11px] text-destructive">
                            须为正数
                          </span>
                        ) : null}
                      </>
                    </TableCell>
                    <TableCell data-align="end">
                      <>
                        <input
                          className="num w-20 rounded border border-border bg-background px-2 py-1 text-right text-sm"
                          value={
                            (() => {
                              const raw =
                                lineEdits[line.lineId]?.inputTaxRate ??
                                line.inputTaxRate
                              if (raw === "") return ""
                              const value = Number(raw)
                              return Number.isFinite(value)
                                ? String(value * 100)
                                : raw
                            })()
                          }
                          onChange={(event) => {
                            const raw = event.target.value
                            const parsed = Number(raw)
                            setLineEdits((prev) => ({
                              ...prev,
                              [line.lineId]: {
                                ...prev[line.lineId],
                                inputTaxRate:
                                  raw === "" || !Number.isFinite(parsed)
                                    ? raw
                                    : String(parsed / 100),
                              },
                            }))
                          }}
                          aria-label={`${line.itemName} 税率（%）`}
                        />
                        <span className="ml-1 text-xs text-muted-foreground">
                          %
                        </span>
                        {!taxRateValid(
                          lineEdits[line.lineId]?.inputTaxRate ??
                            line.inputTaxRate
                        ) ? (
                          <span className="block text-[11px] text-destructive">
                            税率须为 0-1 的小数（如 0.13）
                          </span>
                        ) : null}
                      </>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={!draftEditToken || savePending}
            onClick={onSave}
          >
            {savePending ? "保存中…" : "保存草稿"}
          </Button>
          <Button
            type="button"
            disabled={!draftEditToken || !canSubmit || savePending}
            onClick={onSubmitOpen}
          >
            提交财务审核
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}

function ReviewSurface({
  order,
  reviewForm,
  pending,
  onApprove,
  costMasked,
}: {
  order: NonNullable<
    ReturnType<typeof usePurchaseOrderCenterQuery>["data"]
  >
  reviewForm: ReturnType<typeof useAppForm>
  pending: boolean
  onApprove: () => void
  costMasked: boolean
}) {
  const samePerson =
    order.header.submittedBy === W08_REVIEWER_NAME ||
    order.reviewWorkItem?.submittedBy === W08_REVIEWER_NAME

  return (
    <Card>
      <CardHeader className="border-b border-border">
        <CardTitle>财务审核视图</CardTitle>
        <CardDescription>
          以下为采购提交的只读回显，不可修改
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 pt-4">
        <Alert>
          <AlertTitle>本次提交内容</AlertTitle>
          <AlertDescription>
            经办 {order.header.submittedBy ?? "—"} · 提交于{" "}
            {order.header.submittedAt ?? "—"}
          </AlertDescription>
        </Alert>
        {samePerson ? (
          <Alert variant="destructive">
            <AlertTitle>岗位分离限制</AlertTitle>
            <AlertDescription>
              审核人不得为提交经办人，当前不能通过或驳回本次提交。
            </AlertDescription>
          </Alert>
        ) : null}

        <DescriptionList columns="three">
          <DescriptionItem>
            <DescriptionTerm>供应商</DescriptionTerm>
            <DescriptionDetails>
              {order.header.supplierSnapshot}
            </DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>含税 / 不含税 / 税</DescriptionTerm>
            <DescriptionDetails className="num">
              {costMasked
                ? "•••"
                : `${order.currentContent.totals.gross} / ${order.currentContent.totals.net} / ${order.currentContent.totals.tax}`}
            </DescriptionDetails>
          </DescriptionItem>
          <DescriptionItem>
            <DescriptionTerm>付款条件</DescriptionTerm>
            <DescriptionDetails>
              {order.header.paymentTermLabel}
            </DescriptionDetails>
          </DescriptionItem>
        </DescriptionList>

        <LinesTable order={order} costMasked={costMasked} />

        <Separator />

        <div className="flex flex-wrap items-end gap-3">
          <Button
            type="button"
            disabled={pending || samePerson || !order.reviewWorkItem}
            onClick={onApprove}
          >
            通过
          </Button>
          {samePerson ? (
            <span className="text-xs text-destructive">
              岗位分离：审核人不得为提交经办人
            </span>
          ) : null}
        </div>

        <form
          className="space-y-3 rounded-lg border border-border p-3"
          onSubmit={(event) => {
            event.preventDefault()
            void reviewForm.handleSubmit()
          }}
        >
          <p className="text-sm font-medium">驳回</p>
          <div className="space-y-1.5">
            <Label htmlFor="reject-reason">原因</Label>
            <reviewForm.AppField name="reasonCode">
              {(field) => (
                <OptionCombobox
                  id="reject-reason"
                  className="w-full"
                  value={String(field.state.value ?? "")}
                  onValueChange={(v) =>
                    field.handleChange(v ?? String(field.state.value ?? ""))
                  }
                  options={Object.entries(REJECT_REASON_LABEL).map(
                    ([code, label]) => ({
                      value: code,
                      label,
                    })
                  )}
                  allowClear={false}
                  aria-label="原因"
                  placeholder="选择原因"
                />
              )}
            </reviewForm.AppField>
          </div>
          <reviewForm.AppField name="comment">
            {(field) => (
              <field.TextareaField
                label="说明"
                placeholder="结构化原因说明"
                rows={3}
              />
            )}
          </reviewForm.AppField>
          <reviewForm.AppForm>
            <reviewForm.SubmitButton
              label={pending ? "提交中…" : "确认驳回"}
              disabled={samePerson || !order.reviewWorkItem}
            />
          </reviewForm.AppForm>
        </form>
      </CardContent>
    </Card>
  )
}
