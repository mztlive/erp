"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import { DocumentSection } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import type { CustomerFormApi } from "@/features/customers/components/customer-form-values"

export const ADDRESS_TYPE_OPTIONS = [
    { value: "履约地址", label: "履约地址" },
    { value: "注册地址", label: "注册地址" },
    { value: "经营地址", label: "经营地址" },
] as const

export function FormSection({
    grouped,
    title,
    description,
    action,
    children,
}: {
    grouped: boolean
    title: string
    description?: string
    action?: React.ReactNode
    children: React.ReactNode
}) {
    if (!grouped) {
        return (
            <div className="space-y-2">
                <div className="flex items-center justify-between gap-2">
                    <div>
                        <p className="text-sm font-medium text-foreground">
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
        <DocumentSection
            title={title}
            description={description}
            action={action}
        >
            {children}
        </DocumentSection>
    )
}

/** 联系人行编辑器：增删行与字段编辑全部经 form.AppField 走表单状态。 */
export function ContactRowsSection({
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
            title="联系人"
            description="可多条；手机在详情页按权限打码展示"
            action={
                <form.AppField name="contacts">
                    {(field) => (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                field.pushValue({
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
                                ? "暂不填写；创建后可在客户详情「联系与地址」维护。"
                                : "暂无联系人"}
                        </p>
                    ) : (
                        field.state.value.map((_row, index) => (
                            <div
                                key={`contact-${index}`}
                                className="space-y-2 rounded-lg border border-border p-3"
                            >
                                <div className="grid gap-2 sm:grid-cols-2">
                                    <form.AppField
                                        name={`contacts[${index}].name`}
                                    >
                                        {(nested) => (
                                            <nested.TextField label="姓名" />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`contacts[${index}].title`}
                                    >
                                        {(nested) => (
                                            <nested.TextField label="职务" />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`contacts[${index}].phone`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="手机"
                                                placeholder="11 位手机号"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`contacts[${index}].telephone`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="固定电话"
                                                placeholder="可选"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`contacts[${index}].email`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="邮箱"
                                                placeholder="可选"
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <div className="flex items-center justify-between gap-2">
                                    <form.AppField
                                        name={`contacts[${index}].isDefault`}
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
                                                默认联系人
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
                                        移除
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

/** 地址行编辑器。 */
export function AddressRowsSection({
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
            title="地址"
            description="履约地址在详情页按权限打码展示"
            action={
                <form.AppField name="addresses">
                    {(field) => (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() =>
                                field.pushValue({
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
                                ? "暂不填写；创建后可在客户详情「联系与地址」维护。"
                                : "暂无地址"}
                        </p>
                    ) : (
                        field.state.value.map((_row, index) => (
                            <div
                                key={`address-${index}`}
                                className="space-y-2 rounded-lg border border-border p-3"
                            >
                                <div className="grid gap-2 sm:grid-cols-2">
                                    <form.AppField
                                        name={`addresses[${index}].addressType`}
                                    >
                                        {(nested) => (
                                            <nested.SelectField
                                                label="地址类型"
                                                options={ADDRESS_TYPE_OPTIONS}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`addresses[${index}].address`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="地址"
                                                placeholder="省市区 + 详细地址"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`addresses[${index}].contactName`}
                                    >
                                        {(nested) => (
                                            <nested.TextField
                                                label="地址联系人"
                                                placeholder="可选"
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                                <div className="flex items-center justify-between gap-2">
                                    <form.AppField
                                        name={`addresses[${index}].isDefault`}
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
                                                默认地址
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
                                        移除
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
