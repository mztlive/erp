"use client"
import * as React from "react"
import { toFieldErrors } from "@/components/form"
import { FieldError } from "@/components/ui/field"
import { Button } from "@/components/ui/button"
import { PAYMENT_TERM_OPTIONS, paymentTermLabel } from "@/lib/business-options"
import { multiplyFixed, sumFixed, subtractFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { SellableSkuSelectDialog } from "./sellable-sku-select-dialog"
import { salesChangePickedLine } from "../lib/sales-change-draft"
import type { SalesChangeDraftFormApi } from "../lib/sales-change-draft-form-types"
import type { SalesChangeDraft } from "../api/sales-change-draft"

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
    const [picker, setPicker] = React.useState<{ replace?: string } | null>(
        null,
    )
    return (
        <>
            <section className="space-y-3">
                <h3 className="font-medium">商业条款</h3>
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
                            />
                        )}
                    </form.AppField>
                </div>
            </section>
            <form.Subscribe selector={(state) => state.values.lines}>
                {(values) => (
                    <>
                        <div className="flex flex-wrap items-center justify-between gap-3 border-b pb-3">
                            <div>
                                <h3 className="font-medium">变更商品</h3>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    填写变更后的数量。移除或更换商品保留历史记录，已有采购、交付另行处理。
                                </p>
                            </div>
                            <Button
                                id={`${id}-add-product`}
                                type="button"
                                variant="outline"
                                disabled={disabled || values.length >= 100}
                                onClick={() => setPicker({})}
                            >
                                添加商品
                            </Button>
                        </div>
                        {values.map((value, index) => {
                            const line = value.source
                            const removed = value.quantity.trim() === "0"
                            const saved = draft.lines.find(
                                (item) =>
                                    item.sales_order_line_id ===
                                    line.sales_order_line_id,
                            )
                            const original = draft.base_lines.find(
                                (item) =>
                                    item.sales_order_line_id ===
                                    line.sales_order_line_id,
                            )
                            return (
                                <div
                                    key={line.sales_order_line_id}
                                    className="space-y-4 rounded-lg border p-4"
                                >
                                    <div className="flex flex-wrap items-start justify-between gap-3">
                                        <div>
                                            <p className="font-medium">
                                                {line.line_no}.{" "}
                                                {line.item_name_snapshot}
                                            </p>
                                            <p className="text-sm text-muted-foreground">
                                                {line.spec_snapshot ?? "—"} ·{" "}
                                                {line.unit_snapshot ?? "—"}
                                            </p>
                                            {original?.goods ? (
                                                <p className="mt-1 text-xs text-muted-foreground">
                                                    原数量{" "}
                                                    {original.goods.quantity} ·
                                                    原单价 ¥
                                                    {
                                                        original.goods
                                                            .unit_price_gross
                                                    }
                                                </p>
                                            ) : (
                                                <p className="mt-1 text-xs text-muted-foreground">
                                                    新增商品
                                                </p>
                                            )}
                                        </div>
                                        <div className="flex gap-2">
                                            {removed ? (
                                                <>
                                                    <span className="text-sm text-muted-foreground">
                                                        已移除
                                                    </span>
                                                    <Button
                                                        id={`${id}-${line.line_no}-restore`}
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        disabled={disabled}
                                                        onClick={() =>
                                                            form.setFieldValue(
                                                                `lines[${index}].quantity`,
                                                                original?.goods
                                                                    ?.quantity ===
                                                                    "0"
                                                                    ? "1"
                                                                    : (original
                                                                          ?.goods
                                                                          ?.quantity ??
                                                                          "1"),
                                                            )
                                                        }
                                                    >
                                                        恢复商品
                                                    </Button>
                                                </>
                                            ) : (
                                                <>
                                                    <Button
                                                        id={`${id}-${line.line_no}-replace`}
                                                        type="button"
                                                        variant="outline"
                                                        size="sm"
                                                        disabled={
                                                            disabled ||
                                                            values.length >= 100
                                                        }
                                                        onClick={() =>
                                                            setPicker({
                                                                replace:
                                                                    line.sales_order_line_id,
                                                            })
                                                        }
                                                    >
                                                        更换商品
                                                    </Button>
                                                    <Button
                                                        id={`${id}-${line.line_no}-remove`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="sm"
                                                        disabled={disabled}
                                                        onClick={() => {
                                                            if (saved) {
                                                                form.setFieldValue(
                                                                    `lines[${index}].quantity`,
                                                                    "0",
                                                                )
                                                                form.setFieldValue(
                                                                    `lines[${index}].deliveryAllocations`,
                                                                    [],
                                                                )
                                                            } else
                                                                form.setFieldValue(
                                                                    "lines",
                                                                    values.filter(
                                                                        (
                                                                            item,
                                                                        ) =>
                                                                            item
                                                                                .source
                                                                                .sales_order_line_id !==
                                                                            line.sales_order_line_id,
                                                                    ),
                                                                )
                                                        }}
                                                    >
                                                        移除商品
                                                    </Button>
                                                </>
                                            )}
                                        </div>
                                    </div>
                                    {!removed ? (
                                        <>
                                            <div className="grid gap-3 sm:grid-cols-3">
                                                {line.goods ? (
                                                    <form.AppField
                                                        name={`lines[${index}].quantity`}
                                                    >
                                                        {(field) => (
                                                            <field.TextField
                                                                id={`${id}-${line.line_no}-quantity`}
                                                                label="数量"
                                                                inputMode="decimal"
                                                                required
                                                                disabled={
                                                                    disabled
                                                                }
                                                            />
                                                        )}
                                                    </form.AppField>
                                                ) : (
                                                    <>
                                                        <form.AppField
                                                            name={`lines[${index}].faceValue`}
                                                        >
                                                            {(field) => (
                                                                <field.TextField
                                                                    id={`${id}-${line.line_no}-face`}
                                                                    label="单卡面额"
                                                                    inputMode="decimal"
                                                                    required
                                                                    disabled={
                                                                        disabled
                                                                    }
                                                                />
                                                            )}
                                                        </form.AppField>
                                                        <form.AppField
                                                            name={`lines[${index}].cardCount`}
                                                        >
                                                            {(field) => (
                                                                <field.TextField
                                                                    id={`${id}-${line.line_no}-cards`}
                                                                    label="卡张数"
                                                                    inputMode="numeric"
                                                                    required
                                                                    disabled={
                                                                        disabled
                                                                    }
                                                                />
                                                            )}
                                                        </form.AppField>
                                                    </>
                                                )}
                                                <form.AppField
                                                    name={`lines[${index}].unitPrice`}
                                                >
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${id}-${line.line_no}-price`}
                                                            label="含税成交单价"
                                                            inputMode="decimal"
                                                            required
                                                            disabled={disabled}
                                                        />
                                                    )}
                                                </form.AppField>
                                                <form.AppField
                                                    name={`lines[${index}].taxRate`}
                                                >
                                                    {(field) => (
                                                        <field.TextField
                                                            id={`${id}-${line.line_no}-tax`}
                                                            label="销项税率（如13%填0.13）"
                                                            inputMode="decimal"
                                                            required
                                                            disabled={disabled}
                                                        />
                                                    )}
                                                </form.AppField>
                                            </div>
                                            <details
                                                className="space-y-3 border-t pt-3"
                                                open={
                                                    value.deliveryAllocations
                                                        .length > 0 ||
                                                    !value.dueDate
                                                }
                                            >
                                                <summary
                                                    id={`${id}-${line.line_no}-delivery-toggle`}
                                                    className="cursor-pointer text-sm text-muted-foreground"
                                                >
                                                    交付安排 ·{" "}
                                                    {value.dueDate ||
                                                        "待设置交付期限"}
                                                    {value.serviceRegion
                                                        ? ` · ${value.serviceRegion}`
                                                        : ""}
                                                </summary>
                                                {line.goods ? (
                                                    <div className="grid gap-3 sm:grid-cols-2">
                                                        <form.AppField
                                                            name={`lines[${index}].dueDate`}
                                                        >
                                                            {(field) => (
                                                                <field.TextField
                                                                    id={`${id}-${line.line_no}-due`}
                                                                    type="date"
                                                                    label="交付期限"
                                                                    required
                                                                    disabled={
                                                                        disabled ||
                                                                        removed
                                                                    }
                                                                />
                                                            )}
                                                        </form.AppField>
                                                        <form.AppField
                                                            name={`lines[${index}].serviceRegion`}
                                                        >
                                                            {(field) => (
                                                                <field.TextField
                                                                    id={`${id}-${line.line_no}-region`}
                                                                    label="服务地区"
                                                                    disabled={
                                                                        disabled ||
                                                                        removed
                                                                    }
                                                                />
                                                            )}
                                                        </form.AppField>
                                                    </div>
                                                ) : null}
                                                {line.goods && !removed ? (
                                                    <form.Subscribe
                                                        selector={(state) =>
                                                            state.values.lines[
                                                                index
                                                            ]
                                                                .deliveryAllocations
                                                        }
                                                    >
                                                        {(rows) => (
                                                            <div className="space-y-3">
                                                                <div className="flex items-center justify-between gap-2">
                                                                    <span className="text-sm font-medium">
                                                                        分地址交付
                                                                    </span>
                                                                    <Button
                                                                        id={`${id}-${line.line_no}-address-add`}
                                                                        type="button"
                                                                        variant="outline"
                                                                        size="sm"
                                                                        disabled={
                                                                            disabled ||
                                                                            rows.length >=
                                                                                100
                                                                        }
                                                                        onClick={() =>
                                                                            form.setFieldValue(
                                                                                `lines[${index}].deliveryAllocations`,
                                                                                [
                                                                                    ...rows,
                                                                                    {
                                                                                        rowKey: crypto.randomUUID(),
                                                                                        recipient:
                                                                                            "",
                                                                                        phone: "",
                                                                                        address:
                                                                                            "",
                                                                                        quantity:
                                                                                            "",
                                                                                    },
                                                                                ],
                                                                            )
                                                                        }
                                                                    >
                                                                        添加地址
                                                                    </Button>
                                                                </div>
                                                                <p className="text-xs text-muted-foreground">
                                                                    分配数量合计须等于本商品数量；数量归零时请移除地址分配。
                                                                </p>
                                                                {rows.map(
                                                                    (
                                                                        row,
                                                                        addressIndex,
                                                                    ) => (
                                                                        <div
                                                                            key={
                                                                                row.rowKey
                                                                            }
                                                                            className="grid gap-2 rounded-md bg-muted/30 p-3 sm:grid-cols-2"
                                                                        >
                                                                            {(
                                                                                [
                                                                                    [
                                                                                        "recipient",
                                                                                        "收货人",
                                                                                    ],
                                                                                    [
                                                                                        "phone",
                                                                                        "联系电话",
                                                                                    ],
                                                                                    [
                                                                                        "address",
                                                                                        "完整收货地址",
                                                                                    ],
                                                                                    [
                                                                                        "quantity",
                                                                                        "分配数量",
                                                                                    ],
                                                                                ] as const
                                                                            ).map(
                                                                                ([
                                                                                    key,
                                                                                    label,
                                                                                ]) => (
                                                                                    <form.AppField
                                                                                        key={
                                                                                            key
                                                                                        }
                                                                                        name={`lines[${index}].deliveryAllocations[${addressIndex}].${key}`}
                                                                                    >
                                                                                        {(
                                                                                            field,
                                                                                        ) => (
                                                                                            <field.TextField
                                                                                                id={`${id}-${line.line_no}-${toAutomationIdSegment(row.rowKey)}-${key}`}
                                                                                                label={
                                                                                                    label
                                                                                                }
                                                                                                disabled={
                                                                                                    disabled
                                                                                                }
                                                                                            />
                                                                                        )}
                                                                                    </form.AppField>
                                                                                ),
                                                                            )}
                                                                            <Button
                                                                                id={`${id}-${line.line_no}-${toAutomationIdSegment(row.rowKey)}-address-remove`}
                                                                                type="button"
                                                                                variant="ghost"
                                                                                size="sm"
                                                                                disabled={
                                                                                    disabled
                                                                                }
                                                                                onClick={() =>
                                                                                    form.setFieldValue(
                                                                                        `lines[${index}].deliveryAllocations`,
                                                                                        rows.filter(
                                                                                            (
                                                                                                _,
                                                                                                rowIndex,
                                                                                            ) =>
                                                                                                rowIndex !==
                                                                                                addressIndex,
                                                                                        ),
                                                                                    )
                                                                                }
                                                                            >
                                                                                移除地址
                                                                            </Button>
                                                                        </div>
                                                                    ),
                                                                )}
                                                                <form.AppField
                                                                    name={`lines[${index}].deliveryAllocations`}
                                                                >
                                                                    {(
                                                                        field,
                                                                    ) => (
                                                                        <FieldError
                                                                            errors={toFieldErrors(
                                                                                field
                                                                                    .state
                                                                                    .meta
                                                                                    .errors,
                                                                            )}
                                                                        />
                                                                    )}
                                                                </form.AppField>
                                                            </div>
                                                        )}
                                                    </form.Subscribe>
                                                ) : null}
                                            </details>
                                        </>
                                    ) : null}
                                </div>
                            )
                        })}
                        <SellableSkuSelectDialog
                            open={picker !== null}
                            onOpenChange={(open) => !open && setPicker(null)}
                            multiple={!picker?.replace}
                            excludeProductKind="VOUCHER"
                            title={
                                picker?.replace
                                    ? "选择替换商品"
                                    : "添加销售商品"
                            }
                            confirmLabel="加入改单"
                            onConfirm={(picks) => {
                                const replacement = values.find(
                                    (item) =>
                                        item.source.sales_order_line_id ===
                                        picker?.replace,
                                )
                                const nextNo = Math.max(
                                    draft.next_line_no,
                                    ...values.map(
                                        (item) => item.source.line_no + 1,
                                    ),
                                )
                                if (values.length + picks.length > 100) {
                                    onError("一张改单最多保留 100 条商品记录")
                                    return
                                }
                                const kept = values.flatMap((item) => {
                                    if (item !== replacement) return [item]
                                    if (
                                        !draft.lines.some(
                                            (old) =>
                                                old.sales_order_line_id ===
                                                item.source.sales_order_line_id,
                                        )
                                    )
                                        return []
                                    return [
                                        {
                                            ...item,
                                            quantity: "0",
                                            deliveryAllocations: [],
                                        },
                                    ]
                                })
                                form.setFieldValue("lines", [
                                    ...kept,
                                    ...picks.map((pick, offset) =>
                                        salesChangePickedLine(
                                            pick,
                                            changeOrderId,
                                            nextNo + offset,
                                            replacement ??
                                                values.find(
                                                    (item) => item.source.goods,
                                                ),
                                            Boolean(replacement),
                                        ),
                                    ),
                                ])
                                setPicker(null)
                            }}
                        />
                    </>
                )}
            </form.Subscribe>
            <form.AppField name="reason">
                {(field) => (
                    <field.TextareaField
                        id={`${id}-reason`}
                        label="变更原因"
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
                        disabled={disabled}
                    />
                )}
            </form.AppField>
            <form.Subscribe selector={(state) => state.values.lines}>
                {(lines) => {
                    let total = "—"
                    let delta = "—"
                    try {
                        if (
                            lines.some(
                                (line) =>
                                    !line.quantity.trim() ||
                                    !line.unitPrice.trim(),
                            )
                        )
                            throw new Error("商品数量或价格尚未填写完整")
                        total = sumFixed(
                            lines.map((line) =>
                                multiplyFixed(
                                    line.quantity || "0",
                                    line.unitPrice || "0",
                                    {
                                        leftMaxScale: 6,
                                        rightMaxScale: 4,
                                        outputScale: 2,
                                    },
                                ),
                            ),
                            { maxScale: 2, outputScale: 2 },
                        )
                        delta = subtractFixed(total, draft.base_gross_amount, {
                            maxScale: 2,
                            outputScale: 2,
                        })
                    } catch {
                        /* Field errors identify invalid inputs. */
                    }
                    return (
                        <div className="rounded-lg bg-muted/40 p-4 text-sm">
                            <p className="mb-2 text-muted-foreground">
                                当前生效金额 ¥{draft.base_gross_amount} ·
                                本次调整 ¥{delta}
                            </p>
                            <span className="text-muted-foreground">
                                变更后含税总额
                            </span>
                            <strong className="ml-4 text-xl tabular-nums">
                                ¥{total}
                            </strong>
                            <p className="mt-2 text-xs text-muted-foreground">
                                提交后进入审批，原销售版本保持有效。已有回款、开票和交付记录保留。
                            </p>
                        </div>
                    )
                }}
            </form.Subscribe>
        </>
    )
}
