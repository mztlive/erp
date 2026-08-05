"use client"

import * as React from "react"
import { z } from "zod"
import {
  BanIcon,
  CircleDollarSignIcon,
  RefreshCwIcon,
  ShieldAlertIcon,
} from "lucide-react"

import {
  DraftSaveIndicator,
  FormalActionConfirmDialog,
  FormalActionResult,
  MoneyValue,
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
import { Separator } from "@/components/ui/separator"
import {
  useAdjustProcurementRejectionDraftMutation,
  useDecideLowMarginManagerMutation,
  useResolveProcurementRejectionMutation,
} from "@/features/sales-orders/queries"
import type {
  ProcurementRejectionResolution,
  SalesOrderListItem,
} from "@/features/sales-orders/types"
import { resultText } from "@/lib/ui-text"

const REJECT_REASON_LABEL: Record<string, string> = {
  MARGIN_TOO_LOW: "预计毛利过低",
  COST_TOO_HIGH: "采购成本过高",
  ITEM_UNAVAILABLE: "商品/服务无法采购",
  TERMS_UNCLEAR: "商业条件不清晰",
}

const WORK_ITEM_STATUS_LABEL: Record<string, string> = {
  UNCLAIMED: "待领取",
  CLAIMED: "处理中",
  COMPLETED: "已完成",
}

const priceSchema = z.object({
  unitPriceGross: z
    .string()
    .trim()
    .regex(/^\d+(\.\d{1,2})?$/, "请输入有效含税单价"),
  note: z.string().trim().min(2, "请填写调整说明"),
})

const lowMarginSchema = z.object({
  reason: z.string().trim().min(8, "请填写至少 8 字的理由"),
})

const voidSchema = z.object({
  reason: z.string().trim().min(4, "请填写作废原因"),
})

type FormalResult = {
  status: "succeeded" | "rejected" | "blocked"
  title: string
  description: string
  reference: string
}

type ProcurementRejectionCardProps = {
  order: SalesOrderListItem
  rejection: ProcurementRejectionResolution
}

/**
 * 采购驳回后三条处理方式：改完再报、低价请领导批、不做了作废。
 */
export function ProcurementRejectionCard({
  order,
  rejection,
}: ProcurementRejectionCardProps) {
  const resolveMutation = useResolveProcurementRejectionMutation()
  const adjustMutation = useAdjustProcurementRejectionDraftMutation()
  const lowMarginDecision = useDecideLowMarginManagerMutation()

  const [result, setResult] = React.useState<FormalResult | null>(null)
  const [priceSavedAt, setPriceSavedAt] = React.useState<Date | null>(null)
  const [confirm, setConfirm] = React.useState<
    | null
    | {
        action:
          | "RESUBMIT_CHANGED_TERMS"
          | "REQUEST_LOW_MARGIN_ACCEPTANCE"
          | "VOID_AFTER_REJECTION"
        title: string
        effects: string[]
      }
  >(null)
  const [pendingPayload, setPendingPayload] = React.useState<{
    lowMarginReason?: string
    voidReason?: string
  }>({})
  const [idempotencyKey] = React.useState(
    () => `pr-${order.id}-${rejection.rejectedSubmissionId}`
  )

  const priceForm = useAppForm({
    defaultValues: {
      unitPriceGross: order.lineItems[0]?.unitPriceGross ?? "",
      note: "",
    },
    validators: { onChange: priceSchema },
    onSubmit: async ({ value }) => {
      await adjustMutation.mutateAsync({
        salesOrderId: order.id,
        unitPriceGross: value.unitPriceGross.trim(),
        note: value.note.trim(),
      })
      setPriceSavedAt(new Date())
      setResult({
        status: "succeeded",
        title: "改价已保存",
        description: "可以点「改完再报给采购」继续。",
        reference: order.documentNumber,
      })
    },
  })

  const lowMarginForm = useAppForm({
    defaultValues: { reason: "" },
    validators: { onChange: lowMarginSchema },
    onSubmit: async ({ value }) => {
      setPendingPayload({ lowMarginReason: value.reason.trim() })
      setConfirm({
        action: "REQUEST_LOW_MARGIN_ACCEPTANCE",
        title: "确认：按原条件请领导批低毛利",
        effects: [
          "按被驳回时的商品与价格申请",
          "交给销售上级确认是否接受低毛利",
          "领导通过前不会再推给采购，本单也不会生效",
        ],
      })
    },
  })

  const voidForm = useAppForm({
    defaultValues: { reason: "" },
    validators: { onChange: voidSchema },
    onSubmit: async ({ value }) => {
      setPendingPayload({ voidReason: value.reason.trim() })
      setConfirm({
        action: "VOID_AFTER_REJECTION",
        title: "确认：不做了，作废本单",
        effects: [
          "本单作废，不能再继续履约",
          "采购驳回与历史记录会保留备查",
          "不会再生成后续采购确认",
        ],
      })
    },
  })

  const canResubmit = rejection.allowedActions.includes("RESUBMIT_CHANGED_TERMS")
  const canLowMargin = rejection.allowedActions.includes(
    "REQUEST_LOW_MARGIN_ACCEPTANCE"
  )
  const canVoid = rejection.allowedActions.includes("VOID_AFTER_REJECTION")
  const resubmitBlocker = rejection.actionBlockers.find(
    (b) => b.action === "RESUBMIT_CHANGED_TERMS"
  )

  const resolved =
    rejection.reviewStatus === "RESOLVED" ||
    rejection.reviewStatus === "VOIDED" ||
    Boolean(rejection.resolutionOutcome)

  const reasonLabel =
    REJECT_REASON_LABEL[rejection.rejectReasonCode] ??
    rejection.rejectReasonCode

  const resubmitBlockerText = resubmitBlocker
    ? friendlyResubmitBlocker(resubmitBlocker.reason)
    : null

  return (
    <Card size="sm" className="border-warning/40">
      <CardHeader className="border-b">
        <div className="flex flex-wrap items-center gap-2">
          <ShieldAlertIcon
            className="size-4 text-warning"
            aria-hidden="true"
          />
          <CardTitle>采购未通过，请销售处理</CardTitle>
          <Badge variant="warning">
            {rejection.reviewStatus === "PENDING_LOW_MARGIN_MANAGER"
              ? "等领导批低毛利"
              : rejection.reviewStatus === "VOIDED"
                ? "已作废"
                : rejection.reviewStatus === "RESOLVED"
                  ? "已处理"
                  : "待你处理"}
          </Badge>
        </div>
        <CardDescription>
          任选一种：改商品/改价后再报采购、按原条件请领导批低毛利，或作废本单。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {result ? (
          <FormalActionResult
            status={result.status}
            title={result.title}
            description={result.description}
            reference={result.reference}
            facts={[
              { label: "销售单", value: order.documentNumber },
              {
                label: "被驳回的那一版",
                value: `第 ${rejection.rejectedSubmissionNo} 次报给采购`,
              },
            ]}
          />
        ) : null}

        {rejection.resolutionOutcome ? (
          <Alert
            variant={
              rejection.resolutionOutcome.outcome.includes("VOID")
                ? "destructive"
                : "success"
            }
          >
            <AlertTitle>处理结果</AlertTitle>
            <AlertDescription>
              {rejection.resolutionOutcome.detail}
              {rejection.resolutionOutcome.newWorkItemId
                ? " · 已生成后续待办，请相关同事继续处理。"
                : null}
            </AlertDescription>
          </Alert>
        ) : null}

        <section aria-label="采购驳回说明">
          <h3 className="text-sm font-semibold">采购为什么驳回</h3>
          <dl className="mt-2 grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2">
            <Fact label="原因" value={reasonLabel} />
            <Fact
              label="谁驳回 / 何时"
              value={`${rejection.rejectedByLabel} · ${rejection.rejectedAt}`}
            />
            <Fact
              label="说明"
              value={rejection.rejectComment}
              className="sm:col-span-2"
            />
            {rejection.estimatedCost ? (
              <Fact
                label="采购最新成本"
                value={
                  <MoneyValue
                    value={rejection.estimatedCost}
                    taxBasis="gross"
                  />
                }
              />
            ) : null}
            {rejection.estimatedMarginPercent ? (
              <Fact
                label="预计毛利"
                value={`${rejection.estimatedMarginPercent}%`}
                numeric
              />
            ) : null}
            <Fact
              label="报给采购的次数"
              value={`第 ${rejection.rejectedSubmissionNo} 次`}
              numeric
            />
          </dl>
        </section>

        <section aria-label="你已改了什么">
          <h3 className="text-sm font-semibold">相对被驳回内容，草稿有何变化</h3>
          <ul className="mt-2 space-y-1.5 text-sm" role="list">
            {rejection.draftDifference.diffSummary.map((item) => (
              <li
                key={item.field}
                className="rounded-md border border-border px-3 py-2"
              >
                <span className="font-medium">{item.field}</span>
                <span className="mt-0.5 block text-xs text-muted-foreground">
                  {item.before} → {item.after}
                </span>
              </li>
            ))}
          </ul>
          <p className="mt-2 text-xs text-muted-foreground">
            商品/价格：
            {rejection.draftDifference.changedItemOrService ||
            rejection.draftDifference.changedSalesPrice
              ? "已有改动"
              : "还没改"}
            {" · "}
            与原报采购内容是否一致：
            {rejection.draftDifference.commercialTermsUnchanged
              ? "一致（适合走低毛利申请）"
              : "已不一致"}
          </p>
        </section>

        {rejection.reviewStatus === "PENDING_LOW_MARGIN_MANAGER" &&
        rejection.activeLowMarginManagerTask ? (
          <section
            className="space-y-3 rounded-lg border border-info/30 bg-info-soft/30 p-3"
            aria-label="领导批低毛利"
          >
            <div className="flex flex-wrap items-center gap-2">
              <CircleDollarSignIcon className="size-4" aria-hidden="true" />
              <h3 className="text-sm font-semibold">等销售领导批低毛利</h3>
              <Badge variant="info">
                {WORK_ITEM_STATUS_LABEL[
                  rejection.activeLowMarginManagerTask.workItemStatus
                ] ?? rejection.activeLowMarginManagerTask.workItemStatus}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              按被驳回时的原条件申请。领导通过后才会再推给采购；驳回则回到下面三种处理方式。
              {rejection.lowMarginSubmission
                ? `（申请序号 #${rejection.lowMarginSubmission.submissionNo}）`
                : null}
            </p>
            <div className="rounded-lg border border-dashed border-border p-3">
              <p className="mb-2 text-xs font-medium text-muted-foreground">
                演示模式（模拟领导决策）
              </p>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  disabled={lowMarginDecision.isPending}
                  onClick={async () => {
                    const outcome = await lowMarginDecision.mutateAsync({
                      salesOrderId: order.id,
                      workItemId: rejection.activeLowMarginManagerTask!.workItemId,
                      decision: "APPROVE",
                      idempotencyKey: `${idempotencyKey}-lm-approve`,
                    })
                    setResult({
                      status: "succeeded",
                      title: "领导已同意低毛利",
                      description: outcome.detail,
                      reference: outcome.reference,
                    })
                  }}
                >
                  领导通过（演示）
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={lowMarginDecision.isPending}
                  onClick={async () => {
                    const outcome = await lowMarginDecision.mutateAsync({
                      salesOrderId: order.id,
                      workItemId: rejection.activeLowMarginManagerTask!.workItemId,
                      decision: "REJECT",
                      idempotencyKey: `${idempotencyKey}-lm-reject`,
                      reason: "毛利仍不可接受",
                    })
                    setResult({
                      status: "rejected",
                      title: "领导未同意低毛利",
                      description: outcome.detail,
                      reference: outcome.reference,
                    })
                  }}
                >
                  领导驳回（演示）
                </Button>
              </div>
            </div>
          </section>
        ) : null}

        {!resolved && rejection.reviewStatus === "REJECTED" ? (
          <>
            <Separator />
            <div className="grid gap-4 lg:grid-cols-3">
              <section className="space-y-3 rounded-lg border border-border p-3">
                <div className="flex items-center gap-2">
                  <RefreshCwIcon className="size-4" aria-hidden="true" />
                  <h3 className="text-sm font-semibold">改完再报给采购</h3>
                </div>
                <p className="text-xs text-muted-foreground">
                  需要改商品/服务或销售价，并写明客户已确认。会作为新一版再给采购确认。
                </p>
                <form
                  className="space-y-3"
                  onSubmit={(e) => {
                    e.preventDefault()
                    void priceForm.handleSubmit()
                  }}
                >
                  <priceForm.AppField name="unitPriceGross">
                    {(field) => (
                      <field.TextField
                        label="调整后含税单价"
                        placeholder="例如 720.00"
                        description="此单价对全部明细统一生效；多行明细需分别调整时请走「发起改单」。"
                      />
                    )}
                  </priceForm.AppField>
                  <priceForm.AppField name="note">
                    {(field) => (
                      <field.TextareaField
                        label="调整说明 / 客户确认情况"
                        rows={2}
                        placeholder="客户已确认改价…"
                      />
                    )}
                  </priceForm.AppField>
                  <priceForm.AppForm>
                    <priceForm.SubmitButton
                      label="先保存改价"
                      pendingLabel="保存中"
                    />
                  </priceForm.AppForm>
                  {priceSavedAt ? (
                    <DraftSaveIndicator state="saved" savedAt={priceSavedAt} />
                  ) : null}
                </form>
                <Button
                  type="button"
                  size="sm"
                  className="w-full"
                  disabled={!canResubmit || resolveMutation.isPending}
                  title={resubmitBlockerText ?? undefined}
                  onClick={() =>
                    setConfirm({
                      action: "RESUBMIT_CHANGED_TERMS",
                      title: "确认改完再报给采购",
                      effects: [
                        "按当前草稿生成新一版报给采购",
                        "采购会收到新的确认待办",
                        "以前被驳回的那一版记录仍保留",
                      ],
                    })
                  }
                >
                  改完再报给采购
                </Button>
                {!canResubmit && resubmitBlockerText ? (
                  <p className="text-xs text-warning">{resubmitBlockerText}</p>
                ) : null}
              </section>

              <section className="space-y-3 rounded-lg border border-border p-3">
                <div className="flex items-center gap-2">
                  <CircleDollarSignIcon className="size-4" aria-hidden="true" />
                  <h3 className="text-sm font-semibold">按原条件请领导批</h3>
                </div>
                <p className="text-xs text-muted-foreground">
                  不改商品和价格，接受较低毛利。先由销售领导批，通过前不会再推给采购。
                </p>
                <form
                  className="space-y-3"
                  onSubmit={(e) => {
                    e.preventDefault()
                    void lowMarginForm.handleSubmit()
                  }}
                >
                  <lowMarginForm.AppField name="reason">
                    {(field) => (
                      <field.TextareaField
                        label="为什么接受低毛利"
                        rows={3}
                        placeholder="说明业务依据、客户重要性等…"
                      />
                    )}
                  </lowMarginForm.AppField>
                  <lowMarginForm.AppForm>
                    <lowMarginForm.SubmitButton
                      label="提交给领导"
                      pendingLabel="校验中"
                      disabled={!canLowMargin}
                    />
                  </lowMarginForm.AppForm>
                </form>
              </section>

              <section className="space-y-3 rounded-lg border border-border p-3">
                <div className="flex items-center gap-2">
                  <BanIcon className="size-4" aria-hidden="true" />
                  <h3 className="text-sm font-semibold">不做了，作废本单</h3>
                </div>
                <p className="text-xs text-muted-foreground">
                  本单尚未生效时可作废。历史与驳回原因会保留，作废后不能恢复。
                </p>
                <form
                  className="space-y-3"
                  onSubmit={(e) => {
                    e.preventDefault()
                    void voidForm.handleSubmit()
                  }}
                >
                  <voidForm.AppField name="reason">
                    {(field) => (
                      <field.TextareaField
                        label="作废原因"
                        rows={3}
                        placeholder="客户取消 / 谈不拢条件…"
                      />
                    )}
                  </voidForm.AppField>
                  <voidForm.AppForm>
                    <voidForm.SubmitButton
                      label="确认作废"
                      pendingLabel="校验中"
                      disabled={!canVoid}
                    />
                  </voidForm.AppForm>
                </form>
              </section>
            </div>
          </>
        ) : null}

        <FormalActionConfirmDialog
          open={confirm != null}
          onOpenChange={(open) => {
            if (!open) setConfirm(null)
          }}
          title={confirm?.title ?? "确认操作"}
          actionLabel="提交"
          confirmLabel="确认执行"
          fromStatus={{ label: "采购未通过", tone: "warning" }}
          toStatus={{ label: "处理中", tone: "info" }}
          lockedFields={["销售单号", "被驳回的内容"]}
          effects={confirm?.effects ?? []}
          nextDepartment="采购 / 销售领导"
          onConfirm={async () => {
            if (!confirm) return
            try {
              const outcome = await resolveMutation.mutateAsync({
                salesOrderId: order.id,
                action: confirm.action,
                idempotencyKey: `${idempotencyKey}-${confirm.action}`,
                lowMarginReason: pendingPayload.lowMarginReason,
                voidReason: pendingPayload.voidReason,
              })
              setResult({
                status:
                  outcome.outcome === "VOIDED_AFTER_PROCUREMENT_REJECTION"
                    ? "rejected"
                    : "succeeded",
                title:
                  outcome.outcome === "CHANGED_TERMS_RESUBMITTED"
                    ? "已改完并再报给采购"
                    : outcome.outcome ===
                        "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED"
                      ? "已提交，等领导批低毛利"
                      : "本单已作废",
                description: outcome.detail,
                reference: outcome.reference,
              })
            } catch (error) {
              const message =
                error instanceof Error ? error.message : "操作失败"
              setResult({
                status: "blocked",
                title: resultText.operationBlocked,
                description:
                  message === "NO_COMMERCIAL_CHANGE"
                    ? "还没改商品或价格，不能走「改完再报」。请先保存改价或改明细。"
                    : message,
                reference: order.documentNumber,
              })
            } finally {
              setConfirm(null)
            }
          }}
        />
      </CardContent>
    </Card>
  )
}

function friendlyResubmitBlocker(reason: string): string {
  if (
    reason.includes("尚无改品") ||
    reason.includes("改品/改价") ||
    reason.includes("NO_COMMERCIAL")
  ) {
    return "还没改商品或价格。请先保存改价，再点「改完再报给采购」。"
  }
  return reason
}

function Fact({
  label,
  value,
  numeric,
  className,
}: {
  label: string
  value: React.ReactNode
  numeric?: boolean
  className?: string
}) {
  return (
    <div className={`bg-background px-3 py-2 ${className ?? ""}`}>
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className={`mt-0.5 text-sm font-medium ${numeric ? "num" : ""}`}>
        {value}
      </dd>
    </div>
  )
}
