"use client"

import * as React from "react"
import {
    ChevronDownIcon,
    MessageSquareTextIcon,
    RefreshCwIcon,
    ShieldAlertIcon,
} from "lucide-react"

import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api"
import { isApiError } from "@/lib/api/errors"
import { displayName } from "@/lib/display-name"
import { isCommandOutcomeUnknown } from "@/lib/formal-command"
import { cn } from "@/lib/utils"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    formatByteCounterText,
    utf8ByteLength,
} from "@/lib/validation/reason-length"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"

import { ReasonTextareaField } from "../reason-textarea-field"
import { FormalActionConfirmDialog } from "../workflow-actions"
import { useWorkspaceTaskNavigation } from "../workspace-task-pane"

export type ApprovalActionDecision = "APPROVE" | "REJECT"

export type ApprovalActionRecoveryOption =
    | "RESUME_CURRENT_APPROVER"
    | "CANCEL_BLOCKED"

export const APPROVAL_RECOVERY_ACTION_LABEL: Readonly<
    Record<ApprovalActionRecoveryOption, string>
> = {
    RESUME_CURRENT_APPROVER: "继续由原审批人审批",
    CANCEL_BLOCKED: "终止并退回此审批",
}

export type ApprovalDecisionContext = Readonly<{
    documentLabel?: string
    amountLabel?: string
    currentNodeLabel?: string
    impactSummary?: string
}>

export type ApprovalDecisionPayload = Readonly<{
    workItemId: string
    decision: ApprovalActionDecision
    reason?: string
    expectedTaskVersion: string
    idempotencyKey: string
}>

export type ApprovalActionBarProps<TResult = unknown> = Readonly<{
    allowedActions: readonly string[]
    recoveryOptions?: readonly ApprovalActionRecoveryOption[]
    workItemId?: string
    expectedTaskVersion?: string
    documentType?: string
    documentHref?: string | null
    /** 撤回按钮是否处于驳回后修改原单态。 */
    reviseRejected?: boolean
    emergencyWithdraw?: boolean
    withdrawDisabled?: boolean
    withdrawDisabledTitle?: string
    canReadSensitive?: boolean
    /** 由对象中心页头承接的动作，审批区内不再重复渲染。 */
    hiddenActions?: readonly string[]
    /** 工作台提供的审批事实；高危核对弹窗只展示，不参与命令构造。 */
    decisionContext?: ApprovalDecisionContext
    /** ADR-1 高危单据标记：冲正单自动启用；负毛利、超信用/预算、大额或阻断告警由调用方传入。 */
    highRisk?: boolean
    /** 高危核对原因列表（非空时自动触发 ADR-1 单层高危确认弹窗）。 */
    highRiskReasons?: readonly string[]
    /** 自定义高危核对摘要区块，展示在单层 FormalActionConfirmDialog 顶部。 */
    highRiskSummary?: React.ReactNode
    /** 结合当前单据检测出的具体异常，前置展示的驳回快捷短语。 */
    dynamicRejectPhrases?: readonly string[]
    /** 审批提交回调（通过/驳回均经由此回调执行，支持锁定原幂等键重试）。 */
    onSubmitDecision?: (payload: ApprovalDecisionPayload) => Promise<TResult>
    /** 决定成功应用并重置内联状态后的回调。 */
    onDecisionApplied?: (result: TResult) => void
    /** 外部受控提交态（与内部异步提交态合并）。 */
    submitting?: boolean
    /** 可选：覆盖冲突错误识别，默认识别 HTTP 409。 */
    isConflictError?: (error: unknown) => boolean
    /** 可选：覆盖冲突错误文案提取。 */
    getConflictMessage?: (error: unknown) => string
    /** 刷新当前任务回调；未传时回退至工作台任务导航上下文。 */
    onRefreshTask?: () => void
    /** 业务域恢复/撤回/升级动作回调（传入且权限允许时渲染对应按钮）。 */
    onResumeApprover?: () => void
    onCancelBlocked?: () => void
    onWithdraw?: () => void
    onUpgradeBinding?: () => void
    /** 外部业务域扩展次要动作。 */
    extraSecondaryActions?: React.ReactNode
    /** 外部业务域弹窗节点（如恢复审批人、撤回审批、升级绑定弹窗）。 */
    dialogs?: React.ReactNode
    /** 存在外部业务弹窗打开时暂停审批单键热键（A / R）。 */
    externalDialogOpen?: boolean
    /** 展示承载形态：`"sheet"` 用于 768px 轻作业抽屉底栏，约束内联输入框与展开意见区宽度避免溢出。 */
    presentation?: "default" | "workspace" | "sheet"
    id?: string
}>

