"use client"

import { InvoiceFilesField } from "@/components/business/invoice-files-field"
import { toFieldErrors } from "@/components/form"
import { MoneyValue } from "@/components/business"
import type { AllocationSessionState } from "@/features/supplier-payables/hooks/use-allocation-session"

/** 票面信息按三列排列；已有发票只读，不允许修改原票。 */
export function InvoiceRegistrationFields({
    state,
}: {
    state: AllocationSessionState
}) {
    const { invoiceForm: form, session, isSubmitting } = state
    return (
        <section
            className="space-y-4 border-b border-border pb-6"
            aria-labelledby="invoice-registration-fields-title"
        >
            <h2
                id="invoice-registration-fields-title"
                className="text-base font-semibold"
            >
                发票信息
            </h2>
            {session?.existingInvoiceId ? (
                <div className="space-y-2 text-sm">
                    <p>原发票 {session.existingDocumentNo}</p>
                    <p>
                        未核销金额{" "}
                        <MoneyValue value={session.existingUnallocated} />
                    </p>
                    <p className="text-muted-foreground">
                        当前发票不能继续追加核销。请登记新发票，或联系管理员处理。
                    </p>
                </div>
            ) : (
                <div className="grid items-start gap-x-6 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
                    <form.AppField name="invoiceNo">
                        {(field) => (
                            <field.TextField
                                id="supplier-payables-allocation-form-invoice-no"
                                label="发票号码"
                                placeholder="请输入发票号码"
                                required
                                disabled={isSubmitting}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="invoiceCode">
                        {(field) => (
                            <field.TextField
                                id="supplier-payables-allocation-form-invoice-code"
                                label="发票代码"
                                placeholder="可不填"
                                disabled={isSubmitting}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="invoiceDate">
                        {(field) => (
                            <field.DateField
                                id="supplier-payables-allocation-form-invoice-date"
                                label="开票日期"
                                required
                                clearable={false}
                                disabled={isSubmitting}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="grossAmount">
                        {(field) => (
                            <field.TextField
                                id="supplier-payables-allocation-form-gross-amount"
                                label="含税金额（元）"
                                placeholder="0.00"
                                inputMode="decimal"
                                money
                                required
                                disabled={isSubmitting}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="netAmount">
                        {(field) => (
                            <field.TextField
                                id="supplier-payables-allocation-form-net-amount"
                                label="不含税金额（元）"
                                placeholder="0.00"
                                inputMode="decimal"
                                required
                                disabled={isSubmitting}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="taxAmount">
                        {(field) => (
                            <field.TextField
                                id="supplier-payables-allocation-form-tax-amount"
                                label="税额（元）"
                                placeholder="0.00"
                                inputMode="decimal"
                                required
                                disabled={isSubmitting}
                            />
                        )}
                    </form.AppField>
                </div>
            )}
            {!session?.existingInvoiceId ? (
                <form.AppField name="invoiceFiles">
                    {(field) => (
                        <InvoiceFilesField
                            idPrefix="supplier-payables-invoice-files"
                            label="发票文件（可选）"
                            density="compact"
                            files={field.state.value}
                            disabled={isSubmitting}
                            errors={
                                field.state.meta.isTouched
                                    ? toFieldErrors(field.state.meta.errors)
                                    : []
                            }
                            description="文件随登记一并保存，可在进项发票详情下载。本机草稿不保存文件，刷新后请重新选择。"
                            onChange={(files) => {
                                field.handleChange(files)
                                field.handleBlur()
                            }}
                        />
                    )}
                </form.AppField>
            ) : null}
        </section>
    )
}
