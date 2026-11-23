"use client"

import { useRef, useState, type ReactNode } from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import {
    FormalActionConfirmDialog,
    MoneyValue,
    SensitiveValue,
    WorkspaceTaskFooter,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import { ApprovalActionBar } from "@/features/approval-workflow/components/approval-action-bar"
import { ApprovalReadonly } from "@/features/approval-workflow/components/approval-readonly"
import { ApprovalWithdrawDialog } from "@/features/approval-workflow/components/approval-withdraw-dialog"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import {
    mapDocumentApprovalViewDto,
    type ApprovalCommandView,
} from "@/features/approval-workflow/types"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { getErrorMessage } from "@/lib/api"
import { compareDecimal } from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import {
    useAccountRevealMutation,
    useRequestCommandMutation,
} from "../hooks/queries"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import { paymentReviewBasis } from "../lib/review"
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

type DestructiveActionKind = "withdraw" | "close"

export function RequestDetail({
    request,
    task,
    onDecisionApplied,
    onTaskCompleted,
    renderChrome,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    request: PaymentRequest
    task?: PaymentTaskContext
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onTaskCompleted?: () => void
    renderChrome?: RequestDetailChrome
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
    const [editOpen, setEditOpen] = useState(false)
    const [submitOpen, setSubmitOpen] = useState(false)
    const [submitUnknown, setSubmitUnknown] = useState(false)
    const [reasonAction, setReasonAction] =
        useState<DestructiveActionKind | null>(null)
    const [unknownReasonAction, setUnknownReasonAction] =
        useState<DestructiveActionKind | null>(null)
    const command = useRequestCommandMutation()
    const reveal = useAccountRevealMutation()
    const profile = useAccountProfileQuery()
    const submitIdentity = useRef<RequestCommand | null>(null)
    const reasonIdentities = useRef<
        Partial<Record<DestructiveActionKind, RequestCommand>>
    >({})
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
    const hasUnknownCommand = submitUnknown || unknownReasonAction !== null
    const submitUnsettled = command.isPending || hasUnknownCommand
    const closeLabel =
        request.status === "paid" ? "终止付款授权" : "关闭未付部分"
    const labelForAction = (kind: DestructiveActionKind) =>
        kind === "close" ? closeLabel : "撤回申请"

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
                    disabled={
                        command.isPending ||
                        (hasUnknownCommand &&
                            unknownReasonAction !== "withdraw")
                    }
                    onClick={() => setReasonAction("withdraw")}
                >
                    {unknownReasonAction === "withdraw"
                        ? "查询撤回结果"
                        : "撤回申请"}
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
                    disabled={
                        command.isPending ||
                        (hasUnknownCommand && unknownReasonAction !== "close")
                    }
                    onClick={() => setReasonAction("close")}
                >
                    {unknownReasonAction === "close"
                        ? "查询关闭结果"
                        : closeLabel}
                </Button>
            )}
        </>
    )
    const primaryAction = (actions.has("SUBMIT") || submitUnknown) && (
        <Button
            id="ppr-detail-submit"
            size={renderChrome ? "sm" : undefined}
            className={renderChrome ? "px-3.5" : undefined}
            disabled={command.isPending || unknownReasonAction !== null}
            onClick={() => setSubmitOpen(true)}
        >
            {submitUnknown ? "查询本次提交结果" : "提交财务审核"}
        </Button>
    )
    const unknownAlerts =
        submitUnknown || unknownReasonAction ? (
            <div className="space-y-2.5">
                {submitUnknown ? (
                    <Alert variant="warning">
                        <AlertTitle>提交财务审核结果待确认</AlertTitle>
                        <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                            <span>
                                提交结果暂无法确认，请勿重复操作；点击右侧按钮使用原提交信息查询结果。
                            </span>
                            <Button
                                id="ppr-submit-query-result"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={command.isPending}
                                onClick={() => setSubmitOpen(true)}
                            >
                                查询本次提交结果
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : null}
                {unknownReasonAction ? (
                    <Alert variant="warning">
                        <AlertTitle>
                            {labelForAction(unknownReasonAction)}结果待确认
                        </AlertTitle>
                        <AlertDescription className="flex flex-wrap items-center justify-between gap-3">
                            <span>
                                本次操作结果暂无法确认，请勿重复发起其他变更；点击右侧按钮查询原提交结果。
                            </span>
                            <Button
                                id={`ppr-${unknownReasonAction}-query-result`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={command.isPending}
                                onClick={() =>
                                    setReasonAction(unknownReasonAction)
                                }
                            >
                                查询本次提交结果
                            </Button>
                        </AlertDescription>
                    </Alert>
                ) : null}
            </div>
        ) : null

    const content = renderChrome ? (
        <RequestDetailWorkspaceContent
            request={request}
            account={account}
            approval={approvalHistory}
            alerts={unknownAlerts}
            onSelectFile={onSelectFile}
            activeFileId={activeFileId}
            hideEmbeddedMaterials={hideEmbeddedMaterials}
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
            onSelectFile={onSelectFile}
            activeFileId={activeFileId}
            hideEmbeddedMaterials={hideEmbeddedMaterials}
        />
    )
    const basis = paymentReviewBasis(request)
    const highRiskReasons: string[] = []
    if (basis.exceeds) {
        highRiskReasons.push("存在采购单的申请未付金额超过当前可申请余额")
    }
    if (request.lines.some((line) => line.source_changed)) {
        highRiskReasons.push("采购或收款依据已发生变化，请逐项核对")
    }
    try {
        if (compareDecimal(request.total_amount, "100000", 2) >= 0) {
            highRiskReasons.push(
                "本次付款申请金额达到大额复核阈值（≥ ¥100,000.00）",
            )
        }
    } catch {
        // 金额格式异常时不阻断页面渲染
    }
    const highRisk = highRiskReasons.length > 0
    const hasManageActions =
        actions.has("EDIT") ||
        actions.has("WITHDRAW") ||
        actions.has("CLOSE") ||
        actions.has("SUBMIT") ||
        submitUnknown
    const details = (
        <>
            {!renderChrome && unknownAlerts}
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
                        highRisk={highRisk}
                        highRiskReasons={highRiskReasons}
                        decisionContext={{
                            documentLabel: `${request.supplier_name} · ${request.request_no}`,
                            amountLabel: `¥${request.total_amount}`,
                            currentNodeLabel:
                                approval.instance?.currentNodeName,
                            impactSummary:
                                "批准后交由指定出纳按本申请逐单金额付款；驳回结束本次审核并释放未付金额。",
                        }}
                        onDecisionApplied={onDecisionApplied}
                        presentation="workspace"
                    />
                </WorkspaceTaskFooter>
            )}
            {!renderChrome && !isPaymentTask && hasManageActions && (
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
                    tone: requestStatusTone[request.status],
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
                        希望付款日：
                        <span className="num">
                            {request.desired_payment_date}
                        </span>
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
                        toast.add({
                            title: "已提交财务审核",
                            description: `${request.request_no} 已进入财务审核流程。`,
                            type: "success",
                        })
                    } catch (error) {
                        const unknown =
                            classifyFormalCommandError(error) === "unknown"
                        setSubmitUnknown(unknown)
                        if (!unknown) submitIdentity.current = null
                        throw error
                    }
                }}
            />
            {reasonAction === "withdraw" && (
                <ApprovalWithdrawDialog
                    open
                    onOpenChange={(open) => {
                        if (!open && !command.isPending) setReasonAction(null)
                    }}
                    idPrefix="ppr-withdraw"
                    reasonId="ppr-withdraw-reason"
                    title="撤回申请"
                    description="撤回后可修改申请并重新提交财务审核，请填写撤回原因。"
                    actionLabel="撤回申请"
                    confirmLabel="确认撤回申请"
                    unknownConfirmLabel="查询本次提交结果"
                    cancelLabel="返回修改"
                    unknownCancelLabel="暂时关闭"
                    fromStatus={{
                        label: requestStatusLabel[request.status],
                        tone: requestStatusTone[request.status],
                    }}
                    toStatus={{ label: "已撤回", tone: "void" }}
                    summary={[
                        <span key="no" className="num">
                            申请单号：{request.request_no}
                        </span>,
                        <span key="supplier">
                            供应商：{request.supplier_name}
                        </span>,
                        <span key="unpaid">
                            本次释放未付金额：
                            <MoneyValue value={request.unpaid_amount} />
                        </span>,
                    ]}
                    irreversibleEffects={[
                        "立即结束本次财务审核并释放未付金额占用。",
                        "再次提交审核时将重新校验采购单余额与审核流程。",
                    ]}
                    reasonLabel="撤回原因"
                    reasonPlaceholder="请输入撤回原因"
                    initialReason={
                        reasonIdentities.current.withdraw?.reason ?? ""
                    }
                    pending={command.isPending}
                    unknown={unknownReasonAction === "withdraw"}
                    onUnknownChange={(isUnknown) => {
                        setUnknownReasonAction(isUnknown ? "withdraw" : null)
                    }}
                    allowCloseWhenUnknown
                    unknownHint="操作结果暂无法确认，可点击「查询本次提交结果」核对，或点击「暂时关闭」稍后在页面顶部继续查询。"
                    createIdempotencyKey={() =>
                        `ppr-withdraw:${crypto.randomUUID()}`
                    }
                    onSubmitWithdraw={async ({ reason, idempotencyKey }) => {
                        const identity: RequestCommand = reasonIdentities
                            .current.withdraw ?? {
                            expected_version: request.version,
                            reason,
                            idempotency_key: idempotencyKey,
                        }
                        reasonIdentities.current.withdraw = identity
                        await command.mutateAsync({
                            id: request.id,
                            action: "withdraw",
                            command: identity,
                        })
                        delete reasonIdentities.current.withdraw
                        setUnknownReasonAction(null)
                        toast.add({
                            title: "撤回申请已完成",
                            description: `${request.request_no} 状态已更新。`,
                            type: "success",
                        })
                    }}
                    onSubmitError={(_error, isUnknown) => {
                        if (!isUnknown) {
                            delete reasonIdentities.current.withdraw
                        }
                    }}
                />
            )}
            {reasonAction === "close" && (
                <DestructiveReasonConfirmDialog
                    key={reasonAction}
                    request={request}
                    action={reasonAction}
                    unknown={unknownReasonAction === reasonAction}
                    storedIdentity={reasonIdentities.current[reasonAction]}
                    onIdentityChange={(next) => {
                        if (next) reasonIdentities.current[reasonAction] = next
                        else delete reasonIdentities.current[reasonAction]
                    }}
                    onUnknownChange={(isUnknown) => {
                        setUnknownReasonAction(isUnknown ? reasonAction : null)
                    }}
                    onClose={() => setReasonAction(null)}
                />
            )}
        </>
    )
    return renderChrome ? details : <div className="space-y-6">{details}</div>
}

