"use client"
import * as React from "react"
import { PlusIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"
import { SellableSkuSelectDialog } from "./sellable-sku-select-dialog"
import { ChangeOrderSection } from "@/components/business/change-order-section"
import {
    salesChangeLineAmount,
    salesChangeLineDiff,
    salesChangePickedLine,
    type SalesChangeDraftValues,
    type SalesChangeLineDiff,
} from "../lib/sales-change-draft"
import type { SalesChangeDraftFormApi } from "../lib/sales-change-draft-form-types"
import type { SalesChangeDraft } from "../api/sales-change-draft"

type LineValues = SalesChangeDraftValues["lines"][number]

const DIFF_BADGE: Record<
    Exclude<SalesChangeLineDiff["status"], "unchanged">,
    { label: string; variant: "info" | "warning" | "neutral" }
> = {
    added: { label: "新增", variant: "info" },
    changed: { label: "已修改", variant: "warning" },
    removed: { label: "已移除", variant: "neutral" },
}

const GOODS_GRID =
    "@3xl/lines:grid-cols-[minmax(0,2.4fr)_minmax(0,1fr)_minmax(0,1.1fr)_minmax(0,0.9fr)_minmax(0,1fr)_7.5rem]"
const VOUCHER_GRID =
    "@3xl/lines:grid-cols-[minmax(0,2fr)_minmax(0,1fr)_minmax(0,0.8fr)_minmax(0,1.1fr)_minmax(0,0.9fr)_minmax(0,1fr)_7.5rem]"
const ROW_GRID = "grid items-start gap-3 sm:grid-cols-2"
const WIDE_LABEL_HIDDEN =
    "min-w-0 @3xl/lines:[&>[data-slot=field-label]]:sr-only"

function formatAmount(line: LineValues) {
    try {
        return salesChangeLineAmount(line)
    } catch {
        return null
    }
}

/** 商品明细：每行对照生效版本标出改动，数量归零表示移除。 */
export function SalesChangeLineItems({
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
        <form.Subscribe selector={(state) => state.values.lines}>
            {(values) => {
                const voucher =
                    values.length > 0 && values.every((v) => !v.source.goods)
                const diffs = values.map((value) =>
                    salesChangeLineDiff(
                        value,
                        draft.base_lines.find(
                            (item) =>
                                item.sales_order_line_id ===
                                value.source.sales_order_line_id,
                        ),
                    ),
                )
                const touched = diffs.filter(
                    (diff) => diff.status !== "unchanged",
                ).length
                const columns = voucher
                    ? [
                          "商品",
                          "单卡面额",
                          "卡张数",
                          "含税成交单价",
                          "销项税率",
                          "小计",
                          "",
                      ]
                    : ["商品", "数量", "含税成交单价", "销项税率", "小计", ""]
                return (
                    <ChangeOrderSection
                        id={`${id}-lines`}
                        title="商品明细"
                        description={
                            touched
                                ? `共 ${values.length} 项，${touched} 项有改动。移除或更换的商品保留历史记录，已有采购、交付另行处理。`
                                : `共 ${values.length} 项，尚未改动。直接修改数量、单价或税率；移除或更换的商品保留历史记录。`
                        }
                        action={
                            <Button
                                id={`${id}-add-product`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={disabled || values.length >= 100}
                                onClick={() => setPicker({})}
                            >
                                <PlusIcon aria-hidden="true" />
                                添加商品
                            </Button>
                        }
                    >
                        <div className="@container/lines min-w-0">
                            <div
                                aria-hidden="true"
                                className={cn(
                                    ROW_GRID,
                                    voucher ? VOUCHER_GRID : GOODS_GRID,
                                    "hidden rounded-t-md border-b border-grid bg-muted/40 px-3 py-2 text-xs text-muted-foreground @3xl/lines:grid",
                                )}
                            >
                                {columns.map((label, index) => (
                                    <span
                                        key={label || index}
                                        className={
                                            label === "小计"
                                                ? "text-right"
                                                : undefined
                                        }
                                    >
                                        {label}
                                    </span>
                                ))}
                            </div>
                            <div className="divide-y divide-grid">
                                {values.map((value, index) => (
                                    <SalesChangeLineRow
                                        key={value.source.sales_order_line_id}
                                        form={form}
                                        id={id}
                                        value={value}
                                        index={index}
                                        diff={diffs[index]}
                                        values={values}
                                        draft={draft}
                                        disabled={disabled}
                                        hideLabels={
                                            voucher === !value.source.goods
                                        }
                                        onReplace={() =>
                                            setPicker({
                                                replace:
                                                    value.source
                                                        .sales_order_line_id,
                                            })
                                        }
                                    />
                                ))}
                            </div>
                        </div>
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
                    </ChangeOrderSection>
                )
            }}
        </form.Subscribe>
    )
}

function SalesChangeLineRow({
    form,
    id,
    value,
    index,
    diff,
    values,
    draft,
    disabled,
    hideLabels,
    onReplace,
}: {
    form: SalesChangeDraftFormApi
    id: string
    value: LineValues
    index: number
    diff: SalesChangeLineDiff
    values: LineValues[]
    draft: SalesChangeDraft
    disabled: boolean
    hideLabels: boolean
    onReplace: () => void
}) {
    const line = value.source
    const removed = diff.status === "removed"
    const original = draft.base_lines.find(
        (item) => item.sales_order_line_id === line.sales_order_line_id,
    )
    const badge = diff.status === "unchanged" ? null : DIFF_BADGE[diff.status]
    const amount = formatAmount(value)
    const labelClass = hideLabels ? WIDE_LABEL_HIDDEN : "min-w-0"
    const previous = (
        key: keyof SalesChangeLineDiff["previous"],
        money = false,
    ) =>
        diff.previous[key] === undefined
            ? undefined
            : `原 ${money ? "¥" : ""}${diff.previous[key]}`
    const numberField = (
        key: "quantity" | "faceValue" | "cardCount" | "unitPrice" | "taxRate",
        suffix: string,
        label: string,
        inputMode: "decimal" | "numeric",
        money = false,
    ) => (
        <form.AppField name={`lines[${index}].${key}`}>
            {(field) => (
                <field.TextField
                    id={`${id}-${line.line_no}-${suffix}`}
                    label={label}
                    inputMode={inputMode}
                    required
                    disabled={disabled || (key === "unitPrice" && value.gift)}
                    className={labelClass}
                    inputClassName="num"
                    description={previous(key, money)}
                />
            )}
        </form.AppField>
    )
    return (
        <div
            className={cn(
                ROW_GRID,
                line.goods ? GOODS_GRID : VOUCHER_GRID,
                "py-4 @3xl/lines:px-3",
                removed && "bg-muted/30",
            )}
        >
            <div className="min-w-0 space-y-1 sm:col-span-2 @3xl/lines:col-span-1 @3xl/lines:pt-1.5">
                <div className="flex flex-wrap items-center gap-2">
                    <p
                        className={cn(
                            "break-words font-medium",
                            removed &&
                                "text-muted-foreground line-through decoration-muted-foreground/60",
                        )}
                    >
                        {line.line_no}. {line.item_name_snapshot}
                    </p>
                    {badge ? (
                        <Badge variant={badge.variant}>{badge.label}</Badge>
                    ) : null}
                    {value.gift && original ? (
                        <Badge variant="secondary">赠品</Badge>
                    ) : null}
                </div>
                <p className="text-xs text-muted-foreground">
                    {line.spec_snapshot ?? "无规格"} ·{" "}
                    {line.unit_snapshot ?? "—"}
                </p>
                {line.goods && !original ? (
                    <form.AppField name={`lines[${index}].gift`}>
                        {(field) => (
                            <Label
                                htmlFor={`${id}-${line.line_no}-gift`}
                                className="flex items-center gap-1.5 text-xs font-normal text-muted-foreground"
                            >
                                <Checkbox
                                    id={`${id}-${line.line_no}-gift`}
                                    checked={field.state.value}
                                    disabled={disabled}
                                    onCheckedChange={(checked) => {
                                        field.handleChange(checked === true)
                                        form.setFieldValue(
                                            `lines[${index}].unitPrice`,
                                            checked === true ? "0" : "",
                                        )
                                    }}
                                />
                                赠品（单价为 0，不收款）
                            </Label>
                        )}
                    </form.AppField>
                ) : null}
            </div>
            {line.goods ? (
                numberField("quantity", "quantity", "数量", "decimal")
            ) : (
                <>
                    {numberField(
                        "faceValue",
                        "face",
                        "单卡面额",
                        "decimal",
                        true,
                    )}
                    {numberField("cardCount", "cards", "卡张数", "numeric")}
                </>
            )}
            {removed ? (
                <>
                    <RemovedCell label="含税成交单价" />
                    <RemovedCell label="销项税率" />
                </>
            ) : (
                <>
                    {numberField(
                        "unitPrice",
                        "price",
                        "含税成交单价",
                        "decimal",
                        true,
                    )}
                    {numberField("taxRate", "tax", "销项税率", "decimal")}
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
                {!line.goods ? null : removed ? (
                    <Button
                        id={`${id}-${line.line_no}-restore`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={disabled}
                        onClick={() =>
                            form.setFieldValue(
                                `lines[${index}].quantity`,
                                original?.goods?.quantity === "0"
                                    ? "1"
                                    : (original?.goods?.quantity ?? "1"),
                            )
                        }
                    >
                        恢复
                    </Button>
                ) : (
                    <>
                        <Button
                            id={`${id}-${line.line_no}-replace`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={disabled || values.length >= 100}
                            onClick={onReplace}
                        >
                            更换
                        </Button>
                        <Button
                            id={`${id}-${line.line_no}-remove`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="text-muted-foreground"
                            disabled={disabled}
                            onClick={() => {
                                if (
                                    draft.lines.some(
                                        (item) =>
                                            item.sales_order_line_id ===
                                            line.sales_order_line_id,
                                    )
                                ) {
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
                                            (item) =>
                                                item.source
                                                    .sales_order_line_id !==
                                                line.sales_order_line_id,
                                        ),
                                    )
                            }}
                        >
                            移除
                        </Button>
                    </>
                )}
            </div>
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
