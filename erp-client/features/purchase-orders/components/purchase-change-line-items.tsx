"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import { MoneyValue } from "@/components/business"
import { ChangeOrderSection } from "@/components/business/change-order-section"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseChangeDraft } from "../api/purchase-change-draft"
import {
    expenseLineName,
    mapExpenseDetail,
} from "../api/purchase-order-mapping"
import {
    purchaseChangeBaseLine,
    purchaseChangeGapAfter,
    purchaseChangeLineAmount,
    purchaseChangeLineDiff,
    purchaseChangeLineOption,
    type PurchaseChangeDeliveryOptions,
    type PurchaseChangeLineDiff,
    type PurchaseChangeLineValues,
} from "../lib/purchase-change-draft"
import type { PurchaseChangeDraftFormApi } from "../lib/purchase-change-draft-form-types"

const DIFF_BADGE: Record<
    Exclude<PurchaseChangeLineDiff["status"], "unchanged">,
    { label: string; variant: "warning" | "neutral" }
> = {
    changed: { label: "已修改", variant: "warning" },
    removed: { label: "已移除", variant: "neutral" },
}

const LINE_GRID =
    "grid items-start gap-3 sm:grid-cols-2 @3xl/lines:grid-cols-[minmax(0,2.4fr)_minmax(0,1fr)_minmax(0,1.1fr)_minmax(0,0.9fr)_minmax(0,1fr)_7.5rem]"
const WIDE_LABEL_HIDDEN =
    "min-w-0 @3xl/lines:[&>[data-slot=field-label]]:sr-only"

function formatAmount(line: PurchaseChangeLineValues) {
    try {
        return purchaseChangeLineAmount(line)
    } catch {
        return null
    }
}

/** 采购明细：每行对照生效版本标出改动；数量改为 0 表示移除，缺口生效后回到供给分配。 */
export function PurchaseChangeLineItems({
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
    /** 已确认发生的费用行身份；这些行在改单中只读。 */
    occurredExpenseIds: ReadonlySet<string>
}) {
    return (
        <form.Subscribe selector={(state) => state.values.lines}>
            {(values) => {
                const diffs = values.map((value) =>
                    purchaseChangeLineDiff(
                        value,
                        purchaseChangeBaseLine(draft, value.source),
                    ),
                )
                const touched = diffs.filter(
                    (diff) => diff.status !== "unchanged",
                ).length
                return (
                    <ChangeOrderSection
                        id={`${id}-lines`}
                        title="采购明细"
                        description={
                            touched
                                ? `共 ${values.length} 项，${touched} 项有改动。移除的商品保留记录，销售缺口生效后回到供给分配。`
                                : `共 ${values.length} 项，尚未改动。直接修改数量、单价或税率；不再采购的商品点「移除」。`
                        }
                    >
                        <div className="@container/lines min-w-0">
                            <div
                                aria-hidden="true"
                                className={cn(
                                    LINE_GRID,
                                    "hidden rounded-t-md border-b border-grid bg-muted/40 px-3 py-2 text-xs text-muted-foreground @3xl/lines:grid",
                                )}
                            >
                                <span>商品</span>
                                <span>
                                    数量
                                    <RequiredMark />
                                </span>
                                <span>
                                    含税采购单价
                                    <RequiredMark />
                                </span>
                                <span>
                                    进项税率
                                    <RequiredMark />
                                </span>
                                <span className="text-right">小计</span>
                                <span />
                            </div>
                            <div className="divide-y divide-grid">
                                {values.map((value, index) => (
                                    <PurchaseChangeLineRow
                                        key={value.lineKey}
                                        form={form}
                                        id={id}
                                        value={value}
                                        index={index}
                                        diff={diffs[index]}
                                        draft={draft}
                                        options={options}
                                        disabled={disabled}
                                        occurredExpenseIds={occurredExpenseIds}
                                    />
                                ))}
                            </div>
                        </div>
                    </ChangeOrderSection>
                )
            }}
        </form.Subscribe>
    )
}

