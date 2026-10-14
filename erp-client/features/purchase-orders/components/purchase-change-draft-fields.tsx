"use client"

import { MoneyValue } from "@/components/business"
import { ChangeOrderSection } from "@/components/business/change-order-section"
import {
    paymentTermLabel,
    SUPPLIER_PAYMENT_TERM_OPTIONS,
} from "@/lib/business-options"
import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"
import type { PurchaseChangeDraft } from "../api/purchase-change-draft"
import {
    purchaseChangeTotal,
    type PurchaseChangeDeliveryOptions,
} from "../lib/purchase-change-draft"
import type { PurchaseChangeDraftFormApi } from "../lib/purchase-change-draft-form-types"
import { PurchaseChangeDeliverySection } from "./purchase-change-delivery-section"
import { PurchaseChangeLineItems } from "./purchase-change-line-items"

/** 改单正文：采购明细 → 交付安排 → 商业条款 → 变更说明（与销售改单同序）。 */
export function PurchaseChangeDraftFields({
    form,
    id,
    draft,
    options,
    disabled,
    occurredExpenseIds,
}: {
    form: PurchaseChangeDraftFormApi
    id: string
    draft: PurchaseChangeDraft
    options: PurchaseChangeDeliveryOptions
    disabled: boolean
    occurredExpenseIds: ReadonlySet<string>
}) {
    const paymentOptions = SUPPLIER_PAYMENT_TERM_OPTIONS.some(
        (option) => option.value === draft.payment_term_code,
    )
        ? SUPPLIER_PAYMENT_TERM_OPTIONS
        : [
              ...SUPPLIER_PAYMENT_TERM_OPTIONS,
              {
                  value: draft.payment_term_code,
                  label:
                      paymentTermLabel(draft.payment_term_code) ===
                      draft.payment_term_code
                          ? "原付款条件"
                          : paymentTermLabel(draft.payment_term_code),
              },
          ]
    return (
        <div className="space-y-6">
            <PurchaseChangeLineItems
                form={form}
                id={id}
                draft={draft}
                options={options}
                disabled={disabled}
                occurredExpenseIds={occurredExpenseIds}
            />
            <PurchaseChangeDeliverySection
                form={form}
                id={id}
                options={options}
                disabled={disabled}
            />
            <ChangeOrderSection
                id={`${id}-terms`}
                title="商业条款"
                description="随本次改单一并审批；不需要调整时保持原值。"
            >
                <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                    <form.AppField name="paymentTermCode">
                        {(field) => (
                            <field.SelectField
                                id={`${id}-payment-term`}
                                label="付款条件"
                                required
                                disabled={disabled}
                                allowClear={false}
                                options={paymentOptions}
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
                                placeholder="例如：供应商断货，本单减量，缺口改由其他供应商供应"
                                required
                                disabled={disabled}
                            />
                        )}
                    </form.AppField>
                </div>
            </ChangeOrderSection>
        </div>
    )
}

/** 页脚金额：变更后总额与相对当前生效版本的调整额；低于已付款时提示办理退款。 */
export function PurchaseChangeTotalSummary({
    form,
    baseAmount,
    paidAmount,
}: {
    form: PurchaseChangeDraftFormApi
    baseAmount: string
    /** 已付款金额；改后总额不得低于此金额。 */
    paidAmount?: string
}) {
    return (
        <form.Subscribe selector={(state) => state.values.lines}>
            {(lines) => {
                const total = purchaseChangeTotal(lines)
                let delta: string | null = null
                let belowPaid = false
                try {
                    if (total !== null) {
                        delta = subtractFixed(total, baseAmount || "0", {
                            maxScale: 2,
                            outputScale: 2,
                        })
                        belowPaid =
                            Boolean(paidAmount) &&
                            compareDecimal(total, paidAmount!, 2) < 0
                    }
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
                            当前生效 <MoneyValue value={baseAmount || null} />
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
                        {belowPaid ? (
                            <p className="text-xs text-destructive">
                                低于已付款 <MoneyValue value={paidAmount!} />
                                ，多付金额请先办理供应商退款
                            </p>
                        ) : null}
                    </div>
                )
            }}
        </form.Subscribe>
    )
}

const BLOCKER_SECTIONS = [
    ["lines", "采购明细"],
    ["delivery", "交付安排"],
    ["terms", "商业条款"],
    ["explain", "变更说明"],
] as const

function blockerSection(name: string) {
    if (name === "paymentTermCode") return "terms"
    if (name === "reason") return "explain"
    if (/^lines\[\d+\]\.(expectedDeliveryDate|deliveryRows)/.test(name))
        return "delivery"
    return "lines"
}

/** 提交按钮不可用时说明哪几块还没填好，点击跳到该块。 */
export function PurchaseChangeBlockers({
    form,
    id,
}: {
    form: PurchaseChangeDraftFormApi
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
