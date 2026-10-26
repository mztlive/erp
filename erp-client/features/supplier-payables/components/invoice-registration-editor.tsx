"use client"

import { useState } from "react"
import { LockKeyholeIcon } from "lucide-react"
import {
    FormalActionConfirmDialog,
    MoneyValue,
    StickyTotalBar,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { LoadingButton } from "@/components/ui/loading-button"
import { InvoiceAllocationTable } from "./invoice-allocation-table"
import { InvoiceRegistrationFields } from "./invoice-registration-fields"
import { AllocationResultView } from "./allocation-result-view"
import type { AllocationSessionState } from "@/features/supplier-payables/hooks/use-allocation-session"
import { invoiceSchema } from "@/features/supplier-payables/lib/allocation-model"

const fieldTargets: Record<string, string> = {
    invoiceFiles: "supplier-payables-invoice-files-input",
    invoiceNo: "supplier-payables-allocation-form-invoice-no",
    invoiceDate: "supplier-payables-allocation-form-invoice-date",
    grossAmount: "supplier-payables-allocation-form-gross-amount",
    netAmount: "supplier-payables-allocation-form-net-amount",
    taxAmount: "supplier-payables-allocation-form-tax-amount",
}

/** 独立登记工作面：票面信息 → 应付分配表 → 固定金额与提交栏。 */
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
    const submit = () => {
        setAttempted(true)
        if (!issues.length && state.canSubmit) state.requestSubmit()
        else void invoiceForm.validateAllFields("submit")
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

    return (
        <form
            id="invoice-registration-form"
            className="flex min-w-0 flex-1 flex-col gap-6"
            onSubmit={(event) => {
                event.preventDefault()
                submit()
            }}
        >
            <div className="flex flex-wrap items-center gap-3 rounded-md bg-muted/40 px-4 py-3 text-sm">
                <span className="text-muted-foreground">供应商</span>
                <LockKeyholeIcon
                    className="size-4 text-muted-foreground"
                    aria-hidden="true"
                />
                <span className="font-medium">{session.supplierName}</span>
                <span className="text-xs text-muted-foreground">已选定</span>
            </div>
            <InvoiceRegistrationFields state={state} />
            <InvoiceAllocationTable state={state} />
            {attempted ? <ValidationSummary issues={issues} /> : null}
            <StickyTotalBar
                className="mt-auto"
                items={[
                    {
                        id: "invoice",
                        label: session.existingInvoiceId
                            ? "本次可核销金额"
                            : "发票金额",
                        value: <MoneyValue value={factAmount || "0"} />,
                    },
                    {
                        id: "allocated",
                        label: "本次核销",
                        value: <MoneyValue value={allocatedHint} />,
                    },
                    {
                        id: "unallocated",
                        label: "未核销",
                        value: <MoneyValue value={unallocatedHint} />,
                    },
                ]}
                note={draftHint}
                actions={
                    <>
                        <LoadingButton
                            id="supplier-payables-session-page-save-draft"
                            type="button"
                            variant="outline"
                            disabled={isSubmitting}
                            onClick={onSaveDraft}
                        >
                            保存草稿
                        </LoadingButton>
                        <LoadingButton
                            id="supplier-payables-allocation-form-submit"
                            type="submit"
                            loading={isSubmitting}
                            disabled={
                                isSubmitting ||
                                Boolean(session.existingInvoiceId)
                            }
                        >
                            确认登记并核销
                        </LoadingButton>
                    </>
                }
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
