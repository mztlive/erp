"use client"

import { z } from "zod"

import {
    PAYMENT_TERM_OPTIONS,
    WELFARE_SCENARIO_OPTIONS,
} from "@/lib/business-options"
import {
    NATURE_OPTIONS,
    decimalAtMost,
    decimalInput,
    hasMeaningfulLines,
} from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import type { SalesOrderNature } from "@/features/sales-orders/types"

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
        <>
            <section
                className="space-y-4 border-b border-grid pb-6"
                aria-labelledby="sales-create-business-title"
            >
                <h2
                    id="sales-create-business-title"
                    className="font-heading text-base font-semibold"
                >
                    业务信息
                </h2>
                <div className="grid items-start gap-5 sm:grid-cols-2 lg:grid-cols-3">
                    <form.AppField name="nature">
                        {(field) => {
                            const previousNature = field.state.value
                            return (
                                <field.SelectField
                                    id="sales-orders-create-header-nature"
                                    label="业务性质"
                                    required
                                    options={NATURE_OPTIONS}
                                    allowClear={false}
                                    disabled={natureLocked}
                                    description={
                                        natureLocked
                                            ? "建单后不能改"
                                            : undefined
                                    }
                                    onValueChange={(value: string) => {
                                        const nature = value as SalesOrderNature
                                        if (
                                            natureLocked ||
                                            nature === previousNature
                                        )
                                            return
                                        const lines =
                                            form.state.values.lineItems
                                        if (hasMeaningfulLines(lines)) {
                                            form.setFieldValue(
                                                "nature",
                                                previousNature,
                                            )
                                            onNatureChangeRequest(nature)
                                            return
                                        }
                                        applyNature(nature)
                                    }}
                                />
                            )
                        }}
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
                                id="sales-orders-create-header-welfare-scene"
                                label="福利场景"
                                required
                                options={WELFARE_SCENARIO_OPTIONS}
                                placeholder="选择福利场景"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="ownerUserId">
                        {() => null}
                    </form.AppField>
                    <form.AppField name="ownerName">
                        {(field) => (
                            <div
                                id="sales-orders-create-header-owner-name"
                                tabIndex={-1}
                                className="space-y-3 text-sm"
                            >
                                <div className="font-medium">负责销售</div>
                                <p
                                    className="flex min-h-9 flex-wrap items-center gap-x-2 gap-y-1"
                                    aria-live="polite"
                                >
                                    <span>
                                        {profilePending
                                            ? "加载当前用户…"
                                            : profileError
                                              ? "无法获取登录用户"
                                              : field.state.value ||
                                                "当前用户未就绪"}
                                    </span>
                                    {!profilePending &&
                                    !profileError &&
                                    field.state.value ? (
                                        <span className="text-xs text-muted-foreground">
                                            当前用户 · 自动指定
                                        </span>
                                    ) : null}
                                </p>
                            </div>
                        )}
                    </form.AppField>
                </div>
            </section>
            <section
                className="space-y-4 border-b border-grid pb-6"
                aria-labelledby="sales-create-terms-title"
            >
                <h2
                    id="sales-create-terms-title"
                    className="font-heading text-base font-semibold"
                >
                    交易条件
                </h2>
                <div className="grid items-start gap-5 sm:grid-cols-2 lg:grid-cols-3">
                    <form.AppField
                        name="paymentTerms"
                        validators={{
                            onBlur: z.string().min(1, "请选择付款条件"),
                        }}
                    >
                        {(field) => (
                            <field.SelectField
                                id="sales-orders-create-header-payment-terms"
                                label="付款条件"
                                required
                                options={PAYMENT_TERM_OPTIONS}
                                description="选择合同时带入默认付款条件，可按本单约定调整"
                            />
                        )}
                    </form.AppField>
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
                                id="sales-orders-create-header-tax-rate"
                                label="税率（%）"
                                required
                                type="number"
                                inputClassName="num"
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
                                            onBlur: z
                                                .string()
                                                .min(1, "请选择履约期限"),
                                        }}
                                    >
                                        {(field) => (
                                            <field.DateField
                                                id="sales-orders-create-header-fulfillment-deadline"
                                                label="履约期限"
                                                required
                                            />
                                        )}
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
                                                id="sales-orders-create-header-receivable-due-date"
                                                label="应收到期日"
                                                required
                                                description="审批生效后按此日期形成应收；该日期不能早于提交日"
                                            />
                                        )}
                                    </form.AppField>
                                </>
                            ) : null
                        }
                    </form.Subscribe>
                </div>
            </section>
        </>
    )
}
