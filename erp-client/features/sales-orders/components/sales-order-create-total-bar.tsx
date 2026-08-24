"use client"

import { PlusIcon } from "lucide-react"
import { useSelector } from "@tanstack/react-form"

import {
    MoneyValue,
    StickyTotalBar,
    ValidationSummary,
} from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { calculateTotals } from "@/features/sales-orders/lib/sales-order-create-model"
import type {
    SalesOrderCreateFormApi,
    SalesOrderEditorPurpose,
} from "@/features/sales-orders/lib/sales-order-create-form-types"

const HEADER_VALIDATION_FIELDS = [
    { name: "contractId", label: "有效合同", targetId: "contractId" },
    { name: "ownerName", label: "负责销售", targetId: "ownerName" },
    { name: "welfareScene", label: "福利场景", targetId: "welfareScene" },
    { name: "paymentTerms", label: "付款条件", targetId: "paymentTerms" },
    {
        name: "fulfillmentDeadline",
        label: "履约期限",
        targetId: "fulfillmentDeadline",
    },
    { name: "targetMallId", label: "目标商城", targetId: "targetMallId" },
    {
        name: "receivableDueDate",
        label: "应收到期日",
        targetId: "receivableDueDate",
    },
    { name: "taxRatePercent", label: "税率", targetId: "taxRatePercent" },
    { name: "customerName", label: "客户", targetId: "contractId" },
    { name: "settlementEntity", label: "结算主体", targetId: "contractId" },
] as const

export type SalesOrderCreateTotalBarProps = {
    form: SalesOrderCreateFormApi
    purpose: SalesOrderEditorPurpose
    onSaveDraftClick: () => void
    onSubmitClick: () => void
}

export function SalesOrderCreateTotalBar({
    form,
    purpose,
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
                    className="border-t border-grid px-4 pt-4 md:px-5 lg:px-6"
                    issues={headerIssues}
                    title={`单据头共 ${headerIssues.length} 项待处理`}
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
                            className="rounded-none border-0 border-t border-grid px-4 py-4 shadow-none md:px-5 md:py-4"
                            items={[
                                {
                                    id: "gross",
                                    label: "含税金额",
                                    value: (
                                        <MoneyValue
                                            value={totals.gross}
                                            taxBasis="gross"
                                        />
                                    ),
                                },
                                {
                                    id: "net",
                                    label: "不含税金额",
                                    value: (
                                        <MoneyValue
                                            value={totals.net}
                                            taxBasis="net"
                                        />
                                    ),
                                },
                                {
                                    id: "tax",
                                    label: "税额",
                                    value: <MoneyValue value={totals.tax} />,
                                },
                            ]}
                            note={
                                <>税率 {values.taxRatePercent || "0"}% 预估。</>
                            }
                            actions={
                                <form.AppForm>
                                    <form.SubmitButton
                                        variant="outline"
                                        label="保存草稿"
                                        pendingLabel="正在保存草稿…"
                                        onClick={onSaveDraftClick}
                                    />
                                    <form.SubmitButton
                                        data-testid="sales-order-submit"
                                        label={
                                            purpose === "resubmit"
                                                ? "再报给采购"
                                                : "提交"
                                        }
                                        pendingLabel={
                                            purpose === "resubmit"
                                                ? "正在准备重提…"
                                                : "正在提交…"
                                        }
                                        onClick={onSubmitClick}
                                    >
                                        {purpose === "resubmit" ? (
                                            "再报给采购"
                                        ) : (
                                            <>
                                                <PlusIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                提交
                                            </>
                                        )}
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
