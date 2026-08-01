"use client"

import * as React from "react"
import { z } from "zod"
import { ShieldCheckIcon } from "lucide-react"

import {
  FormalActionConfirmDialog,
  FormalActionResult,
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
  CARD_APPROVAL_TYPE_LABEL,
} from "@/mock/sales-orders"
import {
  useClaimCardSalesApprovalMutation,
  useCompleteCardSalesApprovalMutation,
} from "@/features/sales-orders/queries"
import type {
  CardSalesApproval,
  SalesOrderListItem,
} from "@/features/sales-orders/types"

const rejectSchema = z.object({
  reasonCode: z.string().trim().min(2, "请填写驳回原因码"),
  comment: z.string().trim().min(4, "请填写驳回说明"),
})

type CardSalesApprovalPanelProps = {
  order: SalesOrderListItem
  approval: CardSalesApproval
}

/**
 * 卡券双审批：领导 / 运营共用任务处理器形态。
 * claimToken 仅存会话内存，不进 URL；对象中心无绕过任务的状态按钮。
 */
export function CardSalesApprovalPanel({
  order,
  approval,
}: CardSalesApprovalPanelProps) {
  const claimMutation = useClaimCardSalesApprovalMutation()
  const completeMutation = useCompleteCardSalesApprovalMutation()

  /** claimToken 仅会话内存，不写入 query cache / URL */
  const claimRef = React.useRef<{
    workItemId: string
    claimToken: string
    leaseVersion: number
  } | null>(null)

  const [result, setResult] = React.useState<{
    status: "succeeded" | "rejected" | "blocked"
    title: string
    description: string
    reference: string
  } | null>(null)
  const [confirmApprove, setConfirmApprove] = React.useState(false)
  const [confirmReject, setConfirmReject] = React.useState(false)
  const [rejectPayload, setRejectPayload] = React.useState<{
    reasonCode: string
    comment: string
  } | null>(null)

  const rejectForm = useAppForm({
    defaultValues: { reasonCode: "", comment: "" },
    validators: { onChange: rejectSchema },
    onSubmit: async ({ value }) => {
      setRejectPayload({
        reasonCode: value.reasonCode.trim(),
        comment: value.comment.trim(),
      })
      setConfirmReject(true)
    },
  })

  const claimedHere =
    claimRef.current?.workItemId === approval.workItemId &&
    approval.workItemStatus === "CLAIMED"
  const canDecide =
    claimedHere &&
    approval.allowedActions.includes("APPROVE") &&
    Boolean(claimRef.current)

  return (
    <Card size="sm" className="border-info/40">
      <CardHeader className="border-b">
        <div className="flex flex-wrap items-center gap-2">
          <ShieldCheckIcon className="size-4 text-info" aria-hidden="true" />
          <CardTitle>卡券销售审批任务</CardTitle>
          <Badge variant="info">
            {CARD_APPROVAL_TYPE_LABEL[approval.workItemType]}
          </Badge>
          <Badge variant="secondary">{approval.workItemStatus}</Badge>
        </div>
        <CardDescription>
          与 W02 共用 handlerKey / CompleteWorkItemEnvelope。对象中心不提供绕过任务的审批按钮；
          claimToken 仅存会话内存。
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
              { label: "任务", value: approval.workItemId },
            ]}
          />
        ) : null}

        <Alert variant="info">
          <AlertTitle>冻结提交（只读）</AlertTitle>
          <AlertDescription>
            {approval.frozenSubmissionSummary}
            <span className="mt-1 block num text-xs">
              任务 {approval.workItemId} · 指纹 {approval.subjectHash} · 版本{" "}
              {approval.subjectVersion} · 期望状态{" "}
              {approval.expectedReviewStatus}
            </span>
          </AlertDescription>
        </Alert>

        {approval.claimedByLabel ? (
          <p className="text-xs text-muted-foreground">
            领取人 {approval.claimedByLabel}
            {approval.leaseExpiresAt
              ? ` · 租约至 ${new Date(approval.leaseExpiresAt).toLocaleString("zh-CN")}`
              : null}
          </p>
        ) : (
          <p className="text-xs text-muted-foreground">
            任务待领取。领取后才显示通过/驳回；他人领取时本区只读。
          </p>
        )}

        <div className="flex flex-wrap gap-2">
          {approval.allowedActions.includes("CLAIM") ? (
            <Button
              type="button"
              size="sm"
              disabled={claimMutation.isPending}
              onClick={async () => {
                const lease = await claimMutation.mutateAsync({
                  workItemId: approval.workItemId,
                })
                claimRef.current = {
                  workItemId: approval.workItemId,
                  claimToken: lease.claimToken,
                  leaseVersion: lease.leaseVersion,
                }
                setResult({
                  status: "succeeded",
                  title: "任务已领取",
                  description:
                    "claimToken 已写入会话内存，未进入 URL。请在租约有效期内完成决定。",
                  reference: `CLAIM-${approval.workItemId}`,
                })
              }}
            >
              领取任务
            </Button>
          ) : null}

          {canDecide ? (
            <>
              <Button
                type="button"
                size="sm"
                disabled={completeMutation.isPending}
                onClick={() => setConfirmApprove(true)}
              >
                {approval.workItemType === "CARD_SALES_MANAGER_APPROVAL"
                  ? "领导通过"
                  : "运营通过并生效"}
              </Button>
            </>
          ) : null}
        </div>

        {canDecide ? (
          <form
            className="max-w-md space-y-3 rounded-lg border border-border p-3"
            onSubmit={(e) => {
              e.preventDefault()
              void rejectForm.handleSubmit()
            }}
          >
            <h3 className="text-sm font-semibold">驳回至销售</h3>
            <rejectForm.AppField name="reasonCode">
              {(field) => (
                <field.TextField
                  label="驳回原因码"
                  placeholder="例如 CONTENT_INCOMPLETE"
                />
              )}
            </rejectForm.AppField>
            <rejectForm.AppField name="comment">
              {(field) => (
                <field.TextareaField
                  label="驳回说明"
                  rows={2}
                  placeholder="结构化说明，修改后须从领导审批重启"
                />
              )}
            </rejectForm.AppField>
            <rejectForm.AppForm>
              <rejectForm.SubmitButton
                label="驳回"
                pendingLabel="校验中"
              />
            </rejectForm.AppForm>
          </form>
        ) : null}

        <FormalActionConfirmDialog
          open={confirmApprove}
          onOpenChange={setConfirmApprove}
          title={
            approval.workItemType === "CARD_SALES_MANAGER_APPROVAL"
              ? "确认领导通过"
              : "确认运营通过并生效"
          }
          actionLabel="通过"
          confirmLabel="确认通过"
          fromStatus={{
            label:
              approval.workItemType === "CARD_SALES_MANAGER_APPROVAL"
                ? "待销售领导审批"
                : "待运营审批",
            tone: "warning",
          }}
          toStatus={{
            label:
              approval.workItemType === "CARD_SALES_MANAGER_APPROVAL"
                ? "待运营审批"
                : "已生效",
            tone: "success",
          }}
          lockedFields={["冻结提交", "subjectHash", "任务租约"]}
          effects={
            approval.workItemType === "CARD_SALES_MANAGER_APPROVAL"
              ? [
                  "追加领导 sales_order_review 与 workflow_action",
                  "完成当前任务",
                  "原子创建唯一运营审批任务",
                ]
              : [
                  "追加运营审批事实与 workflow_action",
                  "形成首个正式销售版本与应收",
                  "写入执行投影 outbox",
                ]
          }
          nextDepartment={
            approval.workItemType === "CARD_SALES_MANAGER_APPROVAL"
              ? "运营"
              : "票款与执行投影"
          }
          onConfirm={async () => {
            const claim = claimRef.current
            if (!claim) return
            try {
              const outcome = await completeMutation.mutateAsync({
                workItemId: approval.workItemId,
                workItemType: approval.workItemType,
                decision: "APPROVE",
                claimToken: claim.claimToken,
                leaseVersion: claim.leaseVersion,
                idempotencyKey: `card-approve-${approval.workItemId}`,
              })
              claimRef.current = null
              setResult({
                status: "succeeded",
                title:
                  outcome.outcome === "MANAGER_APPROVED"
                    ? "领导已通过"
                    : "运营已通过，销售单生效",
                description: outcome.detail,
                reference: outcome.reference,
              })
            } catch {
              setResult({
                status: "blocked",
                title: "租约失效或冲突",
                description: "请重新领取任务并重查冻结提交与版本，勿本地推进状态。",
                reference: approval.workItemId,
              })
            }
          }}
        />

        <FormalActionConfirmDialog
          open={confirmReject}
          onOpenChange={setConfirmReject}
          title="确认驳回卡券销售审批"
          actionLabel="驳回"
          confirmLabel="确认驳回"
          fromStatus={{ label: "审批中", tone: "warning" }}
          toStatus={{ label: "退回销售", tone: "destructive" }}
          lockedFields={["冻结提交", "任务身份"]}
          effects={[
            "追加驳回 sales_order_review 与 workflow_action",
            "完成当前任务，不创建下阶段任务",
            "销售修改后须从领导审批重新开始",
          ]}
          nextDepartment="销售"
          onConfirm={async () => {
            const claim = claimRef.current
            if (!claim || !rejectPayload) return
            try {
              const outcome = await completeMutation.mutateAsync({
                workItemId: approval.workItemId,
                workItemType: approval.workItemType,
                decision: "REJECT",
                claimToken: claim.claimToken,
                leaseVersion: claim.leaseVersion,
                idempotencyKey: `card-reject-${approval.workItemId}`,
                reasonCode: rejectPayload.reasonCode,
              })
              claimRef.current = null
              setResult({
                status: "rejected",
                title: "审批已驳回",
                description: outcome.detail,
                reference: outcome.reference,
              })
            } catch {
              setResult({
                status: "blocked",
                title: "租约失效或冲突",
                description: "结果未知期间不移动任务；请用原幂等键查询。",
                reference: approval.workItemId,
              })
            }
          }}
        />
      </CardContent>
    </Card>
  )
}
