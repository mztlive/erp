"use client"

import { InvoiceFilesField } from "@/components/business/invoice-files-field"
import { toFieldErrors } from "@/components/form"
import { ReceiptFiles } from "./receipt-files"
import type { useAllocationSession } from "@/features/customer-receivables/hooks/use-allocation-session"

/** 回款登记的平面字段区；结算主体和已有回款信息保持只读。 */
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
            className="space-y-4 border-b border-border pb-6"
        >
            <h2
                id="customer-receivables-session-fact-title"
                className="text-base font-semibold"
            >
                到账信息
            </h2>
            <div className="grid items-start gap-x-6 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
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
                            inputClassName="num"
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
                            label="实际到账时间"
                            required
                            timeZone="Asia/Shanghai"
                            showTimeZone={false}
                            description="北京时间（UTC+8）"
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
                            disabled={locked}
                        />
                    )}
                />
            </div>
            <form.AppField name="receiptFiles">
                {(field) => (
                    <InvoiceFilesField
                        idPrefix="customer-receivables-receipt-files"
                        label="回款凭证（可选）"
                        uploadLabel="添加回款凭证"
                        fileKind="回款凭证"
                        density="compact"
                        files={field.state.value}
                        disabled={locked}
                        errors={
                            field.state.meta.isTouched
                                ? toFieldErrors(field.state.meta.errors)
                                : []
                        }
                        description="保存草稿或提交审批时一并上传。仅暂存本页不会保存新选文件，刷新后需重新选择。"
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
