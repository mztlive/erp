"use client"

import { PlusIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import type { CustomerFormApi } from "@/features/customers/components/customer-form-values"
import { FormSection } from "@/features/customers/components/customer-form-sections"

/** 银行账户行编辑器：已有账户锁定名称字段，移除按「结束账户」语义处理。 */
export function BankAccountRowsSection({
    form,
    mode,
    grouped,
}: {
    form: Pick<CustomerFormApi, "AppField">
    mode: "create" | "edit"
    grouped: boolean
}) {
    return (
        <FormSection
            grouped={grouped}
            title="银行账户"
            description="账号默认只显示末四位；完整显示需授权，操作会留记录"
            action={
                <form.AppField name="bankAccounts">
                    {(field) => (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                field.pushValue({
                                    accountName: "",
                                    bankName: "",
                                    branchName: "",
                                    accountNumber: "",
                                    isDefault: field.state.value.length === 0,
                                })
                            }
                        >
                            <PlusIcon aria-hidden="true" />
                            添加账户
                        </Button>
                    )}
                </form.AppField>
            }
        >
            <form.AppField name="bankAccounts">
                {(field) =>
                    field.state.value.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            {mode === "create"
                                ? "暂不填写；创建后可由授权财务维护。"
                                : "暂无银行账户"}
                        </p>
                    ) : (
                        field.state.value.map((_row, index) => (
                            <div
                                key={`bank-${index}`}
                                className="space-y-2 rounded-lg border border-border p-3"
                            >
                                <div className="grid gap-2 sm:grid-cols-2">
                                    <form.AppField
                                        name={`bankAccounts[${index}].accountName`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="户名"
                                                disabled={Boolean(
                                                    _row.existingId,
                                                )}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`bankAccounts[${index}].bankName`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="银行名称"
                                                disabled={Boolean(
                                                    _row.existingId,
                                                )}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`bankAccounts[${index}].branchName`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="支行名称"
                                                disabled={Boolean(
                                                    _row.existingId,
                                                )}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`bankAccounts[${index}].accountNumber`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="账号"
                                                disabled={Boolean(
                                                    _row.existingId,
                                                )}
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <div className="flex items-center justify-between gap-2">
                                    <form.AppField
                                        name={`bankAccounts[${index}].isDefault`}
                                    >
                                        {(nested) => (
                                            <label className="flex items-center gap-2 text-sm">
                                                <Checkbox
                                                    checked={
                                                        nested.state.value
                                                    }
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        nested.handleChange(
                                                            checked === true,
                                                        )
                                                    }
                                                />
                                                默认账户
                                            </label>
                                        )}
                                    </form.AppField>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        onClick={() =>
                                            field.removeValue(index)
                                        }
                                    >
                                        {_row.existingId ? "结束账户" : "移除"}
                                    </Button>
                                </div>
                            </div>
                        ))
                    )
                }
            </form.AppField>
        </FormSection>
    )
}
