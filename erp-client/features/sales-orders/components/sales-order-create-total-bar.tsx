"use client"

import { LoaderCircleIcon } from "lucide-react"
import { useSelector } from "@tanstack/react-form"

import {
    MoneyValue,
    StickyTotalBar,
    ValidationSummary,
} from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { calculateTotals } from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"

const HEADER_VALIDATION_FIELDS = [
    { name: "contractId", label: "有效合同", targetId: "contractId" },
    {
        name: "ownerName",
        label: "负责销售",
        targetId: "sales-orders-create-header-owner-name",
    },
    {
        name: "welfareScene",
        label: "福利场景",
        targetId: "sales-orders-create-header-welfare-scene",
    },
    {
        name: "paymentTerms",
        label: "付款条件",
        targetId: "sales-orders-create-header-payment-terms",
    },
    {
        name: "fulfillmentDeadline",
        label: "履约期限",
        targetId: "sales-orders-create-header-fulfillment-deadline",
    },
    {
        name: "receivableDueDate",
        label: "应收到期日",
        targetId: "sales-orders-create-header-receivable-due-date",
    },
    {
        name: "taxRatePercent",
        label: "税率",
        targetId: "sales-orders-create-header-tax-rate",
    },
    { name: "customerName", label: "客户", targetId: "contractId" },
    { name: "settlementEntity", label: "结算主体", targetId: "contractId" },
] as const

export type SalesOrderCreateTotalBarProps = {
    form: SalesOrderCreateFormApi
    isSubmitting: boolean
    onSaveDraftClick: () => void
    onSubmitClick: () => void
}

export function SalesOrderCreateTotalBar({
    form,
    isSubmitting,
    onSaveDraftClick,
    onSubmitClick,
}: SalesOrderCreateTotalBarProps) {
    /** 提交失败后汇总单据头错误，避免只拦提交却看不到原因。 */
    const headerIssues = useSelector(form.store, (state) => {
        if (state.submissionAttempts === 0) return []
        return HEADER_VALIDATION_FIELDS.flatMap((field) => {
            const meta = state.fieldMeta[field.name]
            return toFieldErrors(meta?.errors ?? [])
                .filter((error) => Boolean(error?.message))
                .map((error, index) => ({
                    id: `${field.name}-${index}`,
                    label: field.label,
                    message: error!.message!,
                    targetId: field.targetId,
                }))
        })
    })

    return (
        <>
            {headerIssues.length > 0 ? (
                <ValidationSummary
                    className="border-t border-grid pt-4"
                    issues={headerIssues}
                    title={`基本信息共 ${headerIssues.length} 项待处理`}
                />
            ) : null}

            <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                    const totals = calculateTotals(
                        values.lineItems,
                        values.taxRatePercent,
                    )
                    return (
                        <StickyTotalBar
                            className="mt-auto rounded-none border-0 border-t border-grid px-0 py-3 shadow-none [&>div>div.grid]:block"
                            items={[
                                {
                                    id: "gross",
                                    label: "含税合计",
                                    value: (
                                        <MoneyValue
                                            value={totals.gross}
                                            className="text-xl font-semibold"
                                        />
                                    ),
                                    description: (
                                        <div className="flex flex-wrap items-center gap-x-5 gap-y-1">
                                            <span>
                                                不含税金额{" "}
                                                <MoneyValue
                                                    value={totals.net}
                                                />
                                            </span>
                                            <span>
                                                税额{" "}
                                                <MoneyValue
                                                    value={totals.tax}
                                                />
                                                （税率{" "}
                                                {values.taxRatePercent || "0"}
                                                %）
                                            </span>
                                        </div>
                                    ),
                                },
                            ]}
                            note="提交后进入审批"
                            actions={
                                <form.AppForm>
                                    <form.SubmitButton
                                        id="sales-orders-create-save-draft"
                                        variant="outline"
                                        label={
                                            isSubmitting
                                                ? "处理中…"
                                                : "保存草稿"
                                        }
                                        pendingLabel="正在保存草稿…"
                                        disabled={isSubmitting}
                                        onClick={onSaveDraftClick}
                                    />
                                    <form.SubmitButton
                                        id="sales-orders-create-submit"
                                        data-testid="sales-order-submit"
                                        label="提交审批"
                                        pendingLabel="正在提交…"
                                        disabled={isSubmitting}
                                        onClick={onSubmitClick}
                                    >
                                        {isSubmitting ? (
                                            <LoaderCircleIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                                className="animate-spin"
                                            />
                                        ) : null}
                                        {isSubmitting ? "处理中…" : "提交审批"}
                                    </form.SubmitButton>
                                </form.AppForm>
                            }
                        />
                    )
                }}
            </form.Subscribe>
        </>
    )
}