type IdempotencySlot = Readonly<{
    key: string
    fingerprint: string
}>

type UnknownDecisionOutcome = Readonly<{
    decision: ApprovalActionDecision
    reason: string
    expectedTaskVersion: string
    message: string
}>

const REVERSAL_DOCUMENT_TYPES = new Set([
    "receipt_reversal",
    "payment_reversal",
])

function decisionIntentFingerprint(
    decision: ApprovalActionDecision,
    reason: string,
): string {
    return `${decision}:${reason.trim()}`
}

function slotForDecisionIntent(
    current: IdempotencySlot | null,
    workItemId: string,
    fingerprint: string,
): IdempotencySlot {
    if (current && current.fingerprint === fingerprint) return current
    return {
        key: `approval:decision:${workItemId}:${crypto.randomUUID()}`,
        fingerprint,
    }
}

function defaultIsConflictError(error: unknown): boolean {
    return isApiError(error) && error.status === 409
}

function defaultGetConflictMessage(error: unknown): string {
    return getErrorMessage(error, "提交失败，请刷新后重试。")
}

/** 按单据类型动态匹配常用驳回短语，避免销售/财务/采购短语错位。 */
export function quickRejectPhrasesForDocumentType(
    documentType?: string,
): readonly string[] {
    switch (documentType) {
        case "sales_order":
        case "voucher_sales_order":
            return [
                "售价或毛利需核对",
                "缺少盖章合同或附件",
                "付款条件或账期不符",
                "交期或收货地址需调整",
            ]
        case "sales_change_order":
        case "purchase_change_order":
            return [
                "变更金额或差额需核对",
                "变更原因说明不充分",
                "交期变更影响下游履约",
                "缺少客户或供应商确认凭证",
            ]
        case "purchase_order":
            return [
                "实际采购单价高于采购供给价",
                "预计交期晚于销售承诺",
                "供应商资料或附件不全",
                "采购数量与销售需求不符",
            ]
        case "sales_invoice_request":
            return [
                "开票金额超出可用额度",
                "开票抬头或税号有误",
                "合同约定开票时点未到",
                "缺少对应回款或验收凭证",
            ]
        case "purchase_payment_request":
            return [
                "收款账户信息需核对",
                "付款金额超出可申请余额",
                "履约进度未达付款条件",
                "缺少发票或对账附件",
            ]
        case "customer_receipt":
            return [
                "回款金额与银行流水不符",
                "缺少清晰银行回单凭证",
                "核销分配方案需调整",
                "到账日期或付款主体有误",
            ]
        case "customer_refund":
        case "supplier_refund":
            return [
                "退款金额与原单依据不符",
                "退款原因或审批附件不全",
                "收款账户信息需复核",
                "需先核对关联退货或结清状态",
            ]
        case "receipt_reversal":
        case "payment_reversal":
            return [
                "冲正原因说明不充分",
                "冲正将导致关联余额异常",
                "请核对原收付款单号及金额",
                "需补充财务复核凭证",
            ]
        case "stock_adjustment":
            return [
                "盘盈或盘亏数量依据不足",
                "缺少盘点单或现场附件",
                "调整原因分类不准确",
                "成本影响需进一步复核",
            ]
        default:
            return [
                "金额或单价有误",
                "缺少合同或凭证附件",
                "往来或开票信息需核对",
                "原因说明需补充完整",
            ]
    }
}

/**
 * 按服务端 `allowed_actions` 与 `recovery_options` 渲染审批决策条（ADR-1 分级风险决策制）：
 * - 常规低/普通风险单据：底栏内联意见 + 一键直接通过（零二级弹窗，3 秒 toast + autoNext）；
 * - 高危单据（冲正红字 / 负毛利 / 超信用预算 / 大额 / 阻断告警）：点击通过弹出单层 `FormalActionConfirmDialog` 核对；
 * - 驳回：未填原因时展开并聚焦底栏意见框与按单据类型匹配的快捷短语，严格校验必填与 BPM 512 UTF-8 字节上限，支持 Ctrl/⌘+Enter 提交；
 * - 确定失败展示内联错误提示，网络/5xx/OUTCOME_UNKNOWN 锁定原意图与幂等键并提供「重试/核对本次决定」与「刷新当前任务」。
 */
