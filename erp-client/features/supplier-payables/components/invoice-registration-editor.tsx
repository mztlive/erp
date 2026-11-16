"use client"

import { useState } from "react"
import { LockKeyholeIcon } from "lucide-react"
import {
    FormalActionConfirmDialog,
    MoneyValue,
    PageHeader,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { InvoiceAllocationTable } from "./invoice-allocation-table"
import { InvoiceRegistrationFields } from "./invoice-registration-fields"
import { InvoiceSessionFooter } from "./invoice-session-footer"
import { AllocationResultView } from "./allocation-result-view"
import type { AllocationSessionState } from "@/features/supplier-payables/hooks/use-allocation-session"
import { invoiceSchema } from "@/features/supplier-payables/lib/allocation-model"

const fieldTargets: Record<string, string> = {
    invoiceFiles: "supplier-payables-invoice-files-select",
    invoiceNo: "supplier-payables-allocation-form-invoice-no",
    invoiceDate: "supplier-payables-allocation-form-invoice-date",
    grossAmount: "supplier-payables-allocation-form-gross-amount",
    netAmount: "supplier-payables-allocation-form-net-amount",
    taxAmount: "supplier-payables-allocation-form-tax-amount",
}

/** 进项发票登记工作面：左右分栏工作区（发票信息 + 核销明细）与吸底核对栏。 */
export function InvoiceRegistrationEditor({
    state,
    draftHint,
    onSaveDraft,
    onClose,
    onGoToInvoiceView,
}: {
    state: AllocationSessionState
    draftHint: string | null
    onSaveDraft: () => void
    onClose: () => void
    onGoToInvoiceView: () => void
}) {
    const [attempted, setAttempted] = useState(false)
    const {
        session,
        result,
        invoiceForm,
        isSubmitting,
        factAmount,
        allocatedHint,
        unallocatedHint,
        selected,
    } = state

    if (!session) return null

    const parsed = invoiceSchema.safeParse(invoiceForm.state.values)
    const formIssues: ValidationIssue[] =
        parsed.success || session.existingInvoiceId
            ? []
            : parsed.error.issues.map((issue, index) => ({
                  id: `invoice-field-${index}`,
                  label: "发票信息",
                  message: issue.message,
                  targetId: fieldTargets[String(issue.path[0])],
              }))
    const issues = [...formIssues, ...state.issues]

    function locateIssue(issue: ValidationIssue) {
        if (!issue.targetId) return
        const element =
            document.getElementById(issue.targetId) ||
            document.getElementById(
                issue.targetId.replace("-select", "-input"),
            ) ||
            document.getElementById(issue.targetId.replace("-input", "-select"))
        if (element) {
            element.focus()
            element.scrollIntoView({ behavior: "smooth", block: "center" })
        }
    }

    const submit = () => {
        setAttempted(true)
        if (!issues.length && state.canSubmit) {
            state.requestSubmit()
        } else {
            void invoiceForm.validateAllFields("submit")
            if (issues[0]?.targetId) {
                locateIssue(issues[0])
            }
        }
    }

    if (result)
        return (
            <AllocationResultView
                result={result}
                hasSubmitKey={state.hasSubmitKey}
                pending={isSubmitting}
                onClose={onClose}
                onGoToInvoiceView={onGoToInvoiceView}
                onResolveUnknown={state.handleResolveUnknown}
            />
        )

    const title = session.existingInvoiceId
        ? "继续核销进项发票"
        : "登记进项发票"

    return (
        <form
            id="invoice-registration-form"
            className="@container flex min-w-0 flex-1 flex-col gap-6"
            onSubmit={(event) => {
                event.preventDefault()
                submit()
            }}
        >
            {/* 顶栏：标题与状态（扁平无卡片，无右上角返回按钮） */}
            <div className="space-y-3 border-b border-border pb-4">
                <PageHeader
                    title={title}
                    status={{ label: "草稿", tone: "neutral" }}
                />
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
                    <span className="text-muted-foreground">供应商</span>
                    <LockKeyholeIcon
                        className="size-4 shrink-0 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <span className="min-w-0 break-words font-medium">
                        {session.supplierName}
                    </span>
                    {draftHint ? (
                        <span className="text-xs text-muted-foreground">
                            {draftHint}
                        </span>
                    ) : null}
                </div>
            </div>

            {/* 左右分栏工作区 */}
            <div className="grid min-w-0 flex-1 grid-cols-1 items-start lg:grid-cols-[380px_minmax(0,1fr)] xl:grid-cols-[400px_minmax(0,1fr)]">
                {/* 左侧：发票信息（扁平无卡片，右分割线，宽屏吸顶） */}
                <div className="border-b border-border pb-6 lg:border-b-0 lg:border-r lg:pb-0 lg:pr-8">
                    <div className="space-y-5 lg:sticky lg:top-4">
                        <div className="space-y-1">
                            <h2 className="text-sm font-semibold text-foreground">
                                发票信息
                            </h2>
                            <p className="text-xs text-muted-foreground">
                                录入发票号码、开票日期、金额与发票文件
                            </p>
                        </div>

                        <InvoiceRegistrationFields state={state} />

                        {attempted && formIssues.length > 0 ? (
                            <ValidationSummary
                                id="supplier-payables-invoice-fact-validation"
                                issues={formIssues}
                                title="请补全发票信息"
                                onLocate={locateIssue}
                            />
                        ) : null}
                    </div>
                </div>

                {/* 右侧：核销明细（扁平无卡片，左内边距与左侧隔开） */}
                <div className="min-w-0 pt-6 lg:pt-0 lg:pl-8 space-y-6">
                    <InvoiceAllocationTable state={state} />

                    {attempted && state.issues.length > 0 ? (
                        <ValidationSummary
                            id="supplier-payables-invoice-table-validation"
                            issues={state.issues}
                            title="分配校验"
                            onLocate={locateIssue}
                        />
                    ) : null}
                </div>
            </div>

            {/* 底部吸底核对栏 */}
            <InvoiceSessionFooter
                factAmount={factAmount}
                allocatedHint={allocatedHint}
                unallocatedHint={unallocatedHint}
                isExistingInvoice={Boolean(session.existingInvoiceId)}
                selectedCount={selected.size}
                draftHint={draftHint}
                submitHint={
                    attempted && issues.length > 0
                        ? (issues[0]?.message ?? "请检查填写内容。")
                        : undefined
                }
                isSubmitting={isSubmitting}
                onSaveDraft={onSaveDraft}
            />

            <FormalActionConfirmDialog
                idPrefix="supplier-payables-invoice-allocate-confirm"
                open={state.confirmOpen}
                onOpenChange={state.setConfirmOpen}
                actionVariant="default"
                actionLabel="登记进项发票并核销"
                title="确认登记进项发票并核销"
                confirmLabel="登记进项发票"
                description="登记后更新收票核销金额；记录不可编辑，纠错须开红票。"
                fromStatus={{ label: "本次草稿", tone: "neutral" }}
                toStatus={{ label: "已确认", tone: "success" }}
                summary={[
                    `供应商：${session.supplierName}`,
                    `发票号码：${invoiceForm.state.values.invoiceNo}`,
                    <span key="gross">
                        发票金额：
                        <MoneyValue value={factAmount} />
                    </span>,
                    `核销单据：${state.selected.size} 笔`,
                    `发票文件：${invoiceForm.state.values.invoiceFiles.length} 个`,
                    <span key="allocated">
                        本次核销：
                        <MoneyValue value={allocatedHint} />
                    </span>,
                    <span key="unallocated">
                        未核销：
                        <MoneyValue value={unallocatedHint} />
                    </span>,
                ]}
                pending={isSubmitting}
                onConfirm={() => void state.doSubmit()}
            />
        </form>
    )
}
