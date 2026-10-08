"use client"

import { useState } from "react"
import { z } from "zod"
import { useAppForm, toFieldErrors } from "@/components/form"
import {
    Field,
    FieldLabel,
    FieldDescription,
    FieldError,
} from "@/components/ui/field"
import { Alert, AlertTitle, AlertDescription } from "@/components/ui/alert"
import { CustomerSearchCombobox } from "@/features/entity-selectors"
import { SettlementPartySearchCombobox } from "@/features/party-selector/settlement-party-search-combobox"
import type { ContractImportTask } from "@/features/contracts/api/upload"
import type {
    SalesContractMatches,
    SalesContractPrefill,
} from "../api/contract-prefill"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"

export function SalesContractPrefillReview({
    task,
    matches,
    currentCustomerId,
    onApply,
}: {
    task: ContractImportTask
    matches: SalesContractMatches
    currentCustomerId: string
    onApply: (value: SalesContractPrefill) => void
}) {
    const [defaults] = useState(() => ({
        customerId: matches.customer.item?.id ?? "",
        customerName: matches.customer.item?.legalName ?? "",
        settlementPartyId: matches.settlement.item?.partyId ?? "",
        settlementEntity: matches.settlement.item?.displayName ?? "",
        paymentTerms:
            PAYMENT_TERM_OPTIONS.find(
                (option) => option.label === task.draft?.fields.payment_terms,
            )?.value ?? "",
        invoiceType: task.draft?.fields.invoice_type ?? "",
        taxRatePercent: task.draft?.fields.tax_point ?? "",
    }))
    const form = useAppForm({
        defaultValues: defaults,
        validators: {
            onSubmit: z.object({
                customerId: z.string().min(1, "请选择系统客户"),
                customerName: z.string(),
                settlementEntity: z.string(),
                settlementPartyId: z.string().min(1, "请选择系统结算主体"),
                paymentTerms: z.string().min(1, "请选择付款条件"),
                invoiceType: z.string().min(1, "请选择开票要求"),
                taxRatePercent: z.string().min(1, "请选择税率"),
            }),
        },
        onSubmit: ({ value }) =>
            onApply({
                ...value,
                file: {
                    id: task.source_file_asset_id,
                    fileName: task.file_name,
                },
            }),
    })
    return (
        <form
            id="sales-contract-prefill-review"
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <p className="text-sm text-muted-foreground">
                核对匹配结果；未确定的项目请从系统选项中选择。
            </p>
            <div className="grid gap-4 sm:grid-cols-2">
                <form.AppField name="customerId">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor="sales-contract-prefill-customer">
                                客户 <span className="text-destructive">*</span>
                            </FieldLabel>
                            <CustomerSearchCombobox
                                id="sales-contract-prefill-customer"
                                value={field.state.value || undefined}
                                onValueChange={(id) =>
                                    field.handleChange(id ?? "")
                                }
                                onItemChange={(item) =>
                                    form.setFieldValue(
                                        "customerName",
                                        item?.legalName ?? "",
                                    )
                                }
                                placeholder="搜索并选择系统客户"
                            />
                            <FieldDescription>
                                合同名称：
                                {task.draft?.fields.customer_name ||
                                    "未识别"} ·{" "}
                                {matchMessage(
                                    field.state.value,
                                    matches.customer.item?.id,
                                    matches.customer.message,
                                )}
                            </FieldDescription>
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </Field>
                    )}
                </form.AppField>

                <form.AppField name="settlementPartyId">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor="sales-contract-prefill-settlement">
                                结算主体{" "}
                                <span className="text-destructive">*</span>
                            </FieldLabel>
                            <SettlementPartySearchCombobox
                                id="sales-contract-prefill-settlement"
                                purpose="sales-order"
                                value={field.state.value || undefined}
                                onValueChange={(id) =>
                                    field.handleChange(id ?? "")
                                }
                                onItemChange={(item) =>
                                    form.setFieldValue(
                                        "settlementEntity",
                                        item?.displayName ?? "",
                                    )
                                }
                                placeholder="搜索并选择结算主体"
                            />
                            <FieldDescription>
                                合同名称：
                                {task.draft?.fields.settlement_name ||
                                    "未识别"}{" "}
                                ·{" "}
                                {matchMessage(
                                    field.state.value,
                                    matches.settlement.item?.partyId,
                                    matches.settlement.message,
                                )}
                            </FieldDescription>
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </Field>
                    )}
                </form.AppField>
            </div>
            <form.Subscribe selector={(state) => state.values.customerId}>
                {(id) =>
                    currentCustomerId && id && id !== currentCustomerId ? (
                        <Alert>
                            <AlertTitle>将更换当前销售单的客户</AlertTitle>
                            <AlertDescription>
                                识别匹配的客户与页面原先选择的客户不同，请核对。点击“填入销售单”后将使用上方所选客户。
                            </AlertDescription>
                        </Alert>
                    ) : null
                }
            </form.Subscribe>
            <div className="grid gap-4 sm:grid-cols-3">
                <form.AppField name="paymentTerms">
                    {(field) => (
                        <field.SelectField
                            id="sales-contract-prefill-payment"
                            label="付款条件"
                            required
                            options={PAYMENT_TERM_OPTIONS}
                            placeholder="请选择付款条件"
                        />
                    )}
                </form.AppField>
                <form.AppField name="invoiceType">
                    {(field) => (
                        <field.SelectField
                            id="sales-contract-prefill-invoice"
                            label="开票要求"
                            required
                            options={[
                                "增值税专用发票",
                                "增值税普通发票",
                                "不开发票",
                            ].map((value) => ({ value, label: value }))}
                            placeholder="请选择开票要求"
                        />
                    )}
                </form.AppField>
                <form.AppField name="taxRatePercent">
                    {(field) => (
                        <field.SelectField
                            id="sales-contract-prefill-tax"
                            label="税率"
                            required
                            options={["0", "1", "3", "6", "9", "13"].map(
                                (value) => ({ value, label: `${value}%` }),
                            )}
                            placeholder="请选择税率"
                        />
                    )}
                </form.AppField>
            </div>
            <p className="text-xs text-muted-foreground">
                填入会更新销售单的以上五项，并保留原 PDF
                作为开单材料。此操作不会提交销售单或归档合同。
            </p>
        </form>
    )
}

function matchMessage(
    selected: string,
    matched: string | undefined,
    message: string,
) {
    if (selected && selected !== matched) return "已手动选择，请与合同核对"
    if (!selected && matched) return "请选择系统记录"
    return message
}
