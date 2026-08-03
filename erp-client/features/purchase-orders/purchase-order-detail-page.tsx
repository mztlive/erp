"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { z } from "zod"
import {
  ArrowLeftIcon,
  FilePenLineIcon,
  LockIcon,
  ShieldCheckIcon,
} from "lucide-react"

import {
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
  PURCHASE_TYPE_LABEL,
  REJECT_REASON_LABEL,
} from "@/features/purchase-orders/types"
import { leaseText, versionText, workspaceLabel } from "@/lib/ui-text"

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

export function PurchaseOrderDetailPage({
  purchaseOrderId,
  section,
  mode: modeParam,
}: {
  purchaseOrderId: string
  section?: string
  mode?: string
}) {
  const router = useRouter()
  const [viewerRole] = React.useState<ViewerRole>("procurement")
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
  const [draftContentHash, setDraftContentHash] = React.useState<string>("")
  const [lineEdits, setLineEdits] = React.useState<
    Record<
      string,
      { quantity?: string; unitCostGross?: string; inputTaxRate: string }
    >
  >({})
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

  React.useEffect(() => {
    if (!order || mode !== "edit") return
    if (draftEditToken) return
    if (!order.allowedActions.includes("EDIT")) return
    void acquireToken.mutateAsync(purchaseOrderId).then((res) => {
      setDraftEditToken(res.draftEditToken)
      setDraftContentHash(`dch_${purchaseOrderId}_v${res.lockVersion}`)
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

  async function handleSave(simulateConflict: boolean) {
    if (!order || !draftEditToken) return
    const paymentTermCode = draftForm.state.values.paymentTermCode
    const paymentTermLabel =
      paymentTermCode === "PREPAY_100"
        ? "先款 100%"
        : paymentTermCode === "PREPAY_50"
          ? "先款 50%"
          : paymentTermCode === "PREPAY_30"
            ? "先款 30%"
            : paymentTermCode === "POSTPAY_NET30"
              ? "货到 30 天"
              : "货到 15 天"

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
      setDraftContentHash(response.data.draftContentHash)
      setResult({
        status: "succeeded",
        title: "草稿已保存",
        description: `金额已按系统规范计算：含税 ${response.data.totals.gross} / 不含税 ${response.data.totals.net} / 税额 ${response.data.totals.tax}`,
        reference: response.reference,
        facts: [
          { label: "lockVersion", value: String(response.data.lockVersion) },
          { label: versionText.dataVersion, value: response.data.draftContentHash },
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
        description:
          saveRes.status === "failed"
            ? saveRes.message
            : saveRes.message,
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
          { label: "submissionId", value: response.data.submissionId },
          { label: "subjectHash", value: response.data.subjectHash },
          { label: "审核任务", value: response.data.workItemId },
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
        status: "blocked",
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
          { label: "变更单", value: response.data.changeId },
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

  if (!order) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="采购单不存在"
          description={`未找到 ${purchaseOrderId}`}
          actions={
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
  const displayNo =
    order.identity.purchaseNo ??
    order.identity.draftLabel ??
    order.identity.purchaseOrderId
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
    { id: "lines", label: "明细与分配", href: `${baseHref}?section=lines` },
    {
      id: "fulfillment",
      label: "履约",
      href: `${baseHref}?section=fulfillment`,
    },
    {
      id: "payable",
      label: "应付与票款",
      href: `${baseHref}?section=payable`,
    },
    {
      id: "changes",
      label: "变更与异常",
      href: `${baseHref}?section=changes`,
    },
    { id: "audit", label: "审计", href: `${baseHref}?section=audit` },
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
                onClick: () => router.push("/procurement/orders"),
              },
              ...(canPay
                ? [
                    {
                      actionKey: "pay",
                      label: "去付款/发票",
                      variant: "outline" as const,
                      onClick: () =>
                        router.push(w12PayHref),
                    },
                  ]
                : []),
              ...(canFulfill
                ? [
                    {
                      actionKey: "fulfill",
                      label: "去履约",
                      variant: "outline" as const,
                      onClick: () => router.push("/fulfillment"),
                    },
                  ]
                : []),
              ...(canEdit && mode !== "edit"
                ? [
                    {
                      actionKey: "edit",
                      label: "编辑草稿",
                      icon: FilePenLineIcon,
                      mobileVisibility: "hide" as const,
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
                      mobileVisibility: "hide" as const,
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
                      mobileVisibility: "hide" as const,
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
              tone: "neutral",
            },
          },
          {
            id: "invoice",
            label: "进项票",
            status: {
              label: order.progress.invoice,
              tone: "neutral",
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
        {mode === "edit" ? (
          <Badge variant="info" className="ml-auto">
            mode=edit
          </Badge>
        ) : null}
        {mode === "review" ? (
          <Badge variant="warning" className="ml-auto">
            mode=review
          </Badge>
        ) : null}
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
          onSaveConflictDemo={() => void handleSave(true)}
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
                    {order.currentContent.source}
                    {order.identity.subjectHash
                      ? ` · ${order.identity.subjectHash}`
                      : ""}
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
                          去供应商往来登记付款
                        </Button>
                      ) : undefined
                    }
                  />
                </div>
              ) : null}
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
                  <Button type="button" render={<Link href="/fulfillment" />}>
                    去履约作业
                  </Button>
                ) : (
                  <Button type="button" disabled title={fulfillBlocker?.message}>
                    <LockIcon data-icon="inline-start" aria-hidden="true" />
                    履约入口已阻断
                  </Button>
                )}
                {fulfillBlocker?.code === "PREPAYMENT_GATE" ? (
                  <Button
                    type="button"
                    variant="outline"
                    render={<Link href={w12PayHref} />}
                  >
                    先完成供应商往来付款
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
                  去{workspaceLabel("W12")}
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
                  <Button
                    type="button"
                    disabled
                    title={changeBlocker?.message}
                  >
                    发起变更不可用
                  </Button>
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
          `submissionId ${order.identity.currentSubmissionId ?? "—"}`,
          `subjectHash ${order.identity.subjectHash ?? "—"}`,
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
                  <div className="num text-[11px] text-muted-foreground">
                    {line.procurementConfirmationLineId}
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
  onSaveConflictDemo,
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
  onSaveConflictDemo: () => void
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
            ? ` · 创建依据 ${order.header.creationBasisId}`
            : ""}
          。⌘S 保存 · ⌘↵ 打开提交确认。拆单维度变更需提示影响。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4 pt-4">
        {!draftEditToken ? (
          <Alert variant="warning">
            <AlertTitle>正在进入编辑…</AlertTitle>
            <AlertDescription>
              编辑信息仅保存在当前页面，不进入 URL。
            </AlertDescription>
          </Alert>
        ) : (
          <p className="text-xs text-muted-foreground">
            正在编辑中 · 版本 {order.identity.lockVersion}
          </p>
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
                  options={[
                    { value: "PREPAY_100", label: "先款 100%" },
                    { value: "PREPAY_50", label: "先款 50%" },
                    { value: "PREPAY_30", label: "先款 30%" },
                    { value: "POSTPAY_NET15", label: "货到 15 天" },
                    { value: "POSTPAY_NET30", label: "货到 30 天" },
                  ]}
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
                          : `确认 ${line.procurementConfirmationLineId ?? "—"}`}
                      </div>
                    </TableCell>
                    <TableCell data-align="end">
                      {line.lineType === "LOGISTICS_FEE" ? (
                        "—"
                      ) : (
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
                      )}
                    </TableCell>
                    <TableCell data-align="end">
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
                    </TableCell>
                    <TableCell data-align="end">
                      <input
                        className="num w-16 rounded border border-border bg-background px-2 py-1 text-right text-sm"
                        value={lineEdits[line.lineId]?.inputTaxRate ?? ""}
                        onChange={(event) =>
                          setLineEdits((prev) => ({
                            ...prev,
                            [line.lineId]: {
                              ...prev[line.lineId],
                              inputTaxRate: event.target.value,
                            },
                          }))
                        }
                        aria-label={`${line.itemName} 税率`}
                      />
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
            variant="ghost"
            disabled={!draftEditToken || savePending}
            onClick={onSaveConflictDemo}
          >
            模拟版本冲突
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
    order.header.submittedBy === "财务 · 周敏" ||
    order.reviewWorkItem?.submittedBy === "财务 · 周敏"

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
          <AlertTitle>不可变提交</AlertTitle>
          <AlertDescription>
            submissionId {order.identity.currentSubmissionId ?? "—"} · 经办{" "}
            {order.header.submittedBy ?? "—"} · 提交于{" "}
            {order.header.submittedAt ?? "—"}
          </AlertDescription>
        </Alert>

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
            />
          </reviewForm.AppForm>
        </form>
      </CardContent>
    </Card>
  )
}
