"use client"

import { LockIcon, PlusIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { FieldGroup } from "@/components/ui/field"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"
import { CustomerFormDefaultFlag } from "@/features/customers/components/customer-form-default-flag"
import {
    createCustomerFormDraftId,
    type CustomerFormApi,
} from "@/features/customers/components/customer-form-values"
import { FormSection } from "@/features/customers/components/customer-form-sections"
import { toAutomationIdSegment } from "@/lib/automation-id"

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
                            id="customers-form-bank-add"
                            type="button"
                            size="sm"
                            variant={grouped ? "ghost" : "outline"}
                            className={
                                grouped
                                    ? detailWorkspaceStyles.subtleButton
                                    : undefined
                            }
                            onClick={() =>
                                field.pushValue({
                                    draftId: createCustomerFormDraftId("bank"),
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
                        field.state.value.map((_row, index) => {
                            const rowKey = toAutomationIdSegment(
                                _row.existingId ??
                                    _row.draftId ??
                                    `bank-${index}`,
                            )
                            return (
                                <div
                                    key={
                                        _row.existingId ??
                                        _row.draftId ??
                                        `bank-${index}`
                                    }
                                    className={cn(
                                        "space-y-2",
                                        grouped
                                            ? "border-t border-border/60 py-3 first:border-t-0 first:pt-0"
                                            : "rounded-lg border border-border p-3",
                                    )}
                                >
                                    {grouped && _row.existingId ? (
                                        <div className="mb-2 inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                                            <LockIcon
                                                className="size-3"
                                                aria-hidden="true"
                                            />
                                            已登记的账户资料不可直接修改
                                        </div>
                                    ) : null}
                                    <FieldGroup
                                        data-slot={
                                            grouped
                                                ? "detail-form-grid"
                                                : "field-group"
                                        }
                                        className={cn(
                                            "grid gap-2 sm:grid-cols-2",
                                            grouped &&
                                                _row.existingId &&
                                                "[&_[data-slot=input]:disabled]:border-transparent [&_[data-slot=input]:disabled]:bg-muted/45",
                                        )}
                                    >
                                        <form.AppField
                                            name={`bankAccounts[${index}].accountName`}
                                        >
                                            {(nested) => (
                                                <nested.TextField
                                                    id={`customers-form-bank-${rowKey}-account-name`}
                                                    label="户名"
                                                    required
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
                                                    id={`customers-form-bank-${rowKey}-bank-name`}
                                                    label="银行名称"
                                                    required
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
                                                    id={`customers-form-bank-${rowKey}-branch-name`}
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
                                                    id={`customers-form-bank-${rowKey}-account-number`}
                                                    label="账号"
                                                    required
                                                    inputClassName="num"
                                                    disabled={Boolean(
                                                        _row.existingId,
                                                    )}
                                                />
                                            )}
                                        </form.AppField>
                                    </FieldGroup>
                                    <div className="flex items-center justify-between gap-2">
                                        <form.AppField
                                            name={`bankAccounts[${index}].isDefault`}
                                        >
                                            {(nested) => (
                                                <CustomerFormDefaultFlag
                                                    id={`customers-form-bank-${rowKey}-default`}
                                                    label="默认账户"
                                                    checked={nested.state.value}
                                                    onCheckedChange={
                                                        nested.handleChange
                                                    }
                                                />
                                            )}
                                        </form.AppField>
                                        <Button
                                            id={`customers-form-bank-${rowKey}-remove`}
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.quietDangerButton
                                            }
                                            onClick={() =>
                                                field.removeValue(index)
                                            }
                                        >
                                            {_row.existingId
                                                ? "结束账户"
                                                : "移除"}
                                        </Button>
                                    </div>
                                </div>
                            )
                        })
                    )
                }
            </form.AppField>
        </FormSection>
    )
}
