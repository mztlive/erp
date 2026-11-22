"use client"

import { useRef, useState, type ReactNode } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import {
    FormalActionConfirmDialog,
    MoneyValue,
    SensitiveValue,
    WorkspaceTaskFooter,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import {
    mapDocumentApprovalViewDto,
    type ApprovalCommandView,
} from "@/features/approval-workflow/types"
import { getErrorMessage } from "@/lib/api"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { classifyFormalCommandError } from "@/lib/formal-command"
import {
    useAccountRevealMutation,
    useRequestCommandMutation,
} from "../hooks/queries"
import { requestStatusLabel } from "../lib/display"
import type {
    PaymentRequest,
    PaymentTaskContext,
    RequestCommand,
} from "../types"
import { RequestDraftDialog } from "./request-draft-dialog"
import { RequestPaymentExecution } from "./request-payment-execution"
import {
    RequestDetailContent,
    RequestDetailWorkspaceContent,
} from "./request-detail-content"

type RequestDetailChrome = (slots: {
    tabs?: ReactNode
    secondaryActions?: ReactNode
    primaryAction?: ReactNode
}) => ReactNode

export function RequestDetail({
    request,
    task,
    onDecisionApplied,
    onTaskCompleted,
    renderChrome,
}: {
    request: PaymentRequest
    task?: PaymentTaskContext
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onTaskCompleted?: () => void
    renderChrome?: RequestDetailChrome
}) {
    const [editOpen, setEditOpen] = useState(false)
    const [submitOpen, setSubmitOpen] = useState(false)
    const [submitUnknown, setSubmitUnknown] = useState(false)
    const [reasonAction, setReasonAction] = useState<
        "withdraw" | "close" | null
    >(null)
    const command = useRequestCommandMutation()
    const reveal = useAccountRevealMutation()
    const profile = useAccountProfileQuery()
    const submitIdentity = useRef<RequestCommand | null>(null)
    const approval = request.approval
        ? mapDocumentApprovalViewDto(request.approval)
        : undefined
    const actions = new Set(
        request.allowed_actions.map((action) => action.toUpperCase()),
    )
    const isApprovalTask =
        task?.workItemType === "DOCUMENT_APPROVAL" &&
        (task.allowedActions.includes("APPROVE") ||
            task.allowedActions.includes("REJECT"))
    const isPaymentTask =
        task?.workItemType === "SUPPLIER_PAYMENT_EXECUTION" &&
        task.allowedActions.includes("PROCESS")
    const scopedWorkItemId =
        isApprovalTask || isPaymentTask ? task?.workItemId : undefined
    const resubmission = useResubmissionReason({
        open: submitOpen,
        required: request.status === "rejected",
        id: "ppr-submit-resubmission-reason",
        disabled: command.isPending || submitUnknown,
    })
    const submitUnsettled = command.isPending || submitUnknown
    const closeLabel =
        request.status === "paid" ? "终止付款授权" : "关闭未付部分"
    if (isPaymentTask && task) {
        return (
            <RequestPaymentExecution
                key={request.id}
                request={request}
                task={task}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }
    const account =
        request.execution_method.type === "bank_transfer" &&
        request.payment_recipient &&
        isApprovalTask &&
        task &&
        hasPermission(
            profile.data?.permissions,
            "party_bank_account:reveal",
        ) ? (
            <SensitiveValue
                id="ppr-review-recipient-account"
                label="收款账号"
                maskedValue={
                    request.payment_recipient?.account_number_masked ?? "—"
                }
                onReveal={() => reveal.mutateAsync({ request, task })}
            />
        ) : (
            <span className="num">
                {request.payment_recipient?.account_number_masked ?? "—"}
            </span>
        )
    const approvalHistory = approval ? (
        <ApprovalReadonly
            approval={approval}
            id="ppr-approval-readonly"
            presentation="history"
        />
    ) : null
    const secondaryActions = (
        <>
            {actions.has("EDIT") && (
                <Button
                    id="ppr-detail-edit"
                    variant={renderChrome ? "ghost" : "outline"}
                    size={renderChrome ? "sm" : undefined}
                    className={
                        renderChrome
                            ? detailWorkspaceStyles.quietButton
                            : undefined
                    }
                    disabled={submitUnsettled}
                    onClick={() => setEditOpen(true)}
                >
                    修改申请
                </Button>
            )}
            {actions.has("WITHDRAW") && (
                <Button
                    id="ppr-detail-withdraw"
                    variant={renderChrome ? "ghost" : "outline"}
                    size={renderChrome ? "sm" : undefined}
                    className={
                        renderChrome
                            ? detailWorkspaceStyles.quietDangerButton
                            : undefined
                    }
                    disabled={submitUnsettled}
                    onClick={() => setReasonAction("withdraw")}
                >
                    撤回申请
                </Button>
            )}
            {actions.has("CLOSE") && (
                <Button
                    id="ppr-detail-close"
                    variant={renderChrome ? "ghost" : "outline"}
                    size={renderChrome ? "sm" : undefined}
                    className={
                        renderChrome
                            ? detailWorkspaceStyles.quietDangerButton
                            : undefined
                    }
                    disabled={submitUnsettled}
                    onClick={() => setReasonAction("close")}
                >
                    {closeLabel}
                </Button>
            )}
        </>
    )
    const primaryAction = (actions.has("SUBMIT") || submitUnknown) && (
        <Button
            id="ppr-detail-submit"
            size={renderChrome ? "sm" : undefined}
            className={renderChrome ? "px-3.5" : undefined}
            disabled={command.isPending}
            onClick={() => setSubmitOpen(true)}
        >
            {submitUnknown ? "查询本次提交结果" : "提交财务审核"}
        </Button>
    )
    const submitStatus = submitUnknown && (
        <p role="status" className="text-xs text-muted-foreground">
            提交结果暂无法确认，请再次确认本次提交以查询上次提交结果。
        </p>
    )
    const content = renderChrome ? (
        <RequestDetailWorkspaceContent
            request={request}
            account={account}
            approval={approvalHistory}
            alerts={submitStatus}
            renderChrome={(tabs) =>
                renderChrome({ tabs, secondaryActions, primaryAction })
            }
        />
    ) : (
        <RequestDetailContent
            request={request}
            embedded={Boolean(task)}
            showHistory
            workItemId={scopedWorkItemId}
            account={account}
            approval={approvalHistory}
        />
    )
    const details = (
        <>
            {content}
            {isApprovalTask && task && approval && (
                <WorkspaceTaskFooter>
                    <ApprovalActionBar
                        id="ppr-approval-actions"
                        documentType="purchase_payment_request"
                        documentId={request.id}
                        allowedActions={task.allowedActions}
                        workItemId={task.workItemId}
                        expectedTaskVersion={task.taskVersion}
                        instance={approval.instance}
                        definition={approval.definition}
                        hiddenActions={[
                            "CANCEL",
                            "CANCEL_APPROVAL",
                            "OPEN_DOCUMENT",
                            "VIEW",
                        ]}
                        decisionContext={{
                            documentLabel: request.request_no,
                            amountLabel: request.total_amount,
                            impactSummary:
                                "批准后交由指定出纳按本申请逐单金额付款；驳回结束本次审核并释放未付金额。",
                        }}
                        onDecisionApplied={onDecisionApplied}
                        presentation="workspace"
                    />
                </WorkspaceTaskFooter>
            )}
            {!renderChrome && !isPaymentTask && (
                <WorkspaceTaskFooter>
                    <div className="flex flex-wrap justify-end gap-2">
                        {secondaryActions}
                        {primaryAction}
                    </div>
                </WorkspaceTaskFooter>
            )}
            <RequestDraftDialog
                open={editOpen}
                onOpenChange={setEditOpen}
                request={request}
            />
            <FormalActionConfirmDialog
                idPrefix="ppr-submit-confirm"
                open={submitOpen}
                onOpenChange={setSubmitOpen}
                pending={command.isPending}
                title="提交付款申请审核"
                actionLabel="提交"
                confirmLabel={
                    submitUnknown ? "查询本次提交结果" : "确认提交财务审核"
                }
                cancelLabel={submitUnknown ? "暂时关闭" : "返回修改"}
                fromStatus={{
                    label: requestStatusLabel[request.status],
                    tone: "neutral",
                }}
                toStatus={{ label: "待审核", tone: "info" }}
                summary={[
                    <span key="amount">
                        申请总额 <MoneyValue value={request.total_amount} />
                    </span>,
                    <span key="supplier">供应商：{request.supplier_name}</span>,
                    <span key="company">
                        我方付款主体：{request.our_company_name}
                    </span>,
                    <span key="date">
                        希望付款日：{request.desired_payment_date}
                    </span>,
                ]}
                confirmDisabled={!submitUnknown && !resubmission.valid}
                formContent={
                    <div className="space-y-4">
                        {resubmission.field}
                        <SubmissionRouteConfirmation
                            documentType="purchase_payment_request"
                            definition={approval?.definition}
                        />
                    </div>
                }
                effects={[
                    "提交后锁定本次申请金额及关联的采购、销售单据资料。",
                    "财务批准后由指定出纳登记实际付款。",
                ]}
                onConfirm={async () => {
                    submitIdentity.current ??= {
                        expected_version: request.version,
                        resubmission_reason: resubmission.reason,
                        idempotency_key: `ppr-submit:${crypto.randomUUID()}`,
                    }
                    try {
                        await command.mutateAsync({
                            id: request.id,
                            action: "submit",
                            command: submitIdentity.current,
                        })
                        submitIdentity.current = null
                        setSubmitUnknown(false)
                        setSubmitOpen(false)
                    } catch (error) {
                        const unknown =
                            classifyFormalCommandError(error) === "unknown"
                        setSubmitUnknown(unknown)
                        if (!unknown) submitIdentity.current = null
                        throw error
                    }
                }}
            />
            {!renderChrome && submitStatus}
            {reasonAction && (
                <ReasonCommandDialog
                    request={request}
                    action={reasonAction}
                    onClose={() => setReasonAction(null)}
                />
            )}
        </>
    )
    return renderChrome ? details : <div className="space-y-6">{details}</div>
}
function ReasonCommandDialog({
    request,
    action,
    onClose,
}: {
    request: PaymentRequest
    action: "withdraw" | "close"
    onClose: () => void
}) {
    const command = useRequestCommandMutation()
    const identity = useRef<RequestCommand | null>(null)
    const [unknown, setUnknown] = useState(false)
    const terminatePaidAuthorization =
        action === "close" && request.status === "paid"
    const label =
        action === "close"
            ? terminatePaidAuthorization
                ? "终止付款授权"
                : "关闭未付部分"
            : "撤回申请"
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onSubmit: z.object({
                reason: z
                    .string()
                    .trim()
                    .min(1, "请填写原因")
                    .max(2000, "原因最多 2000 字"),
            }),
        },
        onSubmit: async ({ value }) => {
            identity.current ??= {
                expected_version: request.version,
                reason: value.reason.trim(),
                idempotency_key: `ppr-${action}:${crypto.randomUUID()}`,
            }
            try {
                await command.mutateAsync({
                    id: request.id,
                    action,
                    command: identity.current,
                })
                identity.current = null
                onClose()
            } catch (error) {
                setUnknown(classifyFormalCommandError(error) === "unknown")
                if (classifyFormalCommandError(error) !== "unknown")
                    identity.current = null
            }
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !command.isPending && !unknown) onClose()
            }}
        >
            <DialogContent
                id={`ppr-${action}-dialog`}
                closeButtonId={`ppr-${action}-dialog-close`}
            >
                <DialogHeader>
                    <DialogTitle>{label}</DialogTitle>
                    <DialogDescription>
                        {action === "close"
                            ? terminatePaidAuthorization
                                ? "终止本申请的付款授权，已付记录和原审核记录保留；后续付款冲正不恢复原付款授权，需重新申请并审核后才能再次付款。"
                                : "释放本申请剩余待付金额，并终止相应付款资格；已付记录和原审核记录保留，付款冲正后不恢复原付款授权。"
                            : "结束本次审核并释放未付金额，重新提交须重新核对余额和财务审核。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    id={`ppr-${action}-form`}
                    className="space-y-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id={`ppr-${action}-reason`}
                                label="原因"
                                required
                                disabled={command.isPending || unknown}
                            />
                        )}
                    </form.AppField>
                    {!terminatePaidAuthorization && (
                        <p className="text-body-compact">
                            本次释放未付金额{" "}
                            <MoneyValue value={request.unpaid_amount} />
                        </p>
                    )}
                    {command.isError && (
                        <p
                            role="alert"
                            className="text-body-compact text-destructive"
                        >
                            {getErrorMessage(command.error, "操作失败，请重试")}
                        </p>
                    )}
                    {unknown && (
                        <p className="text-body-compact">
                            操作结果暂无法确认，请重新点击确认以查看上次提交结果。
                        </p>
                    )}
                    <DialogFooter>
                        <Button
                            id={`ppr-${action}-cancel`}
                            variant="outline"
                            type="button"
                            disabled={command.isPending || unknown}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <Button
                            id={`ppr-${action}-confirm`}
                            type="submit"
                            disabled={command.isPending}
                        >
                            {unknown ? "查询本次结果" : `确认${label}`}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