function PurchaseChangeLineRow({
    form,
    id,
    value,
    index,
    diff,
    draft,
    options,
    disabled: formDisabled,
    occurredExpenseIds,
}: {
    form: PurchaseChangeDraftFormApi
    id: string
    value: PurchaseChangeLineValues
    index: number
    diff: PurchaseChangeLineDiff
    draft: PurchaseChangeDraft
    options: PurchaseChangeDeliveryOptions
    disabled: boolean
    occurredExpenseIds: ReadonlySet<string>
}) {
    const line = value.source
    const item = line.line_type === "ITEM_SERVICE"
    const expense = mapExpenseDetail(line.expense)
    const occurred = Boolean(
        expense && occurredExpenseIds.has(expense.expenseLineId),
    )
    const disabled = formDisabled || occurred
    const removed = diff.status === "removed"
    const prefix = `${id}-line-${toAutomationIdSegment(value.lineKey)}`
    const badge = diff.status === "unchanged" ? null : DIFF_BADGE[diff.status]
    const amount = formatAmount(value)
    const base = purchaseChangeBaseLine(draft, line)
    const option = purchaseChangeLineOption(line, options)
    const bound = option?.quantityBound
    const previous = (
        key: keyof PurchaseChangeLineDiff["previous"],
        money = false,
    ) =>
        diff.previous[key] === undefined
            ? undefined
            : `原 ${money ? "¥" : ""}${diff.previous[key]}`
    const numberField = (
        key: "quantity" | "unitCostGross" | "inputTaxRate" | "grossAmount",
        suffix: string,
        label: string,
        money = false,
    ) => (
        <form.AppField name={`lines[${index}].${key}`}>
            {(field) => (
                <field.TextField
                    id={`${prefix}-${suffix}`}
                    label={label}
                    inputMode="decimal"
                    required
                    disabled={disabled}
                    className={WIDE_LABEL_HIDDEN}
                    inputClassName="num"
                    description={previous(key, money)}
                />
            )}
        </form.AppField>
    )
    return (
        <div className={cn("py-4 @3xl/lines:px-3", removed && "bg-muted/30")}>
            <div className={LINE_GRID}>
                <div className="min-w-0 space-y-1 sm:col-span-2 @3xl/lines:col-span-1 @3xl/lines:pt-1.5">
                    <div className="flex flex-wrap items-center gap-2">
                        <p
                            className={cn(
                                "break-words font-medium",
                                removed &&
                                    "text-muted-foreground line-through decoration-muted-foreground/60",
                            )}
                        >
                            {item
                                ? (line.product_name ?? "采购商品/服务")
                                : expenseLineName(expense)}
                        </p>
                        {occurred ? (
                            <Badge variant="secondary">已发生 · 不可修改</Badge>
                        ) : null}
                        {badge ? (
                            <Badge variant={badge.variant}>{badge.label}</Badge>
                        ) : null}
                        {bound?.quantityAffected && !removed ? (
                            <Badge variant="warning">销售已调减</Badge>
                        ) : null}
                    </div>
                    {item ? (
                        <p className="text-xs text-muted-foreground">
                            {line.specification ?? "无规格"} ·{" "}
                            {line.base_unit_code ?? "—"}
                        </p>
                    ) : null}
                </div>
                {item ? (
                    numberField("quantity", "quantity", "采购数量")
                ) : (
                    <span className="hidden @3xl/lines:block" />
                )}
                {removed ? (
                    <>
                        <RemovedCell label="含税采购单价" />
                        <RemovedCell label="进项税率" />
                    </>
                ) : (
                    <>
                        {item
                            ? numberField(
                                  "unitCostGross",
                                  "unit-cost",
                                  "含税采购单价",
                                  true,
                              )
                            : numberField(
                                  "grossAmount",
                                  "logistics-gross",
                                  "含税费用金额",
                                  true,
                              )}
                        {numberField("inputTaxRate", "tax-rate", "进项税率")}
                    </>
                )}
                <div className="flex items-baseline justify-between gap-2 @3xl/lines:block @3xl/lines:pt-2 @3xl/lines:text-right">
                    <span className="text-xs text-muted-foreground @3xl/lines:sr-only">
                        小计
                    </span>
                    {removed ? (
                        <span className="num text-muted-foreground">—</span>
                    ) : (
                        <MoneyValue value={amount} className="justify-end" />
                    )}
                </div>
                <div className="flex flex-wrap justify-end gap-1 sm:col-span-2 @3xl/lines:col-span-1 @3xl/lines:pt-0.5">
                    {item &&
                    bound?.quantityAffected &&
                    compactFixed(value.quantity || "0") !==
                        compactFixed(bound.maxQuantity) ? (
                        <Button
                            id={`${prefix}-use-sales-target`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={disabled}
                            onClick={() => {
                                const target = compactFixed(bound.maxQuantity)
                                form.setFieldValue(
                                    `lines[${index}].quantity`,
                                    target,
                                )
                                // 单地址无需用户重复填写；多地址保留人工分配。
                                if (
                                    option?.requiresAddresses &&
                                    option.addresses.length === 1 &&
                                    compareDecimal(
                                        target,
                                        option.addresses[0].availableQuantity,
                                        6,
                                    ) <= 0
                                ) {
                                    const address = option.addresses[0]
                                    form.setFieldValue(
                                        `lines[${index}].deliveryRows`,
                                        [
                                            {
                                                recipient: address.recipient,
                                                phone: address.phone,
                                                region_code:
                                                    address.region_code,
                                                region_name:
                                                    address.region_name,
                                                address: address.address,
                                                quantity: target,
                                            },
                                        ],
                                    )
                                }
                            }}
                        >
                            按可调数量填入
                        </Button>
                    ) : null}
                    {!item ? null : removed ? (
                        <Button
                            id={`${prefix}-restore`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={disabled}
                            onClick={() =>
                                form.setFieldValue(
                                    `lines[${index}].quantity`,
                                    compactFixed(
                                        base?.quantity && base.quantity !== "0"
                                            ? base.quantity
                                            : "1",
                                    ),
                                )
                            }
                        >
                            恢复
                        </Button>
                    ) : (
                        <Button
                            id={`${prefix}-remove`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="text-muted-foreground"
                            disabled={disabled}
                            onClick={() =>
                                form.setFieldValue(
                                    `lines[${index}].quantity`,
                                    "0",
                                )
                            }
                        >
                            移除
                        </Button>
                    )}
                </div>
            </div>
            {item ? <LineCoverageNote value={value} options={options} /> : null}
        </div>
    )
}

/** 本行可填范围与改后销售缺口。 */
function LineCoverageNote({
    value,
    options,
}: {
    value: PurchaseChangeLineValues
    options: PurchaseChangeDeliveryOptions
}) {
    const bound = purchaseChangeLineOption(value.source, options)?.quantityBound
    if (!bound) return null
    const gap = purchaseChangeGapAfter(value.quantity, bound)
    const fulfilled = compactFixed(bound.fulfilledQuantity)
    return (
        <div className="mt-2 space-y-0.5 text-xs text-muted-foreground">
            <p className="num">
                销售需要 {compactFixed(bound.salesQuantity)}
                ，其他采购与库存已覆盖{" "}
                {compactFixed(bound.otherCoveredQuantity)}
                {fulfilled !== "0"
                    ? `，本单已履约（扣除已退回）${fulfilled}`
                    : ""}
                ；本单可填 {compactFixed(bound.minQuantity)} 至{" "}
                {compactFixed(bound.maxQuantity)}
            </p>
            {gap ? (
                <p
                    className="num text-warning-soft-foreground"
                    aria-live="polite"
                >
                    改后销售还差 {gap}
                    ，生效后回到供给分配，由采购负责人重新安排供应
                </p>
            ) : null}
        </div>
    )
}

function RemovedCell({ label }: { label: string }) {
    return (
        <div className="flex items-baseline justify-between gap-2 text-muted-foreground @3xl/lines:block @3xl/lines:pt-2">
            <span className="text-xs @3xl/lines:sr-only">{label}</span>
            <span className="num">—</span>
        </div>
    )
}
