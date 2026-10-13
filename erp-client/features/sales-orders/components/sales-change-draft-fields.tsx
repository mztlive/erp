"use client"
import { MoneyValue } from "@/components/business"
import { PAYMENT_TERM_OPTIONS, paymentTermLabel } from "@/lib/business-options"
import { compareDecimal, subtractFixed, sumFixed } from "@/lib/fixed-decimal"
import { SalesChangeDeliverySection } from "./sales-change-delivery-section"
import { SalesChangeLineItems } from "./sales-change-line-items"
import { ChangeOrderSection } from "@/components/business/change-order-section"
import { salesChangeLineAmount } from "../lib/sales-change-draft"
import type { SalesChangeDraftFormApi } from "../lib/sales-change-draft-form-types"
import type { SalesChangeDraft } from "../api/sales-change-draft"

/** 改单正文：商品明细 → 交付安排 → 商业条款 → 变更说明。 */
export function SalesChangeDraftFields({
    form,
    id,
    draft,
    changeOrderId,
    disabled,
    onError,
}: {
    form: SalesChangeDraftFormApi
    id: string
    draft: SalesChangeDraft
    changeOrderId: string
    disabled: boolean
    onError: (message: string) => void
}) {
    return (
        <div className="space-y-6">
            <SalesChangeLineItems
                form={form}
                id={id}
                draft={draft}
                changeOrderId={changeOrderId}
                disabled={disabled}
                onError={onError}
            />
            <SalesChangeDeliverySection
                form={form}
                id={id}
                disabled={disabled}
            />
            <ChangeOrderSection
                id={`${id}-terms`}
                title="商业条款"
                description="随本次改单一并审批；不需要调整时保持原值。"
            >
                <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                    <form.AppField name="terms.project_name">
                        {(field) => (
                            <field.TextField
                                id={`${id}-project`}
                                label="项目名称"
                                disabled={disabled}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="terms.payment_term_code">
                        {(field) => (
                            <field.SelectField
                                id={`${id}-payment`}
                                label="付款条件"
                                required
                                disabled={disabled}
                                allowClear={false}
                                options={
                                    PAYMENT_TERM_OPTIONS.some(
                                        (option) =>
                                            option.value ===
                                            draft.terms.payment_term_code,
                                    )
                                        ? PAYMENT_TERM_OPTIONS
                                        : [
                                              ...PAYMENT_TERM_OPTIONS,
                                              {
                                                  value: draft.terms
                                                      .payment_term_code,
                                                  label: draft.terms
                                                      .payment_term_name,
                                              },
                                          ]
                                }
                                onValueChange={(value: string) =>
                                    form.setFieldValue(
                                        "terms.payment_term_name",
                                        value === draft.terms.payment_term_code
                                            ? draft.terms.payment_term_name
                                            : paymentTermLabel(value),
                                    )
                                }
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="terms.invoice_type">
                        {(field) => (
                            <field.SelectField
                                id={`${id}-invoice`}
                                label="开票要求"
                                required
                                disabled={disabled}
                                allowClear={false}
                                options={Array.from(
                                    new Set([
                                        draft.terms.invoice_type,
                                        "增值税专用发票",
                                        "增值税普通发票",
                                        "不开发票",
                                    ]),
                                ).map((value) => ({ value, label: value }))}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="terms.tax_point">
                        {(field) => (
                            <field.TextField
                                id={`${id}-tax-point`}
                                label="约定税点（%）"
                                required
                                disabled={disabled}
                                inputClassName="num"
                            />
                        )}
                    </form.AppField>
                </div>
            </ChangeOrderSection>
            <ChangeOrderSection
                id={`${id}-explain`}
                title="变更说明"
                description="变更原因会随审批流转，审批人据此判断。"
            >
                <div className="grid gap-4 sm:grid-cols-2">
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id={`${id}-reason`}
                                label="变更原因"
                                placeholder="例如：客户追加福利数量，调整成交单价"
                                required
                                disabled={disabled}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="remark">
                        {(field) => (
                            <field.TextareaField
                                id={`${id}-remark`}
                                label="业务备注"
                                placeholder="选填"
                                disabled={disabled}
                            />
                        )}
                    </form.AppField>
                </div>
            </ChangeOrderSection>
        </div>
    )
}

/** 页脚金额：变更后总额与相对当前生效版本的调整额。 */
export function SalesChangeTotalSummary({
    form,
    baseAmount,
}: {
    form: SalesChangeDraftFormApi
    baseAmount: string
}) {
    return (
        <form.Subscribe selector={(state) => state.values.lines}>
            {(lines) => {
                let total: string | null = null
                let delta: string | null = null
                try {
                    total = sumFixed(lines.map(salesChangeLineAmount), {
                        maxScale: 2,
                        outputScale: 2,
                    })
                    delta = subtractFixed(total, baseAmount, {
                        maxScale: 2,
                        outputScale: 2,
                    })
                } catch {
                    /* 字段错误会标出未填写完整的行。 */
                }
                const direction =
                    delta === null ? null : compareDecimal(delta, "0", 2)
                return (
                    <div className="min-w-0 space-y-0.5" aria-live="polite">
                        <div className="flex flex-wrap items-baseline gap-x-3">
                            <span className="text-muted-foreground">
                                变更后含税总额
                            </span>
                            <MoneyValue
                                value={total}
                                size="section"
                                unavailableReason={
                                    total === null ? "待填写完整" : undefined
                                }
                            />
                        </div>
                        <p className="text-xs text-muted-foreground">
                            当前生效 <MoneyValue value={baseAmount} />
                            {direction === null ? null : (
                                <>
                                    <span className="mx-1.5">·</span>
                                    {direction === 0 ? (
                                        "金额未变"
                                    ) : (
                                        <>
                                            {direction > 0 ? "增加 " : "减少 "}
                                            <MoneyValue
                                                value={delta!.replace(/^-/, "")}
                                                className="text-foreground"
                                            />
                                        </>
                                    )}
                                </>
                            )}
                        </p>
                    </div>
                )
            }}
        </form.Subscribe>
    )
}

const BLOCKER_SECTIONS = [
    ["lines", "商品明细"],
    ["delivery", "交付安排"],
    ["terms", "商业条款"],
    ["explain", "变更说明"],
] as const

function blockerSection(name: string) {
    if (name.startsWith("terms")) return "terms"
    if (name === "reason" || name === "remark") return "explain"
    if (/^lines\[\d+\]\.(dueDate|serviceRegion|deliveryAllocations)/.test(name))
        return "delivery"
    return "lines"
}

/** 提交按钮不可用时说明哪几块还没填好，点击跳到该块。 */
export function SalesChangeBlockers({
    form,
    id,
}: {
    form: SalesChangeDraftFormApi
    id: string
}) {
    return (
        <form.Subscribe
            selector={(state) =>
                state.canSubmit
                    ? ""
                    : Array.from(
                          new Set(
                              Object.entries(state.fieldMeta)
                                  .filter(([, meta]) => meta?.errors.length)
                                  .map(([name]) => blockerSection(name)),
                          ),
                      ).join(",")
            }
        >
            {(blocked) =>
                blocked ? (
                    <p
                        role="status"
                        className="flex flex-wrap items-center text-xs text-destructive"
                    >
                        待处理：
                        {BLOCKER_SECTIONS.filter(([key]) =>
                            blocked.split(",").includes(key),
                        ).map(([key, label], index) => (
                            <span
                                key={key}
                                className="inline-flex items-center"
                            >
                                {index ? "、" : null}
                                <button
                                    id={`${id}-blocker-${key}`}
                                    type="button"
                                    className="underline underline-offset-2"
                                    onClick={() =>
                                        document
                                            .getElementById(
                                                `${id}-${key}-title`,
                                            )
                                            ?.scrollIntoView({
                                                behavior: "smooth",
                                                block: "start",
                                            })
                                    }
                                >
                                    {label}
                                </button>
                            </span>
                        ))}
                    </p>
                ) : null
            }
        </form.Subscribe>
    )
}
