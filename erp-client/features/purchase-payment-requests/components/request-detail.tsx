"use client"

import {
    Fragment,
    useEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
} from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import {
    FormalActionConfirmDialog,
    MoneyValue,
    QuantityValue,
    RateValue,
    SensitiveValue,
    WorkspaceTaskFooter,
    detailWorkspaceStyles,
} from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import {
    UnifiedMaterialViewer,
    isMaterialPreviewable,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    clampZeroFixed,
    compareDecimal,
    divideFixed,
    formatCurrencyFixed,
    minFixed,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import {
    AlertTriangleIcon,
    ChevronRightIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { isCommandOutcomeUnknown } from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import { charLimitSchema } from "@/lib/validation/reason-length"
import {
    useAccountRevealMutation,
    usePaymentMaterialsQuery,
    useRequestCommandMutation,
    useRequestDownloadMutation,
} from "../hooks/queries"
import { fetchMaterialPreview } from "../api"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import { fulfillmentLabel, paymentReviewBasis } from "../lib/review"
import type {
    PaymentRequest,
    PaymentTaskContext,
    RequestCommand,
} from "../types"
import { RequestDraftDialog } from "./request-draft-dialog"
import { RequestMaterials } from "./request-materials"
import { RequestReviewHistory } from "./request-review-history"
import { RequestSubmissionHistory } from "./request-submission-history"
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
    hasExternalPrimaryAction = false,
    approvalRoundNo,
    renderResubmission,
}: {
    request: PaymentRequest
    task?: PaymentTaskContext
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onTaskCompleted?: () => void
    renderChrome?: RequestDetailChrome
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
    hasExternalPrimaryAction?: boolean
    approvalRoundNo?: number
    renderResubmission?: (request: PaymentRequest) => ReactNode
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
    const reviewId = `ppr-review-${toAutomationIdSegment(task?.workItemId ?? request.id)}`
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
        request.payment_recipient?.bank_account_id &&
        request.payment_recipient.version > 0 &&
        isApprovalTask &&
        task &&
        hasPermission(
            profile.data?.permissions,
            "party_bank_account:reveal",
        ) ? (
            <SensitiveValue
                key={`${request.payment_recipient.bank_account_id}:${request.payment_recipient.version}`}
                id={`${reviewId}-recipient-account`}
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
            id={`${reviewId}-approval-readonly`}
            presentation="history"
        />
    ) : null
    const secondaryActions = (
        <>
            {actions.has("EDIT") && (
                <Button
                    id="ppr-detail-edit"
                    type="button"
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
                    type="button"
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
                    type="button"
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
            type="button"
            variant={
                isApprovalTask || hasExternalPrimaryAction
                    ? "outline"
                    : "default"
            }
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

    const review = task ? buildPaymentReview(request) : undefined
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
    ) : task && review ? (
        <PaymentRequestTaskReview
            request={request}
            account={account}
            approval={approvalHistory}
            idPrefix={reviewId}
            approvalRoundNo={approvalRoundNo}
            resubmission={renderResubmission?.(request)}
            review={review}
            workItemId={scopedWorkItemId}
            onSelectFile={onSelectFile}
            activeFileId={activeFileId}
            hideEmbeddedMaterials={hideEmbeddedMaterials}
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
            {isApprovalTask && task && approval && review && (
                <WorkspaceTaskFooter>
                    <ApprovalActionBar
                        id={`${reviewId}-actions`}
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
                        highRisk={review.risks.length > 0}
                        highRiskReasons={review.risks.map(
                            (risk) => risk.reason,
                        )}
                        dynamicRejectPhrases={review.risks.map(
                            (risk) => risk.rejectPhrase,
                        )}
                        decisionContext={{
                            documentLabel: `${request.supplier_name} · ${request.request_no}`,
                            amountLabel: formatCurrencyFixed(
                                request.total_amount,
                                {
                                    maxScale: 4,
                                    minimumFractionDigits: 2,
                                },
                            ),
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
                        const unknown = isCommandOutcomeUnknown(error)
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
    return renderChrome ? (
        details
    ) : (
        <div className="min-w-0 space-y-6 bg-card">{details}</div>
    )
}

const DESTRUCTIVE_REASON_MAX_CHARS = 2000

function firstFieldError(errors: readonly unknown[]): string | undefined {
    for (const issue of errors) {
        if (typeof issue === "string" && issue.trim()) return issue
        if (
            issue &&
            typeof issue === "object" &&
            "message" in issue &&
            typeof issue.message === "string" &&
            issue.message.trim()
        ) {
            return issue.message
        }
    }
    return undefined
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
                reason: charLimitSchema({
                    maxChars: DESTRUCTIVE_REASON_MAX_CHARS,
                    required: true,
                    label: "操作原因",
                }),
            }),
        },
        onSubmit: async () => undefined,
    })
    const reason = useStore(form.store, (state) => state.values.reason)
    const trimmedReason = reason.trim()
    const reasonCharCount = Array.from(trimmedReason).length
    const reasonInvalid =
        !trimmedReason || reasonCharCount > DESTRUCTIVE_REASON_MAX_CHARS

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
            confirmDisabled={!unknown && reasonInvalid}
            onConfirm={async () => {
                setConfirmError(null)
                if (!unknown && reasonInvalid) {
                    throw new Error(
                        `请填写操作原因（最多 ${DESTRUCTIVE_REASON_MAX_CHARS} 字）`,
                    )
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
                    const isUnknown = isCommandOutcomeUnknown(error)
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
                    <form.Field name="reason">
                        {(field) => {
                            const fieldError = firstFieldError(
                                field.state.meta.errors,
                            )
                            const overLimit =
                                Array.from(field.state.value).length >
                                DESTRUCTIVE_REASON_MAX_CHARS
                            const showError =
                                (field.state.meta.isTouched || overLimit) &&
                                Boolean(fieldError)
                            return (
                                <ReasonTextareaField
                                    id={`ppr-${action}-reason`}
                                    name="reason"
                                    mode="chars"
                                    max={DESTRUCTIVE_REASON_MAX_CHARS}
                                    label="操作原因"
                                    placeholder={`请填写${label}原因`}
                                    required
                                    rows={3}
                                    value={field.state.value}
                                    disabled={command.isPending || unknown}
                                    error={showError ? fieldError : undefined}
                                    onChange={(next) =>
                                        field.handleChange(next)
                                    }
                                    onBlur={field.handleBlur}
                                    onCtrlEnterSubmit={() => {
                                        if (
                                            !command.isPending &&
                                            (unknown || !reasonInvalid)
                                        ) {
                                            document
                                                .getElementById(
                                                    `ppr-${action}-confirm`,
                                                )
                                                ?.click()
                                        }
                                    }}
                                />
                            )
                        }}
                    </form.Field>
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

const reviewMoney = {
    maxScale: 4,
    outputScale: 4,
    allowNegative: true,
} as const
const reviewPageSize = 20

function buildPaymentReview(request: PaymentRequest, historical = false) {
    const basis = paymentReviewBasis(request, historical)
    const rows = request.lines.map((line) => {
        const { available } = paymentReviewBasis(
            { ...request, lines: [line] },
            !basis.current,
        )
        const amount = basis.current
            ? line.unpaid_amount
            : line.requested_amount
        const over = compareDecimal(amount, available, 4) > 0
        const prepayment =
            line.prepay_gate ??
            (line.payment_term_code?.startsWith("PREPAY_") ||
                /^(?:预付|先款|款到)/.test(line.payment_term_name.trim()))
        return { line, available, amount, over, prepayment }
    })
    const risks = []
    if (basis.exceeds) {
        risks.push({
            id: "over",
            label: "超过可申请余额",
            reason: "部分采购单的申请金额超过可申请余额。",
            rejectPhrase: "申请金额超过采购单可申请余额，请调整后重新提交。",
            target: "sources",
            severe: true,
        })
    }
    if (rows.some((row) => row.prepayment)) {
        risks.push({
            id: "prepayment",
            label: "先款门禁",
            reason: "部分采购单约定先行付款，请核对预付金额与履约依据。",
            rejectPhrase: "请补充预付比例、付款节点及履约依据后重新提交。",
            target: "sources",
        })
    }
    if (
        request.execution_method.type === "bank_transfer" &&
        (!request.payment_recipient?.bank_account_id ||
            !request.payment_recipient.version ||
            !request.payment_recipient.account_number_masked ||
            request.payment_recipient.account_number_masked === "未记录")
    ) {
        risks.push({
            id: "account",
            label: "缺少收款账号",
            reason: "银行付款申请缺少完整的收款账户。",
            rejectPhrase: "请补齐并核实供应商收款账户后重新提交。",
            target: "account",
            severe: true,
        })
    }
    if (compareDecimal(request.total_amount, "100000", 4) >= 0) {
        risks.push({
            id: "large",
            label: "大额付款",
            reason: "本次申请达到大额复核阈值（≥ ¥100,000.00）。",
            rejectPhrase: "请补充大额付款依据及金额拆分说明。",
            target: "amount",
        })
    }
    if (rows.some(({ line }) => line.source_changed)) {
        risks.push({
            id: "source",
            label: "付款依据变更",
            reason: "采购或收款依据已发生变化，请逐项核对。",
            rejectPhrase: "付款依据已变化，请按最新采购及收款资料重新提交。",
            target: "sources",
        })
    }
    return { basis, rows, risks }
}

function PaymentKpi({
    label,
    basis,
    id,
    explanation,
    highlighted = false,
    children,
}: {
    label: string
    basis?: string
    id?: string
    explanation?: string
    highlighted?: boolean
    children: ReactNode
}) {
    return (
        <div
            id={id}
            tabIndex={id ? -1 : undefined}
            className={cn(
                "min-w-0 space-y-1 scroll-mt-16 rounded-md px-1.5 py-1 -mx-1.5 -my-1 transition-colors focus:outline-none focus-visible:outline-2 focus-visible:outline-ring",
                highlighted &&
                    "bg-info-soft/40 ring-1 ring-info-border motion-safe:animate-pulse",
            )}
        >
            <dt className="flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                <span>{label}</span>
                {basis ? (
                    <span className="text-tiny font-normal text-muted-foreground/80">
                        {basis}
                    </span>
                ) : null}
                {id && explanation ? (
                    <Tooltip>
                        <TooltipTrigger
                            id={`${id}-formula`}
                            aria-label={`${label}计算口径`}
                            render={
                                <Button
                                    variant="ghost"
                                    size="icon-xs"
                                    type="button"
                                    className="size-4 rounded-full border border-border/80 text-tiny text-muted-foreground hover:bg-card hover:text-foreground"
                                />
                            }
                        >
                            <span aria-hidden="true">?</span>
                        </TooltipTrigger>
                        <TooltipContent>{explanation}</TooltipContent>
                    </Tooltip>
                ) : null}
            </dt>
            <dd className="min-w-0 space-y-1">{children}</dd>
        </div>
    )
}

const LOCATE_HIGHLIGHT_MS = 2200

export function PaymentRequestTaskReview({
    request,
    account,
    approval,
    idPrefix,
    submissionVersion,
    review = buildPaymentReview(request, submissionVersion != null),
    approvalRoundNo = request.approval?.instance?.current_round_no,
    resubmission,
    workItemId,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    request: PaymentRequest
    account: ReactNode
    approval: ReactNode
    idPrefix: string
    review?: ReturnType<typeof buildPaymentReview>
    approvalRoundNo?: number
    resubmission?: ReactNode
    submissionVersion?: number
    workItemId?: string
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
    const { basis, rows, risks } = review
    const [riskOnly, setRiskOnly] = useState(false)
    const [search, setSearch] = useState("")
    const [page, setPage] = useState(0)
    const [pageSize, setPageSize] = useState(reviewPageSize)
    const [openSourceId, setOpenSourceId] = useState<string>()
    const [locatedId, setLocatedId] = useState<string>()
    const [historyOpen, setHistoryOpen] = useState(false)
    const [paymentHistoryOpen, setPaymentHistoryOpen] = useState(false)
    const locateTimerRef = useRef<number | null>(null)
    const scrollFrameRef = useRef<number | null>(null)

    useEffect(() => {
        return () => {
            if (locateTimerRef.current !== null) {
                window.clearTimeout(locateTimerRef.current)
            }
            if (scrollFrameRef.current !== null) {
                window.cancelAnimationFrame(scrollFrameRef.current)
            }
        }
    }, [])

    const bank = request.execution_method.type === "bank_transfer"
    const paymentTerms = [
        ...new Set(request.lines.map((line) => line.payment_term_name)),
    ].join("、")
    const paidRate =
        compareDecimal(basis.total, "0", 4) > 0
            ? divideFixed(
                  multiplyFixed(
                      clampZeroFixed(basis.paid, reviewMoney),
                      "100",
                      { leftMaxScale: 4, rightMaxScale: 0, outputScale: 4 },
                  ),
                  basis.total,
                  {
                      numeratorMaxScale: 4,
                      denominatorMaxScale: 4,
                      outputScale: 1,
                  },
              )
            : "0"
    const riskRows = rows.filter(
        ({ over, prepayment, line }) =>
            over || prepayment || line.source_changed,
    )
    const lineSearch = search.trim().toLowerCase()
    const filtered = (riskOnly ? riskRows : rows).filter(({ line }) =>
        [
            line.purchase_order_no,
            line.payment_term_name,
            ...line.purchase_items.map((item) => item.item_name),
        ].some((value) => value.toLowerCase().includes(lineSearch)),
    )
    const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize))
    const resolvedPage = Math.min(page, pageCount - 1)
    const visible = filtered.slice(
        resolvedPage * pageSize,
        (resolvedPage + 1) * pageSize,
    )
    const hasActiveFilter = riskOnly || Boolean(lineSearch)
    const showFilter = rows.length > 8 || riskRows.length > 0 || hasActiveFilter
    const showPagination =
        filtered.length > reviewPageSize || pageSize !== reviewPageSize
    const sourceId = `${idPrefix}-sources`
    const hasSevereRisk = risks.some((risk) => risk.severe)

    const focusTarget = (target: string, kind?: string) => {
        const matchedRisk =
            target === "sources"
                ? (riskRows.find(
                      ({ over, prepayment, line }) =>
                          (kind === "over" && over) ||
                          (kind === "prepayment" && prepayment) ||
                          (kind === "source" && line.source_changed),
                  ) ?? riskRows[0])
                : undefined
        const focusId =
            target === "sources"
                ? matchedRisk
                    ? `${sourceId}-${toAutomationIdSegment(matchedRisk.line.payable_account_id)}-toggle`
                    : sourceId
                : `${idPrefix}-${target}`
        if (target === "sources") {
            setSearch("")
            if (riskRows.length > 0) {
                setRiskOnly(true)
            }
            const nextIndex = matchedRisk ? riskRows.indexOf(matchedRisk) : 0
            setPage(nextIndex >= 0 ? Math.floor(nextIndex / pageSize) : 0)
        }
        setLocatedId(focusId)
        if (locateTimerRef.current !== null) {
            window.clearTimeout(locateTimerRef.current)
        }
        locateTimerRef.current = window.setTimeout(() => {
            locateTimerRef.current = null
            setLocatedId(undefined)
        }, LOCATE_HIGHLIGHT_MS)

        if (scrollFrameRef.current !== null) {
            window.cancelAnimationFrame(scrollFrameRef.current)
        }
        scrollFrameRef.current = window.requestAnimationFrame(() => {
            scrollFrameRef.current = window.requestAnimationFrame(() => {
                scrollFrameRef.current = null
                const element = document.getElementById(focusId)
                if (!element) return
                const reducedMotion = window.matchMedia(
                    "(prefers-reduced-motion: reduce)",
                ).matches
                element.scrollIntoView({
                    behavior: reducedMotion ? "auto" : "smooth",
                    block: "center",
                })
                element.focus({ preventScroll: true })
            })
        })
    }

    return (
        <div className="min-w-0 space-y-4 bg-card pb-4 pt-3">
            {Math.max(approvalRoundNo ?? 0, request.submission_version) >= 2
                ? resubmission
                : null}

            {risks.length > 0 && (
                <section
                    aria-label="付款风险"
                    className={cn(
                        "flex flex-wrap items-center gap-x-2 gap-y-1 rounded-r-md border-l-2 px-3 py-2 text-xs",
                        hasSevereRisk
                            ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                    )}
                >
                    <span className="inline-flex items-center gap-1.5 font-medium">
                        <AlertTriangleIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        付款风险
                    </span>
                    {risks.map((risk) => (
                        <Button
                            key={risk.id}
                            id={`${idPrefix}-risk-${risk.id}`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={cn(
                                "h-control-xs text-xs font-medium",
                                hasSevereRisk
                                    ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                    : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                            )}
                            onClick={() => focusTarget(risk.target, risk.id)}
                        >
                            {risk.label}
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    ))}
                    {riskRows.length > 0 && (
                        <Button
                            id={`${idPrefix}-risk-all`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={cn(
                                "ml-auto h-control-xs text-xs font-medium",
                                hasSevereRisk
                                    ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                    : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                            )}
                            onClick={() => focusTarget("sources")}
                        >
                            仅看风险项（
                            <span className="num">{riskRows.length}</span>）
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    )}
                </section>
            )}

            <TooltipProvider>
                <dl className="grid min-w-0 gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/payment-request:grid-cols-2 @[840px]/payment-request:grid-cols-4">
                    <PaymentKpi
                        id={`${idPrefix}-amount`}
                        label="本次申请付款金额"
                        basis="含税"
                        explanation="本次申请各采购单分配金额之和，按本次提交的申请口径展示。"
                        highlighted={locatedId === `${idPrefix}-amount`}
                    >
                        <MoneyValue
                            value={request.total_amount}
                            size="section"
                            display="exact"
                        />
                        <p className="text-tiny text-muted-foreground">
                            含税 · <span className="num">{rows.length}</span>{" "}
                            张采购单
                        </p>
                    </PaymentKpi>
                    <PaymentKpi
                        id={`${idPrefix}-available`}
                        label="可申请余额"
                        basis="采购未结 − 占用"
                        explanation="采购未结余额减去其他申请占用。付款进度按采购已结算额除以采购总额计算。"
                    >
                        <MoneyValue
                            value={basis.available}
                            display="exact"
                            className="num text-value-compact font-semibold"
                        />
                        <div
                            aria-hidden="true"
                            className="h-1.5 overflow-hidden rounded-full bg-muted"
                        >
                            <div
                                className="h-full rounded-full bg-primary"
                                style={{
                                    width: `${minFixed(paidRate, "100", { maxScale: 1, outputScale: 1 })}%`,
                                }}
                            />
                        </div>
                        <p className="text-tiny text-muted-foreground">
                            {basis.current ? "当前" : "提交时"} · 采购总额{" "}
                            <MoneyValue value={basis.total} display="exact" /> ·
                            已结算{" "}
                            <RateValue
                                value={paidRate}
                                precision={1}
                                className="text-muted-foreground"
                            />
                        </p>
                    </PaymentKpi>
                    <PaymentKpi
                        id={`${idPrefix}-unpaid`}
                        label="本申请剩余待付"
                        basis="申请 − 已结"
                        explanation="申请金额减去已结算金额与已关闭金额。"
                    >
                        <MoneyValue
                            value={request.unpaid_amount}
                            display="exact"
                            className="num text-value-compact font-semibold"
                        />
                        <p className="text-tiny text-muted-foreground">
                            已结算{" "}
                            <MoneyValue
                                value={request.paid_amount}
                                display="exact"
                            />
                            {compareDecimal(request.closed_amount, "0", 4) >
                                0 && (
                                <>
                                    {" "}
                                    · 已关闭{" "}
                                    <MoneyValue
                                        value={request.closed_amount}
                                        display="exact"
                                    />
                                </>
                            )}
                        </p>
                    </PaymentKpi>
                    <PaymentKpi
                        id={`${idPrefix}-account`}
                        label={
                            bank ? "收款账户与付款条件" : "预存款账户与付款条件"
                        }
                        basis={bank ? "银行转账" : "供应商预存款"}
                        highlighted={locatedId === `${idPrefix}-account`}
                    >
                        <div className="wrap-anywhere text-body-compact font-medium">
                            {bank
                                ? (request.payment_recipient?.account_name ??
                                  "未记录")
                                : "供应商预存款"}
                        </div>
                        <div className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
                            {bank ? (
                                <>
                                    {request.payment_recipient?.bank_name}
                                    {account}
                                </>
                            ) : (
                                request.deposit_account?.identity
                                    .external_account_ref && (
                                    <span className="num">
                                        ****
                                        {request.deposit_account.identity.external_account_ref.slice(
                                            -4,
                                        )}
                                    </span>
                                )
                            )}
                        </div>
                        <p className="wrap-anywhere text-tiny text-muted-foreground">
                            {paymentTerms || "未标注付款条件"}
                        </p>
                    </PaymentKpi>
                </dl>
            </TooltipProvider>

            <dl className="flex flex-wrap gap-x-5 gap-y-1.5 text-xs">
                {[
                    ["付款主体", request.our_company_name],
                    ["申请人", request.applicant_name],
                    [
                        "希望付款日",
                        <span key="date" className="num">
                            {request.desired_payment_date}
                        </span>,
                    ],
                    ["申请说明", request.description],
                    ["关闭原因", request.close_reason],
                ].map(([label, value]) =>
                    value ? (
                        <div
                            key={String(label)}
                            className="flex min-w-0 items-baseline gap-2"
                        >
                            <dt className="shrink-0 text-muted-foreground">
                                {label}
                            </dt>
                            <dd className="min-w-0 whitespace-pre-wrap wrap-anywhere">
                                {value}
                            </dd>
                        </div>
                    ) : null,
                )}
            </dl>

            <section
                id={sourceId}
                tabIndex={-1}
                aria-label="来源采购与应付依据"
                className={cn(
                    "min-w-0 space-y-2.5 scroll-mt-16 rounded-md focus:outline-none focus-visible:outline-2 focus-visible:outline-ring",
                    locatedId === sourceId &&
                        "bg-info-soft/30 ring-1 ring-info-border motion-safe:animate-pulse",
                )}
            >
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <h2 className="text-section-title font-semibold">
                        来源采购与应付依据
                    </h2>
                    <span className="num text-xs text-muted-foreground">
                        共 {rows.length} 项
                    </span>
                </div>
                {showFilter && (
                    <div className="flex flex-wrap items-center justify-between gap-2.5">
                        <div className="flex flex-wrap items-center gap-2">
                            <div
                                role="group"
                                aria-label="来源采购筛选"
                                className={detailWorkspaceStyles.subTabs}
                            >
                                <button
                                    id={`${sourceId}-all`}
                                    type="button"
                                    aria-pressed={!riskOnly}
                                    data-active={!riskOnly ? "true" : undefined}
                                    className={detailWorkspaceStyles.subTab}
                                    onClick={() => {
                                        setRiskOnly(false)
                                        setPage(0)
                                    }}
                                >
                                    <span>全部</span>
                                    <span className="num text-xs text-muted-foreground">
                                        {rows.length}
                                    </span>
                                </button>
                                {(riskRows.length > 0 || riskOnly) && (
                                    <button
                                        id={`${sourceId}-risk`}
                                        type="button"
                                        aria-pressed={riskOnly}
                                        data-active={
                                            riskOnly ? "true" : undefined
                                        }
                                        className={detailWorkspaceStyles.subTab}
                                        onClick={() => {
                                            setRiskOnly(true)
                                            setPage(0)
                                        }}
                                    >
                                        <span>仅看风险项</span>
                                        <span className="num text-xs text-muted-foreground">
                                            {riskRows.length}
                                        </span>
                                    </button>
                                )}
                            </div>
                            {hasActiveFilter ? (
                                <div
                                    role="status"
                                    aria-live="polite"
                                    className="flex items-center gap-1.5 text-xs text-muted-foreground"
                                >
                                    <span className="num">
                                        命中 {filtered.length} / {rows.length}{" "}
                                        项
                                    </span>
                                    <Button
                                        id={`${sourceId}-clear-filters`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        onClick={() => {
                                            setSearch("")
                                            setRiskOnly(false)
                                            setPage(0)
                                        }}
                                    >
                                        清除筛选
                                    </Button>
                                </div>
                            ) : null}
                        </div>
                        <div className="relative w-full @[560px]/payment-request:w-60">
                            <SearchIcon
                                aria-hidden="true"
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                            />
                            <Input
                                id={`${sourceId}-search`}
                                value={search}
                                onChange={(event) => {
                                    setSearch(event.target.value)
                                    setPage(0)
                                }}
                                placeholder="搜索采购单号、商品或付款条件"
                                aria-label="搜索来源采购单"
                                className="h-control-sm pr-8 pl-8 text-xs"
                            />
                            {search ? (
                                <Button
                                    id={`${sourceId}-search-clear`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="清除搜索"
                                    className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                    onClick={() => {
                                        setSearch("")
                                        setPage(0)
                                    }}
                                >
                                    <XIcon aria-hidden="true" />
                                </Button>
                            ) : null}
                        </div>
                    </div>
                )}
                <Table className="w-full table-fixed text-body-compact">
                    <TableHeader className="bg-surface-sunken text-xs text-muted-foreground [&_tr]:border-b-0 [&_th]:bg-surface-sunken [&_th]:hover:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md">
                        <TableRow>
                            <TableHead
                                scope="col"
                                className="w-2/5 px-2 py-2 text-left font-medium"
                            >
                                采购单 / 履约
                            </TableHead>
                            <TableHead
                                scope="col"
                                className="px-2 py-2 text-right font-medium"
                            >
                                {basis.current ? "申请未付" : "本次申请"}
                            </TableHead>
                            <TableHead
                                scope="col"
                                className="px-2 py-2 text-right font-medium"
                            >
                                可申请余额
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {visible.map(
                            ({ line, available, amount, over, prepayment }) => {
                                const lineId = `${sourceId}-${toAutomationIdSegment(line.payable_account_id)}`
                                const isOpen =
                                    openSourceId === line.payable_account_id
                                const hasRisk =
                                    over || prepayment || line.source_changed
                                const isRowLocated =
                                    locatedId === `${lineId}-toggle`
                                return (
                                    <Fragment key={line.payable_account_id}>
                                        <TableRow
                                            className={cn(
                                                hasRisk
                                                    ? "border-b-0"
                                                    : "border-border/60",
                                                isRowLocated &&
                                                    "bg-info-soft/35 ring-1 ring-inset ring-info-border motion-safe:animate-pulse",
                                            )}
                                        >
                                            <TableHead
                                                scope="row"
                                                className="bg-transparent px-2 py-2 text-left font-normal whitespace-normal"
                                            >
                                                <Button
                                                    id={`${lineId}-toggle`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    className="h-auto max-w-full justify-start scroll-mt-16 px-0 text-left whitespace-normal wrap-anywhere data-[located=true]:bg-info-soft/50 data-[located=true]:ring-1 data-[located=true]:ring-info-border"
                                                    aria-expanded={isOpen}
                                                    aria-controls={lineId}
                                                    data-located={
                                                        isRowLocated
                                                            ? "true"
                                                            : undefined
                                                    }
                                                    onClick={() =>
                                                        setOpenSourceId(
                                                            isOpen
                                                                ? undefined
                                                                : line.payable_account_id,
                                                        )
                                                    }
                                                >
                                                    <ChevronRightIcon
                                                        aria-hidden="true"
                                                        className={cn(
                                                            "transition-transform",
                                                            isOpen &&
                                                                "rotate-90",
                                                        )}
                                                    />
                                                    <span className="num">
                                                        {line.purchase_order_no}
                                                    </span>
                                                </Button>
                                                <span className="num block text-tiny text-muted-foreground">
                                                    V{line.purchase_version}
                                                    {line.current_fulfillment &&
                                                        ` · ${fulfillmentLabel[line.current_fulfillment] ?? "履约待核对"}`}
                                                </span>
                                            </TableHead>
                                            <TableCell className="px-2 py-2 text-right whitespace-normal wrap-anywhere">
                                                <MoneyValue
                                                    display="exact"
                                                    value={amount}
                                                />
                                            </TableCell>
                                            <TableCell className="px-2 py-2 text-right whitespace-normal wrap-anywhere">
                                                <MoneyValue
                                                    display="exact"
                                                    value={available}
                                                />
                                            </TableCell>
                                        </TableRow>
                                        {hasRisk && (
                                            <TableRow className="border-b border-border/60 hover:bg-transparent">
                                                <TableCell
                                                    colSpan={3}
                                                    className="pb-2"
                                                >
                                                    <div
                                                        className={cn(
                                                            "flex flex-wrap items-center gap-x-3 gap-y-1 rounded-r-md border-l-2 px-2 py-1.5 text-xs",
                                                            over
                                                                ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                                                                : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                                                        )}
                                                    >
                                                        {over && (
                                                            <span>
                                                                超出可申请余额{" "}
                                                                <MoneyValue
                                                                    display="exact"
                                                                    value={subtractFixed(
                                                                        amount,
                                                                        available,
                                                                        reviewMoney,
                                                                    )}
                                                                    inheritColor
                                                                />
                                                            </span>
                                                        )}
                                                        {prepayment && (
                                                            <span>
                                                                先款门禁 ·{" "}
                                                                {
                                                                    line.payment_term_name
                                                                }
                                                            </span>
                                                        )}
                                                        {line.source_changed && (
                                                            <span>
                                                                付款依据已变更
                                                            </span>
                                                        )}
                                                    </div>
                                                </TableCell>
                                            </TableRow>
                                        )}
                                        <TableRow
                                            id={lineId}
                                            hidden={!isOpen}
                                            className="border-b border-border/60 hover:bg-transparent"
                                        >
                                            <TableCell
                                                colSpan={3}
                                                className="px-3 py-2 text-xs whitespace-normal"
                                            >
                                                <p className="text-muted-foreground">
                                                    {line.payment_term_name}
                                                </p>
                                                <ul className="divide-y divide-border/60">
                                                    {line.purchase_items.map(
                                                        (item) => (
                                                            <li
                                                                key={item.id}
                                                                className="grid grid-cols-[minmax(0,1fr)_auto] gap-2 py-1.5"
                                                            >
                                                                <span className="wrap-anywhere">
                                                                    {
                                                                        item.item_name
                                                                    }
                                                                    {item.spec && (
                                                                        <span className="ml-2 text-muted-foreground">
                                                                            {
                                                                                item.spec
                                                                            }
                                                                        </span>
                                                                    )}
                                                                </span>
                                                                <span className="num inline-flex flex-wrap items-baseline justify-end gap-1 text-right">
                                                                    <QuantityValue
                                                                        value={
                                                                            item.quantity
                                                                        }
                                                                        unit={
                                                                            item.unit ??
                                                                            ""
                                                                        }
                                                                    />
                                                                    <span>
                                                                        ×
                                                                    </span>
                                                                    <MoneyValue
                                                                        display="exact"
                                                                        value={
                                                                            item.unit_price
                                                                        }
                                                                    />
                                                                    <span>
                                                                        ·
                                                                    </span>
                                                                    <MoneyValue
                                                                        display="exact"
                                                                        value={
                                                                            item.amount
                                                                        }
                                                                    />
                                                                </span>
                                                            </li>
                                                        ),
                                                    )}
                                                </ul>
                                            </TableCell>
                                        </TableRow>
                                    </Fragment>
                                )
                            },
                        )}
                        {visible.length === 0 && (
                            <TableRow>
                                <TableCell
                                    colSpan={3}
                                    className="px-2 py-4 text-xs text-muted-foreground"
                                >
                                    没有匹配的采购单。
                                </TableCell>
                            </TableRow>
                        )}
                    </TableBody>
                    <TableFooter className="border-0 bg-surface-sunken [&_th]:bg-surface-sunken [&_td:last-child]:rounded-r-md [&_th:first-child]:rounded-l-md">
                        <TableRow>
                            <TableHead
                                scope="row"
                                className="px-2 py-2 text-left text-xs font-medium whitespace-normal"
                            >
                                整单合计
                                {(hasActiveFilter || pageCount > 1) && (
                                    <span className="num block text-tiny font-normal text-muted-foreground">
                                        （当前显示 {visible.length}{" "}
                                        项，合计按整单）
                                    </span>
                                )}
                            </TableHead>
                            <TableCell className="px-2 py-2 text-right whitespace-normal wrap-anywhere">
                                <MoneyValue
                                    display="exact"
                                    value={
                                        basis.current
                                            ? request.unpaid_amount
                                            : request.total_amount
                                    }
                                />
                            </TableCell>
                            <TableCell className="px-2 py-2 text-right whitespace-normal wrap-anywhere">
                                <MoneyValue
                                    display="exact"
                                    value={basis.available}
                                />
                            </TableCell>
                        </TableRow>
                    </TableFooter>
                </Table>
                {showPagination && (
                    <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <div className="flex items-center gap-2 text-muted-foreground">
                            <span id={`${sourceId}-page-size-label`}>每页</span>
                            <Select
                                value={String(pageSize)}
                                onValueChange={(next) => {
                                    if (!next) return
                                    setPageSize(next === "50" ? 50 : 20)
                                    setPage(0)
                                }}
                            >
                                <SelectTrigger
                                    id={`${sourceId}-page-size`}
                                    size="sm"
                                    aria-labelledby={`${sourceId}-page-size-label`}
                                    className="w-24"
                                >
                                    <SelectValue>{pageSize} 项</SelectValue>
                                </SelectTrigger>
                                <SelectContent
                                    align="start"
                                    alignItemWithTrigger={false}
                                >
                                    <SelectItem
                                        id={`${sourceId}-page-size-20`}
                                        value="20"
                                    >
                                        20 项
                                    </SelectItem>
                                    <SelectItem
                                        id={`${sourceId}-page-size-50`}
                                        value="50"
                                    >
                                        50 项
                                    </SelectItem>
                                </SelectContent>
                            </Select>
                            <span className="num">
                                第 {resolvedPage + 1} / {pageCount} 页 · 共{" "}
                                {filtered.length} 项
                            </span>
                        </div>
                        <div className="flex items-center gap-1.5">
                            <Button
                                id={`${sourceId}-prev`}
                                type="button"
                                variant="outline"
                                size="xs"
                                disabled={resolvedPage === 0}
                                onClick={() => setPage(resolvedPage - 1)}
                            >
                                上一页
                            </Button>
                            <Button
                                id={`${sourceId}-next`}
                                type="button"
                                variant="outline"
                                size="xs"
                                disabled={resolvedPage + 1 === pageCount}
                                onClick={() => setPage(resolvedPage + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    </div>
                )}
            </section>

            <PaymentEvidenceTable
                request={request}
                idPrefix={`${idPrefix}-evidence`}
                workItemId={workItemId}
                submissionVersion={submissionVersion}
                onSelectFile={onSelectFile}
            />
            {submissionVersion == null && (
                <section className="border-t border-border/60 pt-3">
                    <Button
                        id={`${idPrefix}-payment-history-toggle`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        aria-expanded={paymentHistoryOpen}
                        aria-controls={`${idPrefix}-payment-history`}
                        onClick={() =>
                            setPaymentHistoryOpen(!paymentHistoryOpen)
                        }
                    >
                        <ChevronRightIcon
                            aria-hidden="true"
                            className={cn(paymentHistoryOpen && "rotate-90")}
                        />
                        付款核销与其他申请占用
                    </Button>
                    <div
                        id={`${idPrefix}-payment-history`}
                        hidden={!paymentHistoryOpen}
                    >
                        {paymentHistoryOpen && (
                            <RequestReviewHistory
                                request={request}
                                workItemId={workItemId}
                            />
                        )}
                    </div>
                </section>
            )}
            {!hideEmbeddedMaterials && (
                <RequestMaterials
                    request={request}
                    workItemId={workItemId}
                    submissionVersion={submissionVersion}
                    idPrefix={`${idPrefix}-materials`}
                    concise
                    onSelectFile={onSelectFile}
                    activeFileId={activeFileId}
                />
            )}
            {approval && !hideEmbeddedMaterials && (
                <section className="border-t border-border/60 pt-3">
                    <Button
                        id={`${idPrefix}-history-toggle`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        aria-expanded={historyOpen}
                        aria-controls={`${idPrefix}-history`}
                        onClick={() => setHistoryOpen(!historyOpen)}
                    >
                        <ChevronRightIcon
                            aria-hidden="true"
                            className={cn(historyOpen && "rotate-90")}
                        />
                        审批与历次提交
                    </Button>
                    <div
                        id={`${idPrefix}-history`}
                        hidden={!historyOpen}
                        className="space-y-3"
                    >
                        {historyOpen && (
                            <>
                                {approval}
                                <RequestSubmissionHistory request={request} />
                            </>
                        )}
                    </div>
                </section>
            )}
        </div>
    )
}

/** 发票材料沿用申请附件授权，实际付款回单沿用付款记录授权。 */
function PaymentEvidenceTable({
    request,
    idPrefix,
    workItemId,
    submissionVersion,
    onSelectFile,
}: {
    request: PaymentRequest
    idPrefix: string
    workItemId?: string
    submissionVersion?: number
    onSelectFile?: (fileId: string) => void
}) {
    const {
        mutateAsync: download,
        isPending,
        error,
    } = useRequestDownloadMutation()
    const [selected, setSelected] = useState<string>()
    const [search, setSearch] = useState("")
    const [page, setPage] = useState(0)
    const [pageSize, setPageSize] = useState(reviewPageSize)
    const materials = usePaymentMaterialsQuery(
        request.id,
        workItemId,
        submissionVersion,
    )
    const sourceSales = materials.data?.sales
    const evidence = useMemo(() => {
        const items = new Map(
            [
                ...request.attachments,
                ...(sourceSales?.flatMap((source) => source.materials) ?? []),
            ]
                .filter((file) => /发票|invoice/i.test(file.file_name))
                .map((file) => [
                    file.file_asset_id,
                    { file, label: "发票附件" },
                ]),
        )
        for (const payment of request.payments) {
            if (payment.receipt) {
                items.set(payment.receipt.file_asset_id, {
                    file: payment.receipt,
                    label: `付款回单 · ${payment.payment_no}`,
                })
            }
        }
        return Array.from(items.values())
    }, [request.attachments, request.payments, sourceSales])
    const files = useMemo<readonly MaterialFileItem[]>(
        () =>
            evidence.map(({ file }) => ({
                id: file.file_asset_id,
                name: file.file_name,
                mimeType: file.content_type,
                loadPreview: () =>
                    fetchMaterialPreview(
                        request.id,
                        file,
                        workItemId,
                        submissionVersion,
                    ),
                onDownload: () =>
                    download({
                        requestId: request.id,
                        attachment: file,
                        workItemId,
                        submissionVersion,
                    }),
            })),
        [download, evidence, request.id, submissionVersion, workItemId],
    )
    if (!evidence.length) return null

    const term = search.trim().toLowerCase()
    const filtered = evidence.filter(({ file, label }) =>
        `${label} ${file.file_name}`.toLowerCase().includes(term),
    )
    const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize))
    const resolvedPage = Math.min(page, pageCount - 1)
    const visible = filtered.slice(
        resolvedPage * pageSize,
        (resolvedPage + 1) * pageSize,
    )
    const showPagination =
        filtered.length > reviewPageSize || pageSize !== reviewPageSize
    return (
        <section
            aria-label="发票附件与付款回单"
            className="min-w-0 space-y-2 border-t border-border/60 pt-3"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h2 className="text-section-title font-semibold">
                    发票附件与付款回单
                </h2>
                {(evidence.length > 8 || Boolean(search)) && (
                    <div className="relative w-full @[560px]/payment-request:w-56">
                        <SearchIcon
                            aria-hidden="true"
                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                        />
                        <Input
                            id={`${idPrefix}-search`}
                            value={search}
                            placeholder="搜索凭据或付款单号"
                            aria-label="搜索凭据"
                            className="h-control-sm pr-8 pl-8 text-xs"
                            onChange={(event) => {
                                setSearch(event.target.value)
                                setPage(0)
                            }}
                        />
                        {search ? (
                            <Button
                                id={`${idPrefix}-search-clear`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label="清除凭据搜索"
                                className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                onClick={() => {
                                    setSearch("")
                                    setPage(0)
                                }}
                            >
                                <XIcon aria-hidden="true" />
                            </Button>
                        ) : null}
                    </div>
                )}
            </div>
            <Table className="w-full table-fixed text-body-compact">
                <TableHeader className="bg-surface-sunken text-xs text-muted-foreground [&_tr]:border-b-0 [&_th]:bg-surface-sunken [&_th]:hover:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md">
                    <TableRow>
                        <TableHead
                            scope="col"
                            className="px-2 py-2 text-left font-medium"
                        >
                            凭据
                        </TableHead>
                        <TableHead
                            scope="col"
                            className="w-28 px-2 py-2 text-right font-medium"
                        >
                            操作
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {visible.map(({ file, label }) => {
                        const fileId = `${idPrefix}-${toAutomationIdSegment(file.file_asset_id)}`
                        const previewable = isMaterialPreviewable({
                            name: file.file_name,
                            mimeType: file.content_type,
                        })
                        return (
                            <TableRow
                                key={file.file_asset_id}
                                className="border-b border-border/60"
                            >
                                <TableHead
                                    scope="row"
                                    className="bg-transparent px-2 py-2 text-left font-normal whitespace-normal"
                                >
                                    <span className="block wrap-anywhere">
                                        {file.file_name}
                                    </span>
                                    <span className="num text-tiny text-muted-foreground">
                                        {label}
                                    </span>
                                </TableHead>
                                <TableCell className="px-2 py-2 text-right whitespace-normal wrap-anywhere">
                                    {previewable && (
                                        <Button
                                            id={`${fileId}-preview`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            onClick={() =>
                                                (onSelectFile ?? setSelected)(
                                                    file.file_asset_id,
                                                )
                                            }
                                        >
                                            预览
                                        </Button>
                                    )}
                                    <Button
                                        id={`${fileId}-download`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={isPending}
                                        onClick={() =>
                                            void download({
                                                requestId: request.id,
                                                attachment: file,
                                                workItemId,
                                                submissionVersion,
                                            }).catch(() => undefined)
                                        }
                                    >
                                        下载
                                    </Button>
                                </TableCell>
                            </TableRow>
                        )
                    })}
                    {!visible.length && (
                        <TableRow>
                            <TableCell
                                colSpan={2}
                                className="px-2 py-3 text-xs text-muted-foreground"
                            >
                                没有匹配的凭据。
                            </TableCell>
                        </TableRow>
                    )}
                </TableBody>
                <TableFooter className="border-0 bg-surface-sunken text-xs [&_td:first-child]:rounded-l-md [&_td:first-child]:rounded-r-md">
                    <TableRow>
                        <TableCell colSpan={2} className="num px-2 py-2">
                            共 {evidence.length} 份凭据
                            {(term || pageCount > 1) &&
                                `（当前显示 ${visible.length} 份）`}
                        </TableCell>
                    </TableRow>
                </TableFooter>
            </Table>
            {showPagination && (
                <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                    <div className="flex items-center gap-2 text-muted-foreground">
                        <span id={`${idPrefix}-page-size-label`}>每页</span>
                        <Select
                            value={String(pageSize)}
                            onValueChange={(next) => {
                                if (!next) return
                                setPageSize(next === "50" ? 50 : 20)
                                setPage(0)
                            }}
                        >
                            <SelectTrigger
                                id={`${idPrefix}-page-size`}
                                size="sm"
                                aria-labelledby={`${idPrefix}-page-size-label`}
                                className="w-24"
                            >
                                <SelectValue>{pageSize} 项</SelectValue>
                            </SelectTrigger>
                            <SelectContent
                                align="start"
                                alignItemWithTrigger={false}
                            >
                                <SelectItem
                                    id={`${idPrefix}-page-size-20`}
                                    value="20"
                                >
                                    20 项
                                </SelectItem>
                                <SelectItem
                                    id={`${idPrefix}-page-size-50`}
                                    value="50"
                                >
                                    50 项
                                </SelectItem>
                            </SelectContent>
                        </Select>
                        <span className="num">
                            第 {resolvedPage + 1} / {pageCount} 页 · 共{" "}
                            {filtered.length} 份
                        </span>
                    </div>
                    <div className="flex items-center gap-1.5">
                        <Button
                            id={`${idPrefix}-prev`}
                            type="button"
                            variant="outline"
                            size="xs"
                            disabled={resolvedPage === 0}
                            onClick={() => setPage(resolvedPage - 1)}
                        >
                            上一页
                        </Button>
                        <Button
                            id={`${idPrefix}-next`}
                            type="button"
                            variant="outline"
                            size="xs"
                            disabled={resolvedPage + 1 === pageCount}
                            onClick={() => setPage(resolvedPage + 1)}
                        >
                            下一页
                        </Button>
                    </div>
                </div>
            )}
            {error && (
                <p role="alert" className="text-xs text-destructive">
                    {getErrorMessage(error, "凭据下载失败")}
                </p>
            )}
            {!onSelectFile && (
                <UnifiedMaterialViewer
                    idPrefix={`${idPrefix}-viewer`}
                    mode="dialog"
                    files={files}
                    activeFileId={selected}
                    open={Boolean(selected)}
                    onOpenChange={(open) => {
                        if (!open) setSelected(undefined)
                    }}
                    onActiveFileChange={setSelected}
                />
            )}
        </section>
    )
}
