"use client"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ContractImportTask, ConfirmContractImport } from "../api/upload"

const FIELDS = {
    contract_no: "合同编号",
    customer_name: "客户法定名称",
    customer_credit_code: "客户统一社会信用代码",
    company_name: "我方签约主体",
    company_credit_code: "我方统一社会信用代码",
    settlement_name: "结算主体",
    settlement_credit_code: "结算主体统一社会信用代码",
    payment_terms: "付款条件",
    invoice_type: "开票要求",
    tax_point: "税率（%）",
    signed_at: "签订日期",
    valid_from: "生效日期",
    valid_to: "有效期止",
    business_scope: "业务范围",
} as const

type FieldName = keyof typeof FIELDS
const PAYMENT = [
    "先款 100%",
    "先款 50%",
    "先款 30%",
    "货到 15 天",
    "货到 30 天",
]
const INVOICE = ["增值税专用发票", "增值税普通发票", "不开发票"]
const TAX = ["0", "1", "3", "6", "9", "13"]
const options = (values: string[]) =>
    values.map((value) => ({ value, label: value }))

export function ContractImportReview({
    task,
    busy,
    disabled,
    onConfirm,
}: {
    task: ContractImportTask
    busy: boolean
    disabled: boolean
    onConfirm: (command: ConfirmContractImport) => Promise<unknown>
}) {
    const defaults = Object.fromEntries(
        Object.keys(FIELDS).map((key) => [key, task.draft?.fields[key] ?? ""]),
    ) as Record<FieldName, string>
    const form = useAppForm({
        defaultValues: defaults,
        onSubmit: async ({ value }) => {
            if (busy || disabled) return
            await onConfirm({
                version: task.version,
                fields: Object.fromEntries(
                    Object.entries(value).map(([key, text]) => [
                        key,
                        text.trim() || null,
                    ]),
                ),
            })
        },
    })
    return (
        <form
            id="contract-import-review-form"
            className="space-y-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit().catch(() => undefined)
            }}
        >
            {task.draft?.warnings.length ? (
                <Alert>
                    <AlertTitle>以下内容需要核对</AlertTitle>
                    <AlertDescription>
                        <ul className="list-disc space-y-2 pl-4">
                            {task.draft.warnings.map((warning) => (
                                <li key={warning}>
                                    {Object.entries(FIELDS).reduce(
                                        (text, [key, label]) =>
                                            text.replaceAll(key, label),
                                        warning,
                                    )}
                                </li>
                            ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : null}
            <div className="grid gap-4 sm:grid-cols-2">
                {(Object.entries(FIELDS) as [FieldName, string][]).map(
                    ([name, label]) => {
                        const required = !name.endsWith("_credit_code")
                        const choices =
                            name === "payment_terms"
                                ? PAYMENT
                                : name === "invoice_type"
                                  ? INVOICE
                                  : name === "tax_point"
                                    ? TAX
                                    : null
                        return (
                            <form.AppField
                                key={name}
                                name={name}
                                validators={{
                                    onChange: ({ value }) =>
                                        required && !value.trim()
                                            ? `请填写${label}`
                                            : undefined,
                                }}
                            >
                                {(field) => {
                                    const props = {
                                        id: `contract-import-edit-${name.replaceAll("_", "-")}`,
                                        label,
                                        required,
                                        disabled: busy || disabled,
                                    }
                                    if (choices)
                                        return (
                                            <field.SelectField
                                                {...props}
                                                options={options(choices)}
                                            />
                                        )
                                    if (
                                        name === "signed_at" ||
                                        name === "valid_from"
                                    )
                                        return <field.DateField {...props} />
                                    if (name === "business_scope")
                                        return (
                                            <field.TextareaField
                                                {...props}
                                                maxLength={4096}
                                            />
                                        )
                                    return (
                                        <field.TextField
                                            {...props}
                                            placeholder={
                                                name === "valid_to"
                                                    ? "YYYY-MM-DD 或长期"
                                                    : "请核对或补充"
                                            }
                                        />
                                    )
                                }}
                            </form.AppField>
                        )
                    },
                )}
            </div>
            <p className="text-xs text-muted-foreground">
                主体请填写系统已有的法定名称；缺失信息可对照原文补充。
            </p>
            <form.AppForm>
                <form.SubmitButton
                    id="contract-import-confirm"
                    label="确认并归档"
                    loading={busy}
                    disabled={disabled}
                />
            </form.AppForm>
        </form>
    )
}
