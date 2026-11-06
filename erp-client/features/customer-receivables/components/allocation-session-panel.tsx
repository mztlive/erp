"use client"

import Link from "next/link"
import { useState } from "react"
import { SaveIcon } from "lucide-react"

import {
    AllocationWorkspace,
    DiscardConfirmDialog,
    FormalActionConfirmDialog,
    FormalActionResult,
    MoneyValue,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { MoneyInput } from "@/components/form/money-input"
import { SessionFactFields } from "@/features/customer-receivables/components/session-fact-fields"
import { SessionHeader } from "@/features/customer-receivables/components/session-header"
import { SessionPool } from "@/features/customer-receivables/components/session-pool"
import { ReceiptSessionFields } from "@/features/customer-receivables/components/receipt-session-fields"
import { ReceiptAllocationTable } from "@/features/customer-receivables/components/receipt-allocation-table"
import { ReceiptRegistrationSteps } from "./receipt-registration-steps"
import { ReceiptSessionFooter } from "@/features/customer-receivables/components/receipt-session-footer"
import { CustomerReceiptApprovalArea } from "@/features/customer-receivables/components/customer-receipt-approval-area"
import { CustomerReceiptSubmitConfirmDialog } from "@/features/customer-receivables/components/customer-receipt-submit-confirm-dialog"
import { useAllocationSession } from "@/features/customer-receivables/hooks/use-allocation-session"
import { money } from "@/features/customer-receivables/lib/allocation-math"
import { customerReceiptApprovalPhase } from "@/features/customer-receivables/lib/customer-receipt-approval"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    AllocationSessionView,
    PostAllocationResult,
} from "@/features/customer-receivables/types"

/**
 * 核销工作区。回款创建后只读展示绑定，提交确认走通用审批路线。
 * 发票登记确认使用业务确认框，不接入通用审批区。
 */