export function ApprovalActionBar<TResult = unknown>({
    allowedActions,
    recoveryOptions = [],
    workItemId,
    expectedTaskVersion,
    documentType,
    documentHref,
    reviseRejected = false,
    emergencyWithdraw = false,
    withdrawDisabled = false,
    withdrawDisabledTitle,
    canReadSensitive = true,
    hiddenActions = [],
    decisionContext,
    highRisk = false,
    highRiskReasons,
    highRiskSummary,
    dynamicRejectPhrases,
    onSubmitDecision,
    onDecisionApplied,
    submitting = false,
    isConflictError = defaultIsConflictError,
    getConflictMessage = defaultGetConflictMessage,
    onRefreshTask,
    onResumeApprover,
    onCancelBlocked,
    onWithdraw,
    onUpgradeBinding,
    extraSecondaryActions,
    dialogs,
    externalDialogOpen = false,
    presentation = "default",
    id = "governance-approval-action-bar",
}: ApprovalActionBarProps<TResult>) {
    const taskNavigation = useWorkspaceTaskNavigation()
    const resolvedRefresh = onRefreshTask ?? taskNavigation?.onRefresh
    const [highRiskDialogOpen, setHighRiskDialogOpen] = React.useState(false)

    const [comment, setComment] = React.useState("")
    const [composerOpen, setComposerOpen] = React.useState(false)
    const [composerIntent, setComposerIntent] =
        React.useState<ApprovalActionDecision>("APPROVE")
    const [commentError, setCommentError] = React.useState<string | null>(null)
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const [unknownOutcome, setUnknownOutcome] =
        React.useState<UnknownDecisionOutcome | null>(null)
    const [submittingDecision, setSubmittingDecision] =
        React.useState<ApprovalActionDecision | null>(null)
    const slotRef = React.useRef<IdempotencySlot | null>(null)
    const textareaRef = React.useRef<HTMLTextAreaElement>(null)

    const isSubmitting = Boolean(submitting || submittingDecision !== null)
    const anyDialogOpen = highRiskDialogOpen || externalDialogOpen

    React.useEffect(() => {
        setComment("")
        setComposerOpen(false)
        setComposerIntent("APPROVE")
        setHighRiskDialogOpen(false)
        setCommentError(null)
        setConflictMessage(null)
        setSubmitError(null)
        setUnknownOutcome(null)
        setSubmittingDecision(null)
        slotRef.current = null
    }, [workItemId])

    React.useEffect(() => {
        if (!composerOpen || unknownOutcome !== null) return
        const raf = window.requestAnimationFrame(() => {
            textareaRef.current?.focus()
        })
        return () => window.cancelAnimationFrame(raf)
    }, [composerOpen, composerIntent, unknownOutcome])

    const hidden = new Set(hiddenActions)
    const actions = new Set(
        allowedActions.filter((action) => !hidden.has(action)),
    )
    const recoveries = new Set(recoveryOptions)
    const showApprove =
        actions.has("APPROVE") && Boolean(workItemId && expectedTaskVersion)
    const showReject =
        actions.has("REJECT") && Boolean(workItemId && expectedTaskVersion)
    const showResume =
        recoveries.has("RESUME_CURRENT_APPROVER") &&
        actions.has("RESUME_CURRENT_APPROVER") &&
        Boolean(onResumeApprover)
    const showCancelBlocked =
        recoveries.has("CANCEL_BLOCKED") &&
        actions.has("CANCEL_BLOCKED_APPROVAL") &&
        Boolean(onCancelBlocked)
    const showWithdraw =
        (actions.has("CANCEL") || actions.has("CANCEL_APPROVAL")) &&
        Boolean(onWithdraw)
    const showUpgrade =
        actions.has("UPGRADE_BINDING") && Boolean(onUpgradeBinding)
    const showOpenDocument =
        (actions.has("OPEN_DOCUMENT") || actions.has("VIEW")) &&
        Boolean(documentHref)
    const hasSecondaryActions =
        showOpenDocument ||
        showResume ||
        showCancelBlocked ||
        showWithdraw ||
        showUpgrade ||
        Boolean(extraSecondaryActions)
    const hasDecisionActions = showReject || showApprove
    const isSheet = presentation === "sheet"

    const isReversal = Boolean(
        documentType && REVERSAL_DOCUMENT_TYPES.has(documentType),
    )
    const resolvedHighRiskReasons = React.useMemo(() => {
        const list = [...(highRiskReasons ?? [])].filter((item) => item.trim())
        if (isReversal && list.length === 0) {
            list.push(
                "冲正红字单据：通过后将冲回原收付款记录并同步调整往来余额。",
            )
        }
        return list
    }, [highRiskReasons, isReversal])
    const effectiveHighRisk = Boolean(
        highRisk ||
        isReversal ||
        highRiskSummary ||
        resolvedHighRiskReasons.length > 0,
    )

    const isUnknownOutcome = unknownOutcome !== null
    const isUnknownApprove = unknownOutcome?.decision === "APPROVE"
    const isUnknownReject = unknownOutcome?.decision === "REJECT"

    React.useEffect(() => {
        if (!unknownOutcome) return
        const decisionStillAllowed =
            unknownOutcome.decision === "APPROVE" ? showApprove : showReject
        if (
            !decisionStillAllowed ||
            expectedTaskVersion !== unknownOutcome.expectedTaskVersion
        ) {
            setUnknownOutcome(null)
            slotRef.current = null
        }
    }, [expectedTaskVersion, showApprove, showReject, unknownOutcome])

    const trimmedComment = comment.trim()
    const commentBytes = utf8ByteLength(trimmedComment)
    const overByteLimit = commentBytes > BPM_REASON_MAX_UTF8_BYTES
    const quickRejectPhrases = React.useMemo(() => {
        const base = quickRejectPhrasesForDocumentType(documentType)
        if (!dynamicRejectPhrases?.length) return base
        const seen = new Set<string>()
        const merged: string[] = []
        for (const phrase of [...dynamicRejectPhrases, ...base]) {
            const trimmed = phrase.trim()
            if (!trimmed || seen.has(trimmed)) continue
            seen.add(trimmed)
            merged.push(trimmed)
            if (merged.length >= 6) break
        }
        return merged
    }, [documentType, dynamicRejectPhrases])

    const validateComment = React.useCallback(
        (decision: ApprovalActionDecision): string | null => {
            if (decision === "REJECT" && trimmedComment.length === 0) {
                return "驳回时必须填写原因"
            }
            if (commentBytes > BPM_REASON_MAX_UTF8_BYTES) {
                const label = decision === "REJECT" ? "驳回原因" : "审批意见"
                return `${label}超出长度限制（当前 ${commentBytes} 字节，上限 ${BPM_REASON_MAX_UTF8_BYTES} 字节，约 170 个汉字）`
            }
            return null
        },
        [commentBytes, trimmedComment],
    )

    const executeDecision = React.useCallback(
        async (
            decision: ApprovalActionDecision,
            options?: { fromDialog?: boolean; retryLocked?: boolean },
        ) => {
            if (
                !workItemId ||
                !expectedTaskVersion ||
                !onSubmitDecision ||
                isSubmitting
            ) {
                return
            }

            let resolvedReason: string
            let activeSlot: IdempotencySlot

            if (unknownOutcome) {
                if (unknownOutcome.decision !== decision) return
                resolvedReason = unknownOutcome.reason
                const lockedFingerprint = decisionIntentFingerprint(
                    decision,
                    resolvedReason,
                )
                activeSlot =
                    slotRef.current ??
                    slotForDecisionIntent(null, workItemId, lockedFingerprint)
                slotRef.current = activeSlot
            } else {
                const validationError = validateComment(decision)
                if (validationError) {
                    setComposerIntent(decision)
                    setComposerOpen(true)
                    setCommentError(validationError)
                    return
                }
                resolvedReason = trimmedComment
                const fingerprint = decisionIntentFingerprint(
                    decision,
                    resolvedReason,
                )
                activeSlot = slotForDecisionIntent(
                    slotRef.current,
                    workItemId,
                    fingerprint,
                )
                slotRef.current = activeSlot
            }

            setCommentError(null)
            setConflictMessage(null)
            setSubmitError(null)
            setSubmittingDecision(decision)

            try {
                const result = await onSubmitDecision({
                    workItemId,
                    decision,
                    reason: resolvedReason,
                    expectedTaskVersion,
                    idempotencyKey: activeSlot.key,
                })
                const isReject = decision === "REJECT"
                const docLabel = decisionContext?.documentLabel?.trim()
                toast.add({
                    title: isReject ? "审批已驳回" : "审批已通过",
                    description: docLabel
                        ? isReject
                            ? `${docLabel} 已退回原提交人处理。`
                            : `${docLabel} 已提交通过决定。`
                        : isReject
                          ? "已退回原提交人处理。"
                          : "已提交通过决定。",
                    type: isReject ? "info" : "success",
                    timeout: 3000,
                })
                setComment("")
                setComposerOpen(false)
                setHighRiskDialogOpen(false)
                setUnknownOutcome(null)
                setSubmitError(null)
                setConflictMessage(null)
                slotRef.current = null
                onDecisionApplied?.(result)
            } catch (error) {
                if (isConflictError(error)) {
                    setConflictMessage(getConflictMessage(error))
                    setUnknownOutcome(null)
                    setSubmitError(null)
                    slotRef.current = null
                    setHighRiskDialogOpen(false)
                    return
                }
                if (isCommandOutcomeUnknown(error)) {
                    const fallback =
                        decision === "REJECT"
                            ? "驳回结果暂无法确认，已锁定原原因与提交凭证；请点击「重试/核对本次决定」或刷新当前任务。"
                            : "通过审批结果暂无法确认，已锁定原意见与提交凭证；请点击「重试/核对本次决定」或刷新当前任务。"
                    setUnknownOutcome({
                        decision,
                        reason: resolvedReason,
                        expectedTaskVersion,
                        message: getErrorMessage(error, fallback),
                    })
                    setSubmitError(null)
                    if (options?.fromDialog) {
                        throw error
                    }
                    return
                }
                slotRef.current = null
                setUnknownOutcome(null)
                const fallback =
                    decision === "REJECT"
                        ? "驳回提交失败，请核对后重试。"
                        : "审批通过提交失败，请核对后重试。"
                setSubmitError(getErrorMessage(error, fallback))
                if (options?.fromDialog) {
                    throw error
                }
            } finally {
                setSubmittingDecision(null)
            }
        },
        [
            decisionContext?.documentLabel,
            expectedTaskVersion,
            getConflictMessage,
            isConflictError,
            isSubmitting,
            onDecisionApplied,
            onSubmitDecision,
            trimmedComment,
            unknownOutcome,
            validateComment,
            workItemId,
        ],
    )

    const handleApproveClick = React.useCallback(() => {
        if (isUnknownReject) return
        if (isUnknownApprove) {
            void executeDecision("APPROVE", { retryLocked: true })
            return
        }
        const validationError = validateComment("APPROVE")
        if (validationError) {
            setComposerIntent("APPROVE")
            setComposerOpen(true)
            setCommentError(validationError)
            return
        }
        if (effectiveHighRisk) {
            setSubmitError(null)
            setHighRiskDialogOpen(true)
            return
        }
        void executeDecision("APPROVE")
    }, [
        effectiveHighRisk,
        executeDecision,
        isUnknownApprove,
        isUnknownReject,
        validateComment,
    ])

    const handleRejectClick = React.useCallback(() => {
        if (isUnknownApprove) return
        if (isUnknownReject) {
            void executeDecision("REJECT", { retryLocked: true })
            return
        }
        const validationError = validateComment("REJECT")
        if (validationError) {
            setComposerIntent("REJECT")
            setComposerOpen(true)
            setCommentError(validationError)
            return
        }
        void executeDecision("REJECT")
    }, [executeDecision, isUnknownApprove, isUnknownReject, validateComment])

    React.useEffect(() => {
        if (!hasDecisionActions) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (anyDialogOpen || isSubmitting) return
            if (action === "approve") {
                if (!showApprove || isUnknownReject) return
                handleApproveClick()
                return
            }
            if (action === "reject") {
                if (!showReject || isUnknownApprove) return
                if (isUnknownReject) {
                    void executeDecision("REJECT", { retryLocked: true })
                    return
                }
                setComposerIntent("REJECT")
                setComposerOpen(true)
                setCommentError(null)
                window.requestAnimationFrame(() => {
                    textareaRef.current?.focus()
                })
            }
        })
    }, [
        anyDialogOpen,
        executeDecision,
        handleApproveClick,
        hasDecisionActions,
        isSubmitting,
        isUnknownApprove,
        isUnknownReject,
        showApprove,
        showReject,
    ])

    const handleCommentChange = (next: string) => {
        if (isUnknownOutcome) return
        setComment(next)
        if (submitError) setSubmitError(null)
        if (commentError) {
            const bytes = utf8ByteLength(next.trim())
            if (bytes <= BPM_REASON_MAX_UTF8_BYTES) {
                if (composerIntent !== "REJECT" || next.trim().length > 0) {
                    setCommentError(null)
                }
            }
        }
    }

    const amountLabel = decisionContext?.amountLabel?.trim()
    const nodeLabel = displayName(decisionContext?.currentNodeLabel)
    const hasAlerts = Boolean(conflictMessage || unknownOutcome || submitError)
    const expandSheetRow = composerOpen || hasAlerts

    if (
        !hasSecondaryActions &&
        !hasDecisionActions &&
        !hasAlerts &&
        canReadSensitive
    ) {
        return null
    }

    return (
        <div
            data-slot="approval-action-bar"
            className={cn(
                "flex min-w-0 max-w-full flex-col gap-2",
                isSheet
                    ? expandSheetRow
                        ? "w-[min(42rem,calc(92vw-3rem))]"
                        : "w-auto"
                    : "w-full",
            )}
        >
            {conflictMessage ? (
                <Alert variant="destructive" className="py-2">
                    <AlertDescription className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <span>{conflictMessage}</span>
                        {resolvedRefresh ? (
                            <Button
                                id={`${id}-conflict-refresh`}
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() => {
                                    setConflictMessage(null)
                                    resolvedRefresh()
                                }}
                            >
                                <RefreshCwIcon aria-hidden="true" />
                                刷新当前任务
                            </Button>
                        ) : null}
                    </AlertDescription>
                </Alert>
            ) : null}

            {unknownOutcome ? (
                <Alert variant="warning" className="py-2">
                    <AlertDescription className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <span className="min-w-0 flex-1 wrap-anywhere">
                            {unknownOutcome.message}
                        </span>
                        <div className="flex flex-wrap items-center gap-1.5">
                            <LoadingButton
                                id={`${id}-unknown-retry`}
                                type="button"
                                variant="outline"
                                size="xs"
                                loading={isSubmitting}
                                disabled={isSubmitting}
                                onClick={() =>
                                    void executeDecision(
                                        unknownOutcome.decision,
                                        { retryLocked: true },
                                    )
                                }
                            >
                                重试/核对本次决定
                            </LoadingButton>
                            {resolvedRefresh ? (
                                <Button
                                    id={`${id}-unknown-refresh`}
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    disabled={isSubmitting}
                                    onClick={() => resolvedRefresh()}
                                >
                                    <RefreshCwIcon aria-hidden="true" />
                                    刷新当前任务
                                </Button>
                            ) : null}
                        </div>
                    </AlertDescription>
                </Alert>
            ) : null}

            {submitError && !conflictMessage && !unknownOutcome ? (
                <Alert variant="destructive" className="py-2">
                    <AlertDescription className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <span className="min-w-0 flex-1 wrap-anywhere">
                            {submitError}
                        </span>
                        {resolvedRefresh ? (
                            <Button
                                id={`${id}-error-refresh`}
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() => {
                                    setSubmitError(null)
                                    resolvedRefresh()
                                }}
                            >
                                <RefreshCwIcon aria-hidden="true" />
                                刷新当前任务
                            </Button>
                        ) : null}
                    </AlertDescription>
                </Alert>
            ) : null}

            {hasDecisionActions && composerOpen ? (
                <div
                    data-slot="approval-inline-composer"
                    className={cn(
                        "rounded-lg border p-3 transition-colors",
                        composerIntent === "REJECT"
                            ? "border-destructive-border bg-destructive-soft/30"
                            : "border-border/80 bg-muted/25",
                    )}
                >
                    <div className="mb-2 flex items-center justify-between gap-2">
                        <span className="flex items-center gap-1.5 text-xs font-medium text-foreground">
                            <MessageSquareTextIcon
                                aria-hidden="true"
                                className="size-3.5 text-muted-foreground"
                            />
                            {isUnknownOutcome
                                ? "已锁定本次审批意见（待核对结果）"
                                : composerIntent === "REJECT"
                                  ? "填写驳回原因（必填）"
                                  : "审批意见（可选，驳回时必填）"}
                        </span>
                        <Button
                            id={`${id}-composer-collapse`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            disabled={isSubmitting}
                            className="h-control-xs px-1.5 text-xs text-muted-foreground hover:text-foreground"
                            onClick={() => {
                                setComposerOpen(false)
                                setCommentError(null)
                            }}
                        >
                            收起
                            <ChevronDownIcon
                                data-icon="inline-end"
                                aria-hidden="true"
                            />
                        </Button>
                    </div>
                    <ReasonTextareaField
                        id={`${id}-inline-reason-textarea`}
                        name="approvalReason"
                        mode="utf8-bytes"
                        max={BPM_REASON_MAX_UTF8_BYTES}
                        textareaRef={textareaRef}
                        rows={2}
                        required={composerIntent === "REJECT"}
                        aria-label={
                            composerIntent === "REJECT"
                                ? "驳回原因"
                                : "审批意见"
                        }
                        placeholder={
                            composerIntent === "REJECT"
                                ? "请说明驳回原因及需修改的内容（支持 Ctrl/⌘+Enter 直接驳回）"
                                : "可选填写审批意见，驳回时必填…"
                        }
                        value={comment}
                        onChange={handleCommentChange}
                        error={commentError ?? undefined}
                        invalid={Boolean(commentError || overByteLimit)}
                        disabled={isSubmitting || isUnknownOutcome}
                        quickPhrases={
                            !isUnknownOutcome &&
                            (composerIntent === "REJECT" || showReject)
                                ? quickRejectPhrases
                                : undefined
                        }
                        onCtrlEnterSubmit={() => {
                            if (isUnknownOutcome) return
                            if (composerIntent === "REJECT" && showReject) {
                                handleRejectClick()
                            } else if (showApprove) {
                                handleApproveClick()
                            }
                        }}
                    />
                </div>
            ) : null}

            <div
                className={cn(
                    "flex min-w-0 flex-wrap items-center gap-2",
                    isSheet ? "justify-end" : "w-full",
                )}
            >
                {hasSecondaryActions ? (
                    <div className="flex flex-wrap items-center gap-2">
                        {showOpenDocument ? (
                            <Button
                                id={`${id}-open-document`}
                                type="button"
                                variant="outline"
                                size="sm"
                                render={
                                    <a
                                        id={`${id}-open-document-link`}
                                        href={documentHref ?? undefined}
                                        aria-label="打开单据"
                                    />
                                }
                            >
                                打开单据
                            </Button>
                        ) : null}
                        {showResume ? (
                            <Button
                                id={`${id}-resume`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={isUnknownOutcome}
                                onClick={onResumeApprover}
                            >
                                {
                                    APPROVAL_RECOVERY_ACTION_LABEL.RESUME_CURRENT_APPROVER
                                }
                            </Button>
                        ) : null}
                        {showCancelBlocked ? (
                            <Button
                                id={`${id}-cancel-blocked`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={isUnknownOutcome}
                                onClick={onCancelBlocked}
                            >
                                {APPROVAL_RECOVERY_ACTION_LABEL.CANCEL_BLOCKED}
                            </Button>
                        ) : null}
                        {showWithdraw ? (
                            <Button
                                id={`${id}-withdraw`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={withdrawDisabled || isUnknownOutcome}
                                title={withdrawDisabledTitle}
                                onClick={onWithdraw}
                            >
                                {reviseRejected
                                    ? "修改原单"
                                    : emergencyWithdraw
                                      ? "代申请人撤回"
                                      : "撤回审批"}
                            </Button>
                        ) : null}
                        {showUpgrade ? (
                            <Button
                                id={`${id}-upgrade`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={isUnknownOutcome}
                                onClick={onUpgradeBinding}
                            >
                                采用最新审批流程
                            </Button>
                        ) : null}
                        {extraSecondaryActions}
                    </div>
                ) : null}

                {hasDecisionActions ? (
                    <div
                        className={cn(
                            "flex min-w-0 flex-wrap items-center justify-end gap-2",
                            !isSheet && "flex-1",
                            hasSecondaryActions && !isSheet && "ml-auto",
                        )}
                    >
                        {!composerOpen ? (
                            <div
                                className={cn(
                                    "relative flex max-w-full items-center",
                                    isSheet
                                        ? "w-44 sm:w-52"
                                        : "min-w-44 max-w-md flex-1",
                                )}
                            >
                                <Input
                                    id={`${id}-inline-reason`}
                                    value={comment}
                                    disabled={isSubmitting || isUnknownOutcome}
                                    aria-label="审批意见"
                                    aria-invalid={overByteLimit || undefined}
                                    placeholder={
                                        isUnknownOutcome
                                            ? "已锁定本次审批意见"
                                            : "可选填写审批意见，驳回时必填…"
                                    }
                                    className="pr-20"
                                    onFocus={() => {
                                        if (isUnknownOutcome) return
                                        setComposerIntent("APPROVE")
                                        setComposerOpen(true)
                                    }}
                                    onChange={(event) => {
                                        handleCommentChange(event.target.value)
                                        setComposerOpen(true)
                                    }}
                                />
                                {commentBytes > 0 ? (
                                    <span
                                        className={cn(
                                            "num pointer-events-none absolute right-2.5 text-tiny",
                                            overByteLimit
                                                ? "font-medium text-destructive"
                                                : "text-muted-foreground",
                                        )}
                                        title={formatByteCounterText(comment)}
                                    >
                                        {commentBytes}/
                                        {BPM_REASON_MAX_UTF8_BYTES}B
                                    </span>
                                ) : null}
                            </div>
                        ) : null}

                        <div className="flex shrink-0 items-center gap-2">
                            {showReject ? (
                                <LoadingButton
                                    id={`${id}-reject`}
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    loading={
                                        isSubmitting &&
                                        submittingDecision === "REJECT"
                                    }
                                    disabled={isSubmitting || isUnknownApprove}
                                    className="border-destructive-border text-destructive-soft-foreground hover:border-destructive/50 hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                    onClick={handleRejectClick}
                                >
                                    {isUnknownReject
                                        ? "重试/核对本次决定"
                                        : composerOpen &&
                                            composerIntent === "REJECT" &&
                                            trimmedComment.length > 0
                                          ? "确认驳回"
                                          : "驳回"}
                                </LoadingButton>
                            ) : null}
                            {showApprove ? (
                                <LoadingButton
                                    id={`${id}-approve`}
                                    type="button"
                                    variant="default"
                                    size="sm"
                                    loading={
                                        isSubmitting &&
                                        submittingDecision === "APPROVE"
                                    }
                                    disabled={isSubmitting || isUnknownReject}
                                    onClick={handleApproveClick}
                                >
                                    {effectiveHighRisk ? (
                                        <ShieldAlertIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                    ) : null}
                                    {isUnknownApprove
                                        ? "重试/核对本次决定"
                                        : "通过审批"}
                                </LoadingButton>
                            ) : null}
                        </div>
                    </div>
                ) : null}
            </div>

            {!canReadSensitive ? (
                <p className="w-full text-xs text-muted-foreground">
                    当前账号无权查看部分业务字段
                </p>
            ) : null}

            {showApprove && effectiveHighRisk ? (
                <FormalActionConfirmDialog
                    idPrefix={`${id}-high-risk-confirm`}
                    layout="compact"
                    open={highRiskDialogOpen}
                    onOpenChange={(open) => {
                        if (!open && !isSubmitting) {
                            setHighRiskDialogOpen(false)
                            setSubmitError(null)
                        }
                    }}
                    title="高危单据审批通过核对"
                    description="该单据命中高风险复核条件，请再次确认风险提示与关键金额后再提交通过。"
                    actionLabel="通过审批"
                    confirmLabel={
                        isUnknownApprove ? "重试/核对本次决定" : "确认通过审批"
                    }
                    cancelLabel={isUnknownApprove ? "暂时关闭" : "返回核对"}
                    fromStatus={{ label: "待审批", tone: "info" }}
                    toStatus={{ label: "已通过", tone: "success" }}
                    pending={isSubmitting}
                    summary={[
                        decisionContext?.documentLabel?.trim() ? (
                            <span key="doc">
                                单据：{decisionContext.documentLabel.trim()}
                            </span>
                        ) : null,
                        amountLabel ? (
                            <span key="amount" className="num">
                                金额：{amountLabel}
                            </span>
                        ) : null,
                        nodeLabel ? (
                            <span key="node">当前节点：{nodeLabel}</span>
                        ) : null,
                        trimmedComment ? (
                            <span key="comment">
                                审批意见：{trimmedComment}
                            </span>
                        ) : null,
                    ].filter(Boolean)}
                    irreversibleEffects={resolvedHighRiskReasons}
                    effects={
                        decisionContext?.impactSummary?.trim()
                            ? [decisionContext.impactSummary.trim()]
                            : undefined
                    }
                    formContent={
                        highRiskSummary || isUnknownApprove || submitError ? (
                            <div className="space-y-2.5">
                                {highRiskSummary}
                                {submitError && !isUnknownApprove ? (
                                    <Alert variant="destructive">
                                        <AlertDescription className="text-xs wrap-anywhere">
                                            {submitError}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                                {isUnknownApprove ? (
                                    <Alert variant="warning">
                                        <AlertDescription className="flex flex-wrap items-center justify-between gap-2 text-xs">
                                            <span>
                                                本次提交结果暂无法确认，原审批意见与提交凭证已锁定。请点击「重试/核对本次决定」，或点击「暂时关闭」后刷新当前任务核对。
                                            </span>
                                            {resolvedRefresh ? (
                                                <Button
                                                    id={`${id}-high-risk-unknown-refresh`}
                                                    type="button"
                                                    variant="outline"
                                                    size="xs"
                                                    disabled={isSubmitting}
                                                    onClick={() => {
                                                        setHighRiskDialogOpen(
                                                            false,
                                                        )
                                                        resolvedRefresh()
                                                    }}
                                                >
                                                    <RefreshCwIcon aria-hidden="true" />
                                                    刷新当前任务
                                                </Button>
                                            ) : null}
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                            </div>
                        ) : undefined
                    }
                    onConfirm={() =>
                        executeDecision("APPROVE", {
                            fromDialog: true,
                            retryLocked: isUnknownApprove,
                        })
                    }
                    onConfirmError={() => undefined}
                />
            ) : null}

            {dialogs}
        </div>
    )
}
