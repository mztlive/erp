"use client"

import * as React from "react"
import { LockIcon, PlusIcon, Trash2Icon } from "lucide-react"

import { SensitiveValue, detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import { FieldGroup } from "@/components/ui/field"
import { CustomerFormDefaultFlag } from "@/features/customers/components/customer-form-default-flag"
import { FormSection } from "@/features/customers/components/customer-form-sections"
import {
    createCustomerFormDraftId,
    type CustomerFormApi,
} from "@/features/customers/components/customer-form-values"
import { revealCustomerSensitiveField } from "@/features/customers/hooks/queries"
import type { CustomerBankAccountView } from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

/**
 * 银行账户行编辑器：
 * - 已登记账户不可直接改号，在详情编辑态收口为紧凑只读行（带脱敏账号揭示、默认切换与结束账户），避免铺出 4 个空白禁用框；
 * - 新增账户展示完整录入字段。
 */
export function BankAccountRowsSection({
    form,
    mode,
    grouped,
    existingBanks = [],
}: {
    form: Pick<CustomerFormApi, "AppField" | "setFieldValue">
    mode: "create" | "edit"
    grouped: boolean
    existingBanks?: readonly CustomerBankAccountView[]
}) {
    const existingById = React.useMemo(
        () => new Map(existingBanks.map((item) => [item.id, item])),
        [existingBanks],
    )

    return (
        <FormSection
            grouped={grouped}
            title="银行账户"
            description="已登记账户不可直接修改账号，仅支持切换默认标记或结束账户后重新新增"
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
                                : "暂无银行账户，点击右上角「添加账户」录入。"}
                        </p>
                    ) : (
                        <div
                            className={
                                grouped
                                    ? "divide-y divide-border/60"
                                    : "space-y-3"
                            }
                        >
                            {field.state.value.map((row, index) => {
                                const rowKey = toAutomationIdSegment(
                                    row.existingId ??
                                        row.draftId ??
                                        `bank-${index}`,
                                )
                                const existing = row.existingId
                                    ? existingById.get(row.existingId)
                                    : undefined

                                const clearSiblingDefaults = () => {
                                    field.state.value.forEach(
                                        (item, siblingIndex) => {
                                            if (
                                                siblingIndex !== index &&
                                                item.isDefault
                                            ) {
                                                form.setFieldValue(
                                                    `bankAccounts[${siblingIndex}].isDefault`,
                                                    false,
                                                )
                                            }
                                        },
                                    )
                                }

                                if (grouped && row.existingId) {
                                    return (
                                        <div
                                            key={row.existingId}
                                            className="space-y-2 py-4 first:pt-0 last:pb-0"
                                        >
                                            <div className="flex flex-wrap items-center justify-between gap-2">
                                                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
                                                    <LockIcon
                                                        className="size-3.5 shrink-0 text-muted-foreground"
                                                        aria-hidden="true"
                                                    />
                                                    <span className="font-medium text-foreground">
                                                        {row.accountName}
                                                    </span>
                                                    <span className="text-xs text-muted-foreground">
                                                        {[
                                                            row.bankName,
                                                            row.branchName,
                                                        ]
                                                            .filter(Boolean)
                                                            .join(" · ")}
                                                    </span>
                                                </div>
                                                <div className="flex shrink-0 items-center gap-3">
                                                    <form.AppField
                                                        name={`bankAccounts[${index}].isDefault`}
                                                    >
                                                        {(nested) => (
                                                            <CustomerFormDefaultFlag
                                                                id={`customers-form-bank-${rowKey}-default`}
                                                                label="默认账户"
                                                                checked={
                                                                    nested.state
                                                                        .value
                                                                }
                                                                onCheckedChange={(
                                                                    checked,
                                                                ) => {
                                                                    if (checked)
                                                                        clearSiblingDefaults()
                                                                    nested.handleChange(
                                                                        checked,
                                                                    )
                                                                }}
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
                                                            field.removeValue(
                                                                index,
                                                            )
                                                        }
                                                    >
                                                        结束账户
                                                    </Button>
                                                </div>
                                            </div>
                                            <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 pl-5.5">
                                                <SensitiveValue
                                                    id={`customers-form-bank-${rowKey}-account-current`}
                                                    label="银行账号"
                                                    maskedValue={
                                                        existing?.accountMasked ??
                                                        "****"
                                                    }
                                                    onReveal={
                                                        existing?.accountRevealToken
                                                            ? () =>
                                                                  revealCustomerSensitiveField(
                                                                      existing.accountRevealToken!,
                                                                  )
                                                            : undefined
                                                    }
                                                />
                                                <span className="text-xs text-muted-foreground">
                                                    已登记账户仅支持调整默认状态或结束
                                                </span>
                                            </div>
                                        </div>
                                    )
                                }

                                return (
                                    <div
                                        key={
                                            row.existingId ??
                                            row.draftId ??
                                            `bank-${index}`
                                        }
                                        className={cn(
                                            "space-y-3",
                                            grouped
                                                ? "py-4 first:pt-0 last:pb-0"
                                                : "rounded-lg border border-border p-3",
                                        )}
                                    >
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <span className="text-xs font-semibold text-foreground">
                                                {row.accountName.trim() ||
                                                    `新增银行账户 ${index + 1}`}
                                            </span>
                                            <div className="flex shrink-0 items-center gap-3">
                                                <form.AppField
                                                    name={`bankAccounts[${index}].isDefault`}
                                                >
                                                    {(nested) => (
                                                        <CustomerFormDefaultFlag
                                                            id={`customers-form-bank-${rowKey}-default`}
                                                            label="默认账户"
                                                            checked={
                                                                nested.state
                                                                    .value
                                                            }
                                                            onCheckedChange={(
                                                                checked,
                                                            ) => {
                                                                if (checked)
                                                                    clearSiblingDefaults()
                                                                nested.handleChange(
                                                                    checked,
                                                                )
                                                            }}
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
                                                    <Trash2Icon
                                                        data-icon="inline-start"
                                                        aria-hidden="true"
                                                    />
                                                    {row.existingId
                                                        ? "结束账户"
                                                        : "移除"}
                                                </Button>
                                            </div>
                                        </div>

                                        <FieldGroup
                                            data-slot={
                                                grouped
                                                    ? "detail-form-grid"
                                                    : "field-group"
                                            }
                                            className={
                                                grouped
                                                    ? "grid gap-x-4 gap-y-3 sm:grid-cols-12"
                                                    : "grid gap-2 sm:grid-cols-2"
                                            }
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
                                                            row.existingId,
                                                        )}
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-6"
                                                                : undefined
                                                        }
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
                                                            row.existingId,
                                                        )}
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-6"
                                                                : undefined
                                                        }
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
                                                        placeholder="可选"
                                                        disabled={Boolean(
                                                            row.existingId,
                                                        )}
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-6"
                                                                : undefined
                                                        }
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
                                                            row.existingId,
                                                        )}
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-6"
                                                                : undefined
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                        </FieldGroup>
                                    </div>
                                )
                            })}
                        </div>
                    )
                }
            </form.AppField>
        </FormSection>
    )
}