export function AllocationSessionPanel({
    session,
    onClose,
    onPosted,
    canOperate = true,
    permissionReason,
    workItemId,
    expectedTaskVersion,
    taskReceivableAccountId,
    hideSessionClose = false,
    closeLabel = "返回列表",
}: {
    session: AllocationSessionView
    onClose: () => void
    onPosted: (
        result: Extract<PostAllocationResult, { status: "succeeded" }>,
    ) => void
    canOperate?: boolean
    permissionReason?: string
    workItemId?: string
    expectedTaskVersion?: string
    taskReceivableAccountId?: string
    hideSessionClose?: boolean
    closeLabel?: string
}) {
    const [attempted, setAttempted] = useState(false)
    const [receiptStep, setReceiptStep] = useState<1 | 2>(1)
    const [receiptAttempted, setReceiptAttempted] = useState(false)
    const {
        form,
        isReceipt,
        existing,
        locked,
        allocations,
        draftSavedAt,
        temporarySaved,
        postedLocally,
        result,
        actionError,
        confirmOpen,
        setConfirmOpen,
        leaveConfirmOpen,
        setLeaveConfirmOpen,
        removedLine,
        undoRemoveLine,
        issues,
        receiptFactIssues,
        canSubmit,
        factAmountStr,
        proposedAllocated,
        proposedUnallocated,
        addFromPool,
        addTargetsFromPool,
        updateAmount,
        removeLine,
        fillLineAmount,
        requestClose,
        discardAndClose,
        doSaveDraft,
        doPost,
        resolveUnknown,
        receiptApproval,
        saveMutation,
        postMutation,
        resolveMutation,
    } = useAllocationSession({
        session,
        onClose,
        onPosted,
        canOperate,
        permissionReason,
        workItemId,
        expectedTaskVersion,
        taskReceivableAccountId,
    })
    const receiptPhase = customerReceiptApprovalPhase(
        receiptApproval,
        postedLocally ? "IN_APPROVAL" : "DRAFT",
    )
    const submitted = session.status === "posted" || postedLocally
    const editingDisabled =
        !canOperate ||
        result?.status === "unknown" ||
        submitted ||
        saveMutation.isPending ||
        postMutation.isPending
    const saveAction = (
        <LoadingButton
            id="customer-receivables-session-save-draft"
            size="default"
            type="button"
            loading={saveMutation.isPending}
            variant="outline"
            disabled={editingDisabled}
            title={canOperate ? undefined : permissionReason}
            onClick={() => void doSaveDraft()}
        >
            {!saveMutation.isPending && !isReceipt ? (
                <SaveIcon data-icon="inline-start" aria-hidden="true" />
            ) : null}
            {saveMutation.isPending ? "保存中…" : "保存草稿"}
        </LoadingButton>
    )
    const focusTarget = (targetId: string) => {
        requestAnimationFrame(() => {
            const target = document.getElementById(targetId)
            target?.scrollIntoView({ block: "center" })
            target?.focus({ preventScroll: true })
        })
    }
    const showReceiptStep = (step: 1 | 2) => {
        setReceiptStep(step)
        focusTarget(
            step === 1
                ? "customer-receivables-session-fact-title"
                : "customer-receivables-session-allocations-title",
        )
    }
    const locateIssue = (issue: ValidationIssue) => {
        if (!issue.targetId) return
        if (receiptFactIssues.some((item) => item.id === issue.id)) {
            setReceiptAttempted(true)
            setReceiptStep(1)
        }
        focusTarget(issue.targetId)
    }
    const nextReceiptStep = () => {
        if (editingDisabled) return
        setReceiptAttempted(true)
        if (receiptFactIssues.length > 0) {
            void form.validateAllFields("submit")
            locateIssue(receiptFactIssues[0])
            return
        }
        showReceiptStep(2)
    }
    const requestSubmit = () => {
        if (editingDisabled) return
        if (isReceipt && receiptStep === 1) {
            nextReceiptStep()
            return
        }
        setAttempted(true)
        if (isReceipt && receiptFactIssues.length > 0) {
            locateIssue(receiptFactIssues[0])
            return
        }
        if (canSubmit) void form.handleSubmit()
        else void form.validateAllFields("submit")
    }
    const FormRoot = isReceipt ? "form" : "div"
    const submitAction = canOperate ? (
        <LoadingButton
            id="customer-receivables-session-submit"
            size="default"
            type={isReceipt ? "submit" : "button"}
            loading={postMutation.isPending}
            disabled={editingDisabled || (!isReceipt && !canSubmit)}
            title={
                !canOperate
                    ? permissionReason
                    : !canSubmit
                      ? (issues[0]?.message ?? "请先填写大于零的金额")
                      : undefined
            }
            onClick={isReceipt ? undefined : () => void form.handleSubmit()}
        >
            {postMutation.isPending
                ? "提交中…"
                : isReceipt
                  ? "提交回款审批"
                  : "确认登记并核销"}
        </LoadingButton>
    ) : null
    const actions = (
        <>
            {saveAction}
            {submitAction}
        </>
    )

    return (
        <FormRoot
            id={isReceipt ? "receipt-registration-form" : undefined}
            className="@container flex min-w-0 flex-1 flex-col"
            onSubmit={(event) => {
                if (event.target !== event.currentTarget) return
                event.preventDefault()
                requestSubmit()
            }}
        >
            <div
                className={
                    isReceipt ? "flex min-w-0 flex-1 flex-col gap-6" : "min-w-0"
                }
            >
                <div
                    className={`flex min-w-0 flex-col ${isReceipt ? "gap-6" : "gap-4"}`}
                >
                    <SessionHeader
                        session={session}
                        isReceipt={isReceipt}
                        existing={existing}
                        draftSavedAt={draftSavedAt}
                        onRequestClose={requestClose}
                        showClose={!hideSessionClose}
                        submitted={submitted}
                        temporarySaved={temporarySaved && !result}
                        closeDisabled={
                            saveMutation.isPending ||
                            postMutation.isPending ||
                            resolveMutation.isPending
                        }
                    />

                    {isReceipt ? (
                        <ReceiptRegistrationSteps step={receiptStep} />
                    ) : null}
                    {result ? (
                        <FormalActionResult
                            status={
                                result.status === "failed"
                                    ? "rejected"
                                    : result.status
                            }
                            title={result.title}
                            description={result.description}
                            reference={result.reference}
                            facts={result.facts}
                            actions={
                                <>
                                    {result.pendingKey ? (
                                        <LoadingButton
                                            id="customer-receivables-session-result-resolve"
                                            type="button"
                                            loading={
                                                resolveMutation.isPending ||
                                                saveMutation.isPending
                                            }
                                            size="sm"
                                            onClick={() =>
                                                void resolveUnknown()
                                            }
                                            disabled={resolveMutation.isPending}
                                        >
                                            {resolveMutation.isPending
                                                ? "查询中…"
                                                : "查询最终结果"}
                                        </LoadingButton>
                                    ) : null}
                                    {result.returnTo ? (
                                        <Button
                                            id="customer-receivables-session-result-return"
                                            type="button"
                                            size="sm"
                                            render={
                                                <Link href={result.returnTo} />
                                            }
                                        >
                                            返回销售单
                                        </Button>
                                    ) : null}
                                    <Button
                                        id="customer-receivables-session-result-close"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={onClose}
                                    >
                                        {closeLabel}
                                    </Button>
                                </>
                            }
                        />
                    ) : null}

                    {actionError ? (
                        <Alert variant="destructive">
                            <AlertTitle>操作未成功</AlertTitle>
                            <AlertDescription>{actionError}</AlertDescription>
                        </Alert>
                    ) : null}

                    {isReceipt ? (
                        <div hidden={receiptStep !== 1} className="space-y-4">
                            <ReceiptSessionFields
                                form={form}
                                receiptId={session.existingFactId}
                                unsavedFileCount={
                                    session.fact.receiptFileNames?.length ?? 0
                                }
                                existing={existing}
                                locked={locked || editingDisabled}
                            />
                            <ValidationSummary
                                id="customer-receivables-session-fact-validation"
                                issues={
                                    receiptAttempted ? receiptFactIssues : []
                                }
                                title="请补全到账信息"
                                onLocate={locateIssue}
                            />
                        </div>
                    ) : (
                        <div className="grid gap-4 lg:grid-cols-2">
                            <SessionFactFields
                                form={form}
                                isReceipt={isReceipt}
                                existing={existing}
                                locked={locked || editingDisabled}
                            />
                            <SessionPool
                                session={session}
                                allocations={allocations}
                                disabled={
                                    !canOperate ||
                                    session.status === "posted" ||
                                    postedLocally
                                }
                                onAdd={addFromPool}
                            />
                        </div>
                    )}

                    {removedLine && (!isReceipt || receiptStep === 2) ? (
                        <div
                            role="status"
                            className="flex items-center justify-between gap-3 rounded-lg bg-muted px-3 py-2 text-sm"
                        >
                            <span>已移除 {removedLine.line.label}</span>
                            {canOperate && (
                                <Button
                                    id="customer-receivables-session-undo-remove"
                                    type="button"
                                    variant="link"
                                    size="sm"
                                    onClick={undoRemoveLine}
                                    disabled={
                                        locked ||
                                        !canOperate ||
                                        saveMutation.isPending ||
                                        postMutation.isPending
                                    }
                                >
                                    撤销
                                </Button>
                            )}
                        </div>
                    ) : null}
                    {isReceipt ? (
                        receiptStep === 2 ? (
                            <>
                                <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
                                    <span className="text-muted-foreground">
                                        到账金额
                                    </span>
                                    <MoneyValue
                                        value={factAmountStr}
                                        className="font-semibold"
                                    />
                                    <Button
                                        id="customer-receivables-session-edit-fact"
                                        type="button"
                                        size="sm"
                                        variant="link"
                                        disabled={editingDisabled}
                                        onClick={() => showReceiptStep(1)}
                                    >
                                        修改到账信息
                                    </Button>
                                </div>
                                <ReceiptAllocationTable
                                    session={session}
                                    allocations={allocations}
                                    issues={attempted ? issues : []}
                                    disabled={editingDisabled}
                                    removalDisabled={locked}
                                    onAdd={addTargetsFromPool}
                                    onLocateIssue={locateIssue}
                                    onRemove={removeLine}
                                    onAmountChange={updateAmount}
                                    onFill={fillLineAmount}
                                />
                            </>
                        ) : null
                    ) : (
                        <AllocationWorkspace
                            id="customer-receivables-session-allocations"
                            title="本次分配"
                            description="拟分配金额仅供参考，以提交后结果为准。"
                            summary={{
                                totalToAllocate: (
                                    <MoneyValue
                                        value={factAmountStr || "0"}
                                        taxBasis="gross"
                                    />
                                ),
                                allocated: (
                                    <span>
                                        <MoneyValue
                                            value={money(proposedAllocated)}
                                            taxBasis="gross"
                                        />
                                        <span className="ml-1 text-xs text-muted-foreground">
                                            拟
                                        </span>
                                    </span>
                                ),
                                difference: (
                                    <span>
                                        <MoneyValue
                                            value={money(proposedUnallocated)}
                                            taxBasis="gross"
                                        />
                                        <span className="ml-1 text-xs text-muted-foreground">
                                            拟未分配
                                        </span>
                                    </span>
                                ),
                            }}
                            allocations={allocations}
                            getRowId={(a) => a.lineKey}
                            disabled={
                                !canOperate ||
                                session.status === "posted" ||
                                postedLocally
                            }
                            addLabel="从池中选择"
                            addDisabledReason="请从左侧同主体池加入目标"
                            onRemoveAllocation={(a) => removeLine(a.lineKey)}
                            columns={[
                                {
                                    id: "target",
                                    header: "目标",
                                    renderValue: ({ item }) => (
                                        <div>
                                            <div className="text-sm">
                                                {item.label}
                                            </div>
                                            <div className="num text-xs text-muted-foreground">
                                                {item.salesOrderNo}
                                            </div>
                                        </div>
                                    ),
                                },
                                {
                                    id: "open",
                                    header: "开放余额",
                                    align: "end",
                                    numeric: true,
                                    renderValue: ({ item }) => (
                                        <MoneyValue
                                            value={item.openAmount}
                                            taxBasis="gross"
                                        />
                                    ),
                                },
                                {
                                    id: "amount",
                                    header: "分配金额",
                                    required: true,
                                    align: "end",
                                    numeric: true,
                                    renderValue: ({ item }) => (
                                        <MoneyValue
                                            value={item.amount || "0"}
                                        />
                                    ),
                                    renderEditor: ({ item }) => (
                                        <div className="flex items-center justify-end gap-1">
                                            <MoneyInput
                                                id={`customer-receivables-session-allocation-${toAutomationIdSegment(item.lineKey)}-amount`}
                                                className="num text-right"
                                                value={item.amount}
                                                inputMode="decimal"
                                                aria-label={`${item.label} 分配金额`}
                                                onChange={(e) =>
                                                    updateAmount(
                                                        item.lineKey,
                                                        e.target.value,
                                                    )
                                                }
                                            />
                                            <Button
                                                id={`customer-receivables-session-allocation-${toAutomationIdSegment(item.lineKey)}-fill`}
                                                type="button"
                                                size="xs"
                                                variant="ghost"
                                                onClick={() =>
                                                    fillLineAmount(item)
                                                }
                                            >
                                                填满
                                            </Button>
                                        </div>
                                    ),
                                },
                            ]}
                            statusNotice={
                                issues.length > 0 ? (
                                    <ValidationSummary
                                        issues={issues}
                                        title="分配校验"
                                    />
                                ) : (
                                    <p className="text-xs text-muted-foreground">
                                        {session.submitPolicy.label}
                                    </p>
                                )
                            }
                            actions={actions}
                        />
                    )}
                    {isReceipt &&
                    receiptStep === 2 &&
                    !submitted &&
                    receiptApproval ? (
                        <CustomerReceiptApprovalArea
                            phase={receiptPhase}
                            approval={receiptApproval}
                            documentId={session.existingFactId}
                        />
                    ) : null}
                </div>
                {isReceipt ? (
                    receiptStep === 1 ? (
                        <div className="flex flex-wrap items-center justify-end gap-3 border-t pt-4">
                            {saveAction}
                            <Button
                                id="customer-receivables-session-next"
                                type="submit"
                                disabled={editingDisabled}
                            >
                                下一步：核销应收
                            </Button>
                        </div>
                    ) : (
                        <ReceiptSessionFooter
                            submitHint={
                                attempted && !submitted && !canSubmit
                                    ? (issues[0]?.message ??
                                      "请填写大于零的到账金额。")
                                    : undefined
                            }
                            allocationCount={allocations.length}
                            amount={factAmountStr}
                            allocated={proposedAllocated}
                            unallocated={proposedUnallocated}
                            existing={existing}
                            submitted={submitted}
                            actions={
                                <>
                                    <Button
                                        id="customer-receivables-session-previous"
                                        type="button"
                                        variant="ghost"
                                        disabled={editingDisabled}
                                        onClick={() => showReceiptStep(1)}
                                    >
                                        上一步
                                    </Button>
                                    {actions}
                                </>
                            }
                        />
                    )
                ) : null}
            </div>

            {/* 离开前未保存草稿确认 */}
            <DiscardConfirmDialog
                id="customer-receivables-session-discard-dialog"
                open={leaveConfirmOpen}
                onOpenChange={setLeaveConfirmOpen}
                title={
                    isReceipt
                        ? "回款登记尚有未保存的更改，确定离开？"
                        : "本次核销尚未保存草稿，确定离开？"
                }
                description={
                    isReceipt
                        ? "到账信息与拟核销金额尚未保存，可先保存草稿再离开。"
                        : "记录表单与分配金额尚未保存，离开后将丢失；可先「保存草稿」再离开。"
                }
                confirmLabel="放弃并离开"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setLeaveConfirmOpen(false)
                    discardAndClose()
                }}
            />

            {isReceipt ? (
                <CustomerReceiptSubmitConfirmDialog
                    id="customer-receivables-session-receipt-confirm-dialog"
                    open={confirmOpen}
                    pending={postMutation.isPending || saveMutation.isPending}
                    approval={receiptApproval}
                    counterpartyName={session.counterpartyPartyName}
                    amountLabel="回款金额"
                    amount={factAmountStr}
                    allocationCount={allocations.length}
                    allocatedAmount={proposedAllocated}
                    unallocatedAmount={proposedUnallocated}
                    onOpenChange={setConfirmOpen}
                    onConfirm={(reason) => void doPost(reason)}
                />
            ) : (
                <FormalActionConfirmDialog
                    actionVariant="default"
                    id="customer-receivables-session-invoice-confirm-dialog"
                    open={confirmOpen}
                    onOpenChange={setConfirmOpen}
                    title="确认登记销项发票并分配"
                    actionLabel="提交"
                    confirmLabel="登记销项发票"
                    fromStatus={{ label: "本次草稿", tone: "warning" }}
                    toStatus={{
                        label: "已登记发票",
                        tone: "success",
                    }}
                    description="登记后更新应收余额；记录不可编辑，纠错须开红票。"
                    summary={[
                        `结算主体：${session.counterpartyPartyName}`,
                        <span key="amount">
                            发票金额：
                            <MoneyValue value={factAmountStr} />
                        </span>,
                        <span key="allocations">
                            分配 {allocations.length} 笔：
                            <MoneyValue value={proposedAllocated} />
                        </span>,
                        <span key="remainder">
                            未分配：
                            <MoneyValue value={proposedUnallocated} />
                        </span>,
                    ]}
                    nextDepartment="财务"
                    pending={postMutation.isPending || saveMutation.isPending}
                    onConfirm={() => void doPost()}
                />
            )}
        </FormRoot>
    )
}
