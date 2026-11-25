"use client"

import { useState } from "react"
import { useStore } from "@tanstack/react-form"
import { ArrowLeftIcon, LockKeyholeIcon } from "lucide-react"
import {
    DetailWorkspaceChrome,
    FormalActionConfirmDialog,
    MoneyValue,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { InvoiceAllocationTable } from "./invoice-allocation-table"
import { InvoiceRegistrationFields } from "./invoice-registration-fields"
import { InvoiceSessionFooter } from "./invoice-session-footer"
import { AllocationResultView } from "./allocation-result-view"
import type { AllocationSessionState } from "@/features/supplier-payables/hooks/use-allocation-session"
import {
    cents,
    fromCents,
    invoiceSchema,
} from "@/features/supplier-payables/lib/allocation-model"

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

    const taxValues = useStore(invoiceForm.store, (s) => ({
        faceTax: s.values.taxAmount,
        allocationTaxes: s.values.allocationTaxes,
    }))

    if (!session) return null

    const splitTax = selected.size > 1
    const faceTaxCents = cents(taxValues.faceTax || "0")
    let allocatedTaxCents = BigInt(0)
    if (splitTax) {
        for (const id of selected) {
            allocatedTaxCents += cents(taxValues.allocationTaxes[id] || "0")
        }
    }
    const taxMismatchHint =
        splitTax &&
        faceTaxCents > BigInt(0) &&
        allocatedTaxCents !== faceTaxCents
            ? `已分配税额 ¥${fromCents(allocatedTaxCents)} 与票面税额 ¥${fromCents(faceTaxCents)} 不一致`
            : undefined

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
            <div className="mx-auto w-full max-w-3xl px-(--scaffold-px) py-8">
                <AllocationResultView
                    result={result}
                    hasSubmitKey={state.hasSubmitKey}
                    pending={isSubmitting}
                    onClose={onClose}
                    onGoToInvoiceView={onGoToInvoiceView}
                    onResolveUnknown={state.handleResolveUnknown}
                />
            </div>
        )

    const title = session.existingInvoiceId
        ? "继续核销进项发票"
        : "登记进项发票"

    return (
        <form
            id="invoice-registration-form"
            className="@container flex min-w-0 flex-1 flex-col"
            onSubmit={(event) => {
                event.preventDefault()
                submit()
            }}
        >
            <DetailWorkspaceChrome
                title={title}
                primaryStatus={{ label: "草稿", tone: "neutral" }}
                meta={
                    <span className="inline-flex flex-wrap items-center gap-x-2.5 gap-y-1">
                        <span className="inline-flex items-center gap-1.5 text-foreground">
                            <span className="text-muted-foreground">
                                供应商
                            </span>
                            <LockKeyholeIcon
                                className="size-3.5 shrink-0 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <span className="font-medium">
                                {session.supplierName}
                            </span>
                        </span>
                        {draftHint ? (
                            <span className="text-muted-foreground">
                                · {draftHint}
                            </span>
                        ) : null}
                    </span>
                }
                secondaryActions={
                    <Button
                        id="invoice-registration-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={isSubmitting}
                        onClick={onClose}
                    >
                        <ArrowLeftIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        返回供应商往来
                    </Button>
                }
            />

            {/* 左右分栏工作区 */}
            <div className="grid min-w-0 flex-1 grid-cols-1 items-start px-(--scaffold-px) py-6 lg:grid-cols-[380px_minmax(0,1fr)] @[1120px]/workspace:grid-cols-[400px_minmax(0,1fr)]">
                {/* 左侧：发票信息（扁平无卡片，右分割线，宽屏吸顶） */}
                <div className="border-b border-border/60 pb-6 lg:border-b-0 lg:border-r lg:pb-0 lg:pr-8">
                    <div className="space-y-5 lg:sticky lg:top-20">
                        <h2 className="text-section-title font-semibold text-foreground tracking-tight">
                            发票信息
                        </h2>

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
                <div className="min-w-0 space-y-6 pt-6 lg:pt-0 lg:pl-8">
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
                taxMismatchHint={taxMismatchHint}
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