function DestructiveReasonConfirmDialog({
    request,
    action,
    unknown,
    storedIdentity,
    onIdentityChange,
    onUnknownChange,
    onClose,
}: {
    request: PaymentRequest
    action: DestructiveActionKind
    unknown: boolean
    storedIdentity?: RequestCommand
    onIdentityChange: (next: RequestCommand | null) => void
    onUnknownChange: (unknown: boolean) => void
    onClose: () => void
}) {
    const command = useRequestCommandMutation()
    const [confirmError, setConfirmError] = useState<string | null>(null)
    const terminatePaidAuthorization =
        action === "close" && request.status === "paid"
    const label =
        action === "close"
            ? terminatePaidAuthorization
                ? "终止付款授权"
                : "关闭未付部分"
            : "撤回申请"
    const form = useAppForm({
        defaultValues: { reason: storedIdentity?.reason ?? "" },
        validators: {
            onChange: z.object({
                reason: z
                    .string()
                    .trim()
                    .min(1, "请填写原因")
                    .max(2000, "原因最多 2000 字"),
            }),
        },
        onSubmit: async () => undefined,
    })
    const reason = useStore(form.store, (state) => state.values.reason)

    const summary: ReactNode[] = [
        <span key="no" className="num">
            申请单号：{request.request_no}
        </span>,
        <span key="supplier">供应商：{request.supplier_name}</span>,
        terminatePaidAuthorization ? (
            <span key="paid">
                申请金额：
                <MoneyValue value={request.total_amount} />
            </span>
        ) : (
            <span key="unpaid">
                本次释放未付金额：
                <MoneyValue value={request.unpaid_amount} />
            </span>
        ),
    ]

    const irreversibleEffects =
        action === "close"
            ? terminatePaidAuthorization
                ? [
                      "终止本申请的付款授权，已付记录和原审核记录继续保留。",
                      "后续即使发生付款冲正也不会恢复原付款授权，如需再次付款须重新申请并审核。",
                  ]
                : [
                      "释放本申请剩余待付金额并终止后续付款资格，已付记录和原审核记录继续保留。",
                      "后续付款冲正不恢复原付款授权，无法撤销本次关闭。",
                  ]
            : [
                  "立即结束本次财务审核并释放未付金额占用。",
                  "再次提交审核时将重新校验采购单余额与审核流程。",
              ]

    return (
        <FormalActionConfirmDialog
            idPrefix={`ppr-${action}`}
            open
            onOpenChange={(open) => {
                if (!open && !command.isPending) onClose()
            }}
            pending={command.isPending}
            title={label}
            description={
                action === "close"
                    ? terminatePaidAuthorization
                        ? "请核对影响并填写终止付款授权的原因。"
                        : "请核对释放金额与业务影响，填写关闭原因后确认。"
                    : "撤回后可修改申请并重新提交财务审核，请填写撤回原因。"
            }
            actionLabel={label}
            confirmLabel={unknown ? "查询本次提交结果" : `确认${label}`}
            cancelLabel={unknown ? "暂时关闭" : "返回修改"}
            actionVariant="destructive"
            fromStatus={{
                label: requestStatusLabel[request.status],
                tone: requestStatusTone[request.status],
            }}
            toStatus={
                action === "close"
                    ? { label: "已关闭", tone: "neutral" }
                    : { label: "已撤回", tone: "neutral" }
            }
            summary={summary}
            irreversibleEffects={irreversibleEffects}
            confirmDisabled={
                !unknown && (!reason.trim() || reason.trim().length > 2000)
            }
            onConfirm={async () => {
                setConfirmError(null)
                const trimmedReason = reason.trim()
                if (
                    !unknown &&
                    (!trimmedReason || trimmedReason.length > 2000)
                ) {
                    throw new Error("请填写原因（最多 2000 字）")
                }
                const identity: RequestCommand = storedIdentity ?? {
                    expected_version: request.version,
                    reason: trimmedReason,
                    idempotency_key: `ppr-${action}:${crypto.randomUUID()}`,
                }
                onIdentityChange(identity)
                try {
                    await command.mutateAsync({
                        id: request.id,
                        action,
                        command: identity,
                    })
                    onIdentityChange(null)
                    onUnknownChange(false)
                    toast.add({
                        title: `${label}已完成`,
                        description: `${request.request_no} 状态已更新。`,
                        type: "success",
                    })
                    onClose()
                } catch (error) {
                    const isUnknown =
                        classifyFormalCommandError(error) === "unknown"
                    onUnknownChange(isUnknown)
                    if (!isUnknown) onIdentityChange(null)
                    setConfirmError(
                        getErrorMessage(error, `${label}未完成，请核对后重试`),
                    )
                    throw error
                }
            }}
            onConfirmError={() => undefined}
            formContent={
                <div className="space-y-3">
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id={`ppr-${action}-reason`}
                                label="操作原因"
                                required
                                disabled={command.isPending || unknown}
                            />
                        )}
                    </form.AppField>
                    {unknown ? (
                        <Alert variant="warning">
                            <AlertDescription>
                                操作结果暂无法确认，可点击「查询本次提交结果」核对，或点击「暂时关闭」稍后在页面顶部继续查询。
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    {confirmError ? (
                        <Alert variant="destructive">
                            <AlertDescription>{confirmError}</AlertDescription>
                        </Alert>
                    ) : null}
                </div>
            }
        />
    )
}
