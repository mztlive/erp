"use client"

import { z } from "zod"

import {
    PAYMENT_TERM_OPTIONS,
    WELFARE_SCENARIO_OPTIONS,
} from "@/lib/business-options"
import { toFieldErrors } from "@/components/form"
import {
    NATURE_OPTIONS,
    decimalAtMost,
    decimalInput,
    hasMeaningfulLines,
} from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { MallSearchCombobox } from "@/features/entity-selectors"
import type { SalesOrderNature } from "@/features/sales-orders/types"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"

export type SalesOrderCreateHeaderFieldsProps = {
    form: SalesOrderCreateFormApi
    /** 继续编辑 / 驳回改单：业务性质锁定，不可再改。 */
    natureLocked: boolean
    profilePending: boolean
    profileError: boolean
    applyNature: (nature: SalesOrderNature) => void
    /** 明细已有内容时先弹确认，确认后才真正切换。 */
    onNatureChangeRequest: (nature: SalesOrderNature) => void
}

export function SalesOrderCreateHeaderFields({
    form,
    natureLocked,
    profilePending,
    profileError,
    applyNature,
    onNatureChangeRequest,
}: SalesOrderCreateHeaderFieldsProps) {
    return (
        <div className="grid gap-x-4 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
            <form.AppField name="nature">
                {(field) => (
                    <field.SelectField
                        label="业务性质"
                        required
                        options={NATURE_OPTIONS}
                        disabled={natureLocked}
                        description={natureLocked ? "建单后不能改" : undefined}
                        onValueChange={(value: string) => {
                            const nature = value as SalesOrderNature
                            if (natureLocked || nature === field.state.value)
                                return
                            const lines = form.state.values.lineItems
                            if (hasMeaningfulLines(lines)) {
                                onNatureChangeRequest(nature)
                                return
                            }
                            applyNature(nature)
                        }}
                    />
                )}
            </form.AppField>
            <form.AppField name="ownerUserId">{() => null}</form.AppField>
            <form.AppField name="ownerName">
                {(field) => (
                    <field.TextField
                        label="负责销售"
                        disabled
                        placeholder={
                            profilePending
                                ? "加载当前用户…"
                                : profileError
                                  ? "无法获取登录用户"
                                  : "当前登录用户"
                        }
                        description="固定为当前登录用户，不可更改"
                    />
                )}
            </form.AppField>
            <form.AppField
                name="welfareScene"
                validators={{
                    onBlur: z
                        .string()
                        .trim()
                        .min(1, "请选择福利场景")
                        .refine(
                            (value) =>
                                WELFARE_SCENARIO_OPTIONS.some(
                                    (o) => o.value === value,
                                ),
                            "请选择有效的福利场景",
                        ),
                }}
            >
                {(field) => (
                    <field.SelectField
                        label="福利场景"
                        required
                        options={WELFARE_SCENARIO_OPTIONS}
                        placeholder="选择福利场景"
                    />
                )}
            </form.AppField>
            <form.AppField
                name="paymentTerms"
                validators={{
                    onBlur: z.string().min(1, "请选择付款条件"),
                }}
            >
                {(field) => (
                    <field.SelectField
                        label="付款条件"
                        required
                        options={PAYMENT_TERM_OPTIONS}
                    />
                )}
            </form.AppField>
            <form.Subscribe selector={(state) => state.values.nature}>
                {(nature) =>
                    nature === "card_voucher" ? (
                        <>
                            <form.AppField
                                name="fulfillmentDeadline"
                                validators={{
                                    onBlur: z.string().min(1, "请选择履约期限"),
                                }}
                            >
                                {(field) => (
                                    <field.DateField label="履约期限" required />
                                )}
                            </form.AppField>
                            <form.AppField
                                name="targetMallId"
                                validators={{
                                    onBlur: z
                                        .string()
                                        .trim()
                                        .min(1, "请选择目标商城"),
                                }}
                            >
                                {(field) => {
                                    const isInvalid =
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    const errors = toFieldErrors(
                                        field.state.meta.errors,
                                    )
                                    return (
                                        <Field
                                            data-invalid={
                                                isInvalid || undefined
                                            }
                                        >
                                            <FieldLabel htmlFor="targetMallId">
                                                目标商城
                                                <span className="text-destructive">*</span>
                                            </FieldLabel>
                                            <MallSearchCombobox
                                                purpose="form"
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(id) =>
                                                    field.handleChange(id ?? "")
                                                }
                                                onBlur={field.handleBlur}
                                                placeholder="选择目标商城"
                                                emptyLabel="暂无启用中的商城"
                                            />
                                            {isInvalid ? (
                                                <FieldError errors={errors} />
                                            ) : null}
                                        </Field>
                                    )
                                }}
                            </form.AppField>
                            <form.AppField
                                name="receivableDueDate"
                                validators={{
                                    onBlur: z
                                        .string()
                                        .min(1, "请选择应收到期日"),
                                }}
                            >
                                {(field) => (
                                    <field.DateField
                                        label="应收到期日"
                                        required
                                        description="运营通过后按此日期形成应收；该日期不能早于提交日"
                                    />
                                )}
                            </form.AppField>
                        </>
                    ) : null
                }
            </form.Subscribe>
            <form.AppField
                name="taxRatePercent"
                validators={{
                    onBlur: decimalInput("税率", 6).refine(
                        (value) => decimalAtMost(value, "100", 6),
                        "税率不能超过 100%",
                    ),
                }}
            >
                {(field) => (
                    <field.TextField
                        label="税率（%）"
                        required
                        type="number"
                        inputClassName="num"
                    />
                )}
            </form.AppField>
        </div>
    )
}
