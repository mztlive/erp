"use client"

import * as React from "react"
import { PlusIcon, Trash2Icon } from "lucide-react"

import {
    DetailWorkspaceSection,
    SensitiveValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { FieldGroup } from "@/components/ui/field"
import { CustomerFormDefaultFlag } from "@/features/customers/components/customer-form-default-flag"
import {
    createCustomerFormDraftId,
    type CustomerFormApi,
} from "@/features/customers/components/customer-form-values"
import { revealCustomerSensitiveField } from "@/features/customers/hooks/queries"
import type {
    CustomerAddressView,
    CustomerContactView,
} from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export const ADDRESS_TYPE_OPTIONS = [
    { value: "履约地址", label: "履约地址" },
    { value: "注册地址", label: "注册地址" },
    { value: "经营地址", label: "经营地址" },
] as const

export function FormSection({
    grouped,
    title,
    badge,
    description,
    action,
    children,
}: {
    grouped: boolean
    title: string
    badge?: React.ReactNode
    description?: string
    action?: React.ReactNode
    children: React.ReactNode
}) {
    if (!grouped) {
        return (
            <div className="space-y-2">
                <div className="flex items-center justify-between gap-2">
                    <div>
                        <p className="text-body-compact font-medium text-foreground">
                            {title}
                        </p>
                        {description ? (
                            <p className="text-xs text-muted-foreground">
                                {description}
                            </p>
                        ) : null}
                    </div>
                    {action}
                </div>
                {children}
            </div>
        )
    }
    return (
        <DetailWorkspaceSection
            title={title}
            badge={badge}
            subtitle={description}
            actions={action}
            width="form"
        >
            {children}
        </DetailWorkspaceSection>
    )
}

/** 联系人行编辑器：增删行与字段编辑全部经 form.AppField 走表单状态。 */
export function ContactRowsSection({
    form,
    mode,
    grouped,
    existingContacts = [],
}: {
    form: Pick<CustomerFormApi, "AppField" | "setFieldValue">
    mode: "create" | "edit"
    grouped: boolean
    existingContacts?: readonly CustomerContactView[]
}) {
    const existingById = React.useMemo(
        () => new Map(existingContacts.map((item) => [item.id, item])),
        [existingContacts],
    )

    return (
        <FormSection
            grouped={grouped}
            title="联系人"
            description="可维护多位联系人；已有联系人手机号留空则保留原号码"
            action={
                <form.AppField name="contacts">
                    {(field) => (
                        <Button
                            id="customers-form-contacts-add"
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
                                    draftId:
                                        createCustomerFormDraftId("contact"),
                                    name: "",
                                    title: "",
                                    phone: "",
                                    telephone: "",
                                    email: "",
                                    isDefault: field.state.value.length === 0,
                                })
                            }
                        >
                            <PlusIcon aria-hidden="true" />
                            添加联系人
                        </Button>
                    )}
                </form.AppField>
            }
        >
            <form.AppField name="contacts">
                {(field) =>
                    field.state.value.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            {mode === "create"
                                ? "暂不填写；创建后可在客户详情维护。"
                                : "暂无联系人，点击右上角「添加联系人」录入。"}
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
                                        `contact-${index}`,
                                )
                                const existing = row.existingId
                                    ? existingById.get(row.existingId)
                                    : undefined
                                return (
                                    <div
                                        key={
                                            row.existingId ??
                                            row.draftId ??
                                            `contact-${index}`
                                        }
                                        className={cn(
                                            "space-y-3",
                                            grouped
                                                ? "py-4 first:pt-0 last:pb-0"
                                                : "rounded-lg border border-border p-3",
                                        )}
                                    >
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
                                                <span className="text-xs font-semibold text-foreground">
                                                    {row.name.trim() ||
                                                        `联系人 ${index + 1}`}
                                                </span>
                                                {grouped && existing ? (
                                                    <span className="inline-flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                                                        <span>当前手机</span>
                                                        <SensitiveValue
                                                            id={`customers-form-contacts-${rowKey}-phone-current`}
                                                            label={`${existing.name}当前手机`}
                                                            maskedValue={
                                                                existing.phoneMasked
                                                            }
                                                            onReveal={
                                                                existing.phoneRevealToken
                                                                    ? () =>
                                                                          revealCustomerSensitiveField(
                                                                              existing.phoneRevealToken!,
                                                                          )
                                                                    : undefined
                                                            }
                                                        />
                                                    </span>
                                                ) : null}
                                            </div>
                                            <div className="flex shrink-0 items-center gap-3">
                                                <form.AppField
                                                    name={`contacts[${index}].isDefault`}
                                                >
                                                    {(nested) => (
                                                        <CustomerFormDefaultFlag
                                                            id={`customers-form-contacts-${rowKey}-default`}
                                                            label="默认联系人"
                                                            checked={
                                                                nested.state
                                                                    .value
                                                            }
                                                            onCheckedChange={(
                                                                checked,
                                                            ) => {
                                                                if (checked) {
                                                                    field.state.value.forEach(
                                                                        (
                                                                            item,
                                                                            siblingIndex,
                                                                        ) => {
                                                                            if (
                                                                                siblingIndex !==
                                                                                    index &&
                                                                                item.isDefault
                                                                            ) {
                                                                                form.setFieldValue(
                                                                                    `contacts[${siblingIndex}].isDefault`,
                                                                                    false,
                                                                                )
                                                                            }
                                                                        },
                                                                    )
                                                                }
                                                                nested.handleChange(
                                                                    checked,
                                                                )
                                                            }}
                                                        />
                                                    )}
                                                </form.AppField>
                                                <Button
                                                    id={`customers-form-contacts-${rowKey}-remove`}
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
                                                    移除
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
                                                name={`contacts[${index}].name`}
                                            >
                                                {(nested) => (
                                                    <nested.TextField
                                                        id={`customers-form-contacts-${rowKey}-name`}
                                                        label="姓名"
                                                        required
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-4"
                                                                : undefined
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`contacts[${index}].title`}
                                            >
                                                {(nested) => (
                                                    <nested.TextField
                                                        id={`customers-form-contacts-${rowKey}-title`}
                                                        label="职务"
                                                        placeholder="可选，如采购经理"
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-4"
                                                                : undefined
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`contacts[${index}].phone`}
                                            >
                                                {(nested) => (
                                                    <nested.TextField
                                                        id={`customers-form-contacts-${rowKey}-phone`}
                                                        label={
                                                            row.existingId
                                                                ? "更换手机"
                                                                : "手机"
                                                        }
                                                        required={
                                                            !row.existingId
                                                        }
                                                        inputClassName="num"
                                                        placeholder={
                                                            row.existingId
                                                                ? "留空保留原号码"
                                                                : "11 位手机号"
                                                        }
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-4"
                                                                : undefined
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`contacts[${index}].telephone`}
                                            >
                                                {(nested) => (
                                                    <nested.TextField
                                                        id={`customers-form-contacts-${rowKey}-telephone`}
                                                        label="固定电话"
                                                        inputClassName="num"
                                                        placeholder="可选"
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-4"
                                                                : undefined
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`contacts[${index}].email`}
                                            >
                                                {(nested) => (
                                                    <nested.TextField
                                                        id={`customers-form-contacts-${rowKey}-email`}
                                                        label="邮箱"
                                                        placeholder="可选"
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-8"
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

/** 地址行编辑器。 */
export function AddressRowsSection({
    form,
    mode,
    grouped,
    existingAddresses = [],
}: {
    form: Pick<CustomerFormApi, "AppField" | "setFieldValue">
    mode: "create" | "edit"
    grouped: boolean
    existingAddresses?: readonly CustomerAddressView[]
}) {
    const existingById = React.useMemo(
        () => new Map(existingAddresses.map((item) => [item.id, item])),
        [existingAddresses],
    )

    return (
        <FormSection
            grouped={grouped}
            title="地址"
            description="支持登记注册、经营与履约收货地址；已有地址留空则保留原地址"
            action={
                <form.AppField name="addresses">
                    {(field) => (
                        <Button
                            id="customers-form-addresses-add"
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
                                    draftId:
                                        createCustomerFormDraftId("address"),
                                    addressType: "履约地址",
                                    contactName: "",
                                    address: "",
                                    isDefault: field.state.value.length === 0,
                                })
                            }
                        >
                            <PlusIcon aria-hidden="true" />
                            添加地址
                        </Button>
                    )}
                </form.AppField>
            }
        >
            <form.AppField name="addresses">
                {(field) =>
                    field.state.value.length === 0 ? (
                        <p className="text-xs text-muted-foreground">
                            {mode === "create"
                                ? "暂不填写；创建后可在客户详情维护。"
                                : "暂无地址，点击右上角「添加地址」录入。"}
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
                                        `address-${index}`,
                                )
                                const existing = row.existingId
                                    ? existingById.get(row.existingId)
                                    : undefined
                                return (
                                    <div
                                        key={
                                            row.existingId ??
                                            row.draftId ??
                                            `address-${index}`
                                        }
                                        className={cn(
                                            "space-y-3",
                                            grouped
                                                ? "py-4 first:pt-0 last:pb-0"
                                                : "rounded-lg border border-border p-3",
                                        )}
                                    >
                                        <div className="flex flex-wrap items-center justify-between gap-2">
                                            <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1">
                                                <span className="text-xs font-semibold text-foreground">
                                                    {row.addressType ||
                                                        `地址 ${index + 1}`}
                                                </span>
                                                {grouped && existing ? (
                                                    <span className="inline-flex flex-wrap items-center gap-1.5 text-xs text-muted-foreground">
                                                        <span>当前地址</span>
                                                        <SensitiveValue
                                                            id={`customers-form-addresses-${rowKey}-address-current`}
                                                            label={`当前${existing.addressType}`}
                                                            maskedValue={
                                                                existing.addressMasked
                                                            }
                                                            onReveal={
                                                                existing.addressRevealToken
                                                                    ? () =>
                                                                          revealCustomerSensitiveField(
                                                                              existing.addressRevealToken!,
                                                                          )
                                                                    : undefined
                                                            }
                                                        />
                                                    </span>
                                                ) : null}
                                            </div>
                                            <div className="flex shrink-0 items-center gap-3">
                                                <form.AppField
                                                    name={`addresses[${index}].isDefault`}
                                                >
                                                    {(nested) => (
                                                        <CustomerFormDefaultFlag
                                                            id={`customers-form-addresses-${rowKey}-default`}
                                                            label="默认地址"
                                                            checked={
                                                                nested.state
                                                                    .value
                                                            }
                                                            onCheckedChange={(
                                                                checked,
                                                            ) => {
                                                                if (checked) {
                                                                    field.state.value.forEach(
                                                                        (
                                                                            item,
                                                                            siblingIndex,
                                                                        ) => {
                                                                            if (
                                                                                siblingIndex !==
                                                                                    index &&
                                                                                item.isDefault
                                                                            ) {
                                                                                form.setFieldValue(
                                                                                    `addresses[${siblingIndex}].isDefault`,
                                                                                    false,
                                                                                )
                                                                            }
                                                                        },
                                                                    )
                                                                }
                                                                nested.handleChange(
                                                                    checked,
                                                                )
                                                            }}
                                                        />
                                                    )}
                                                </form.AppField>
                                                <Button
                                                    id={`customers-form-addresses-${rowKey}-remove`}
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
                                                    移除
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
                                                name={`addresses[${index}].addressType`}
                                            >
                                                {(nested) => (
                                                    <nested.SelectField
                                                        id={`customers-form-addresses-${rowKey}-type`}
                                                        label="地址类型"
                                                        required
                                                        options={
                                                            ADDRESS_TYPE_OPTIONS
                                                        }
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-6"
                                                                : undefined
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`addresses[${index}].contactName`}
                                            >
                                                {(nested) => (
                                                    <nested.TextField
                                                        id={`customers-form-addresses-${rowKey}-contact-name`}
                                                        label="地址联系人"
                                                        placeholder="可选"
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-6"
                                                                : undefined
                                                        }
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`addresses[${index}].address`}
                                            >
                                                {(nested) => (
                                                    <nested.TextField
                                                        id={`customers-form-addresses-${rowKey}-address`}
                                                        label={
                                                            row.existingId
                                                                ? "更换地址"
                                                                : "地址"
                                                        }
                                                        required={
                                                            !row.existingId
                                                        }
                                                        placeholder={
                                                            row.existingId
                                                                ? "留空保留原地址，输入新地址将替换"
                                                                : "省市区 + 详细地址"
                                                        }
                                                        className={
                                                            grouped
                                                                ? "sm:col-span-12"
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
