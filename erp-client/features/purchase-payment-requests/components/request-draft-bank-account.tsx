import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { toFieldErrors } from "@/components/form/utils"
import type { DraftForm } from "../hooks/use-draft-form"
import type { PaymentRecipient } from "../types"

export function RequestDraftBankAccount({
    form,
    index,
    id,
    accounts,
    onRefreshCandidates,
}: {
    form: DraftForm
    index: number
    id: string
    accounts: PaymentRecipient[]
    onRefreshCandidates: () => Promise<unknown>
}) {
    const [choosing, setChoosing] = useState(false)
    const focusControl = (controlId: string) => {
        requestAnimationFrame(() => document.getElementById(controlId)?.focus())
    }
    return (
        <form.AppField name={`groups[${index}].account`}>
            {(field) => {
                const account = accounts.find(
                    (item) => item.bank_account_id === field.state.value,
                )
                const invalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                if (!account || choosing)
                    return (
                        <div className="space-y-2">
                            <field.SelectField
                                id={id}
                                label="收款账户"
                                className="min-w-0 gap-2"
                                inputClassName="h-10"
                                required
                                onRefresh={onRefreshCandidates}
                                options={accounts.map((item) => ({
                                    value: item.bank_account_id,
                                    label: `${item.account_name} · ${item.bank_name} · ${item.account_number_masked}`,
                                }))}
                                emptyLabel="供应商暂无有效收款账户"
                                onValueChange={(value) => {
                                    if (value) {
                                        field.handleBlur()
                                        setChoosing(false)
                                        focusControl(`${id}-change`)
                                    }
                                }}
                            />
                            {account && (
                                <Button
                                    id={`${id}-cancel-change`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={() => {
                                        setChoosing(false)
                                        focusControl(`${id}-change`)
                                    }}
                                >
                                    保留当前账户
                                </Button>
                            )}
                        </div>
                    )
                return (
                    <Field
                        className="gap-2"
                        data-invalid={invalid || undefined}
                    >
                        <FieldLabel htmlFor={`${id}-change`}>
                            收款账户
                            <span className="text-destructive">*</span>
                        </FieldLabel>
                        <div className="flex items-start justify-between gap-3 rounded-lg border px-3 py-3">
                            <div className="min-w-0 space-y-1">
                                <p className="break-words text-sm font-medium">
                                    {account.account_name}
                                </p>
                                <p className="break-words text-xs text-muted-foreground">
                                    {account.bank_name}
                                    {account.bank_branch_name &&
                                        !account.bank_name.includes(
                                            account.bank_branch_name,
                                        ) &&
                                        ` · ${account.bank_branch_name}`}
                                    <span className="num">
                                        {" · "}
                                        {account.account_number_masked}
                                    </span>
                                </p>
                            </div>
                            <Button
                                id={`${id}-change`}
                                type="button"
                                size="sm"
                                variant="ghost"
                                className="shrink-0"
                                aria-label="更换收款账户"
                                aria-describedby={
                                    invalid ? `${id}-error` : undefined
                                }
                                onClick={() => {
                                    setChoosing(true)
                                    focusControl(id)
                                }}
                            >
                                更换
                            </Button>
                        </div>
                        {invalid && (
                            <FieldError
                                id={`${id}-error`}
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        )}
                    </Field>
                )
            }}
        </form.AppField>
    )
}
