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
import { resultText, versionText } from "@/lib/ui-text"

const priceSchema = z.object({
  unitPriceGross: z
    .string()
    .trim()
    .regex(/^\d+(\.\d{1,2})?$/, "请输入有效含税单价"),
  note: z.string().trim().min(2, "请填写调整说明"),
})

const lowMarginSchema = z.object({
  reason: z.string().trim().min(8, "请填写至少 8 字的承接理由"),
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
 * 采购驳回后仅三条固定出路：改品/改价重提、低毛利承接、不做并作废。
 * 不提供通用重提或恢复旧 W07 任务入口。
 */
export function ProcurementRejectionCard({
  order,
  rejection,
}: ProcurementRejectionCardProps) {
  const resolveMutation = useResolveProcurementRejectionMutation()
  const adjustMutation = useAdjustProcurementRejectionDraftMutation()
  const lowMarginDecision = useDecideLowMarginManagerMutation()

  const [result, setResult] = React.useState<FormalResult | null>(null)
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
      setResult({
        status: "succeeded",
        title: "草稿已改价（会话）",
        description:
          "相对被驳回提交已有销售价格变化；可走「改品/改价后重提」。仍不会复用旧采购二次确认任务。",
        reference: `DRAFT-${order.documentNumber}`,
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
        title: "确认申请照原条件低毛利承接",
        effects: [
          "冻结新提交与新 subjectHash",
          "创建唯一 LOW_MARGIN_MANAGER_CONFIRMATION",
          "此时不创建采购确认任务、不使销售生效",
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
        title: "确认不做并作废",
        effects: [
          "生效前销售单置为作废",
          "完整保留旧提交、采购驳回与任务历史",
          "不创建任何后继任务",
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

  return (
    <Card size="sm" className="border-warning/40">
      <CardHeader className="border-b">
        <div className="flex flex-wrap items-center gap-2">
          <ShieldAlertIcon
            className="size-4 text-warning"
            aria-hidden="true"
          />
          <CardTitle>采购驳回 · 固定出路</CardTitle>
          <Badge variant="warning">
            {rejection.reviewStatus === "PENDING_LOW_MARGIN_MANAGER"
              ? "待低毛利上级确认"
              : rejection.reviewStatus === "VOIDED"
                ? "已作废"
                : rejection.reviewStatus === "RESOLVED"
                  ? "已处理"
                  : "待销售处理"}
          </Badge>
        </div>
        <CardDescription>
          仅提供改品/改价重提、照原条件低毛利承接、不做并作废三条互斥出路；不存在通用重提或恢复旧
          W07 任务入口。旧任务 {rejection.rejectedProcurementWorkItemId}{" "}
          仅作历史引用。
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
                label: "原提交",
                value: `#${rejection.rejectedSubmissionNo}`,
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
            <AlertTitle>
              处理结果 · {rejection.resolutionOutcome.reference}
            </AlertTitle>
            <AlertDescription>
              {rejection.resolutionOutcome.detail}
              {rejection.resolutionOutcome.newWorkItemId
                ? ` · 新任务 ${rejection.resolutionOutcome.newWorkItemId}`
                : null}
            </AlertDescription>
          </Alert>
        ) : null}

        <section aria-label="被驳回提交">
          <h3 className="text-sm font-semibold">被驳回提交（只读）</h3>
          <dl className="mt-2 grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2">
            <Fact
              label="提交号"
              value={`#${rejection.rejectedSubmissionNo}`}
              numeric
            />
            <Fact
              label="提交身份"
              value={rejection.rejectedSubmissionId}
              numeric
            />
            <Fact
              label={versionText.dataVersion}
              value={rejection.rejectedSubjectHash}
              numeric
            />
            <Fact
              label="采购确认"
              value={rejection.rejectedProcurementConfirmationId}
              numeric
            />
            <Fact label="驳回原因码" value={rejection.rejectReasonCode} />
            <Fact
              label="处理人 / 时间"
              value={`${rejection.rejectedByLabel} · ${rejection.rejectedAt}`}
            />
            <Fact
              label="驳回说明"
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
          </dl>
        </section>

        <section aria-label="草稿差异">
          <h3 className="text-sm font-semibold">相对被驳回提交的草稿差异</h3>
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
            改品/改价：
            {rejection.draftDifference.changedItemOrService ||
            rejection.draftDifference.changedSalesPrice
              ? "已检测到变化"
              : "未变化"}
            {" · "}
            商业条件未变：
            {rejection.draftDifference.commercialTermsUnchanged ? "是" : "否"}
          </p>
        </section>

        {rejection.reviewStatus === "PENDING_LOW_MARGIN_MANAGER" &&
        rejection.activeLowMarginManagerTask ? (
          <section
            className="space-y-3 rounded-lg border border-info/30 bg-info-soft/30 p-3"
            aria-label="低毛利上级确认"
          >
            <div className="flex flex-wrap items-center gap-2">
              <CircleDollarSignIcon className="size-4" aria-hidden="true" />
              <h3 className="text-sm font-semibold">
                低毛利上级确认（LOW_MARGIN_MANAGER_CONFIRMATION）
              </h3>
              <Badge variant="info">
                {rejection.activeLowMarginManagerTask.workItemStatus}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
              任务 {rejection.activeLowMarginManagerTask.workItemId} · 版本{" "}
              {rejection.activeLowMarginManagerTask.subjectHash}
              {rejection.lowMarginSubmission
                ? ` · 新提交 #${rejection.lowMarginSubmission.submissionNo}`
                : null}
              。商业条件须与被驳回提交一致；上级通过后才创建新采购确认，驳回则回到三条出路。
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
                    title: "低毛利承接已通过",
                    description: outcome.detail,
                    reference: outcome.reference,
                  })
                }}
              >
                上级通过（演示）
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
                    title: "低毛利承接已驳回",
                    description: outcome.detail,
                    reference: outcome.reference,
                  })
                }}
              >
                上级驳回（演示）
              </Button>
            </div>
          </section>
        ) : null}

        {!resolved && rejection.reviewStatus === "REJECTED" ? (
          <>
            <Separator />
            <div className="grid gap-4 lg:grid-cols-3">
              {/* 出路 1 */}
              <section className="space-y-3 rounded-lg border border-border p-3">
                <div className="flex items-center gap-2">
                  <RefreshCwIcon className="size-4" aria-hidden="true" />
                  <h3 className="text-sm font-semibold">改品/改价后重提</h3>
                </div>
                <p className="text-xs text-muted-foreground">
                  须确有商品/服务或销售价格变化，并附客户重新确认依据；将创建全新采购确认任务。
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
                      />
                    )}
                  </priceForm.AppField>
                  <priceForm.AppField name="note">
                    {(field) => (
                      <field.TextareaField
                        label="调整说明 / 客户确认依据"
                        rows={2}
                        placeholder="客户已确认改价…"
                      />
                    )}
                  </priceForm.AppField>
                  <priceForm.AppForm>
                    <priceForm.SubmitButton
                      label="保存改价草稿"
                      pendingLabel="保存中"
                    />
                  </priceForm.AppForm>
                </form>
                <Button
                  type="button"
                  size="sm"
                  className="w-full"
                  disabled={!canResubmit || resolveMutation.isPending}
                  title={resubmitBlocker?.reason}
                  onClick={() =>
                    setConfirm({
                      action: "RESUBMIT_CHANGED_TERMS",
                      title: "确认改品/改价后重提",
                      effects: [
                        "冻结递增提交号的新 sales_order_submission",
                        "计算新 subjectHash",
                        "原子创建唯一新 PROCUREMENT_CONFIRMATION",
                        "旧提交与旧采购二次确认任务不变",
                      ],
                    })
                  }
                >
                  重新提交采购确认
                </Button>
                {!canResubmit && resubmitBlocker ? (
                  <p className="text-xs text-warning">{resubmitBlocker.reason}</p>
                ) : null}
              </section>

              {/* 出路 2 */}
              <section className="space-y-3 rounded-lg border border-border p-3">
                <div className="flex items-center gap-2">
                  <CircleDollarSignIcon className="size-4" aria-hidden="true" />
                  <h3 className="text-sm font-semibold">照原条件低毛利承接</h3>
                </div>
                <p className="text-xs text-muted-foreground">
                  商业条件须与被驳回提交一致；须由销售上级确认，尚不会回采购。
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
                        label="低毛利承接理由"
                        rows={3}
                        placeholder="说明承担低毛利的业务依据…"
                      />
                    )}
                  </lowMarginForm.AppField>
                  <lowMarginForm.AppForm>
                    <lowMarginForm.SubmitButton
                      label="申请上级确认"
                      pendingLabel="校验中"
                      disabled={!canLowMargin}
                    />
                  </lowMarginForm.AppForm>
                </form>
              </section>

              {/* 出路 3 */}
              <section className="space-y-3 rounded-lg border border-border p-3">
                <div className="flex items-center gap-2">
                  <BanIcon className="size-4" aria-hidden="true" />
                  <h3 className="text-sm font-semibold">不做并作废</h3>
                </div>
                <p className="text-xs text-muted-foreground">
                  生效前且无有效后继任务时可作废；历史提交与驳回记录完整保留，不可恢复。
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
                        placeholder="客户取消 / 无法达成商业条件…"
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
          actionLabel="提交处理结果"
          confirmLabel="确认执行"
          fromStatus={{ label: "采购已驳回", tone: "warning" }}
          toStatus={{ label: "处理中", tone: "info" }}
          lockedFields={[
            "被驳回提交号",
            "subjectHash",
            "采购确认身份",
          ]}
          effects={confirm?.effects ?? []}
          nextDepartment="采购 / 销售上级"
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
                    ? "已改品/改价并重提"
                    : outcome.outcome ===
                        "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED"
                      ? "已申请低毛利上级确认"
                      : "销售单已作废",
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
                    ? "内容未发生改品/改价，不得冒充此路径。请先调整草稿。"
                    : message,
                reference: idempotencyKey,
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
