"use client"

import { InvoiceFilesField } from "@/components/business/invoice-files-field"
import { toFieldErrors } from "@/components/form"
import { ReceiptFiles } from "./receipt-files"
import type { useAllocationSession } from "@/features/customer-receivables/hooks/use-allocation-session"

/** 回款登记到账事实与凭证录入表单。 */
export function ReceiptSessionFields({
    form,
    existing,
    locked,
    receiptId,
    unsavedFileCount = 0,
}: {
    form: ReturnType<typeof useAllocationSession>["form"]
    existing: boolean
    locked: boolean
    receiptId?: string
    unsavedFileCount?: number
}) {
    return (
        <section
            aria-labelledby="customer-receivables-session-fact-title"
            className="space-y-4"
        >
            <h2
                id="customer-receivables-session-fact-title"
                tabIndex={-1}
                className="sr-only"
            >
                填写到账信息
            </h2>
            <div className="flex flex-col gap-4">
                <form.AppField
                    name="amount"
                    children={(field) => (
                        <field.TextField
                            id="customer-receivables-session-amount"
                            money
                            label={
                                existing
                                    ? "原单到账金额（含税）"
                                    : "到账金额（含税）"
                            }
                            required
                            inputMode="decimal"
                            inputClassName="num h-control-lg text-base font-semibold"
                            placeholder="请输入到账金额"
                            disabled={locked}
                        />
                    )}
                />
                <form.AppField
                    name="receivedAt"
                    children={(field) => (
                        <field.DateTimeField
                            id="customer-receivables-session-received-at"
                            label="到账时间（北京时间）"
                            required
                            clearable={false}
                            timeZone="Asia/Shanghai"
                            showTimeZone={false}
                            disabled={locked}
                        />
                    )}
                />
                <form.AppField
                    name="bankReference"
                    children={(field) => (
                        <field.TextField
                            id="customer-receivables-session-bank-reference"
                            label="银行流水 / 回单号（选填）"
                            placeholder="填写银行流水或回单引用"
                            inputClassName="h-control-lg"
                            disabled={locked}
                        />
                    )}
                />
            </div>
            <form.AppField
                name="receiptFiles"
                validators={{
                    onChange: ({ value }) => {
                        if (
                            value.length === 0 &&
                            (!existing || (unsavedFileCount > 0 && !receiptId))
                        ) {
                            return "请上传回款凭证"
                        }
                        return undefined
                    },
                }}
            >
                {(field) => (
                    <InvoiceFilesField
                        idPrefix="customer-receivables-receipt-files"
                        label="回款凭证"
                        required
                        uploadLabel="上传回款凭证"
                        fileKind="回款凭证"
                        density="compact"
                        files={field.state.value}
                        disabled={locked}
                        errors={
                            field.state.meta.isTouched ||
                            field.form.state.isSubmitted
                                ? toFieldErrors(field.state.meta.errors)
                                : []
                        }
                        onChange={(files) => {
                            field.handleChange(files)
                            field.handleBlur()
                        }}
                    />
                )}
            </form.AppField>
            {unsavedFileCount > 0 &&
            form.state.values.receiptFiles.length === 0 ? (
                <p role="status" className="text-xs text-muted-foreground">
                    原选 {unsavedFileCount}{" "}
                    个凭证文件尚未保存，请重新选择；若上次保存结果待确认，请先查询最终结果。
                </p>
            ) : null}
            {receiptId ? (
                <div className="space-y-2">
                    <h3 className="text-sm font-medium">已保存凭证</h3>
                    <ReceiptFiles receiptId={receiptId} draft />
                </div>
            ) : null}
        </section>
    )
}
