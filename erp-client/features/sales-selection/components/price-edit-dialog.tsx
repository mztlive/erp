"use client"

import * as React from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { sumFixed } from "@/lib/fixed-decimal"
import { isApiError } from "@/lib/api/errors"
import { createIdempotencyKey } from "../lib/validation"
import type { DisplayItem } from "../types"
import type { UpdateSelectionPricesInput } from "../api/books"
import { positiveDecimal } from "../lib/price-validation"
import { PriceComparison } from "./price-comparison"

const priceSchema = z.object({ skuId: z.string(), value: positiveDecimal })

/** 单品与套餐成员定价；无 item 时执行当前批次统一加价。 */
export function PriceEditDialog({
    item,
    batchId,
    version,
    busy,
    onClose,
    onSubmit,
}: {
    item?: DisplayItem
    batchId: string
    version: number
    busy: boolean
    onClose: () => void
    onSubmit: (input: UpdateSelectionPricesInput) => Promise<unknown>
}) {
    const products =
        item?.kind === "PACKAGE"
            ? item.members
            : item?.sku_id
              ? [
                    {
                        sku_id: item.sku_id,
                        name: item.name,
                        price: item.price_gross || item.price,
                        reference_prices: item.reference_prices,
                        original_price: item.original_price,
                        markup_percent: item.markup_percent,
                    },
                ]
              : []
    const [error, setError] = React.useState("")
    const [uncertain, setUncertain] = React.useState(false)
    const baseline = React.useRef({ batchId, version })
    const request = React.useRef<UpdateSelectionPricesInput | null>(null)
    const form = useAppForm({
        defaultValues: {
            skuId: products[0]?.sku_id ?? "",
            value: item ? (products[0]?.price ?? "") : "",
        },
        validators: { onChange: priceSchema },
        onSubmit: async ({ value }) => {
            if (busy || (item && !value.skuId)) return
            const parsed = priceSchema.parse(value)
            const payload = request.current ?? {
                batch_id: baseline.current.batchId,
                expected_version: baseline.current.version,
                idempotency_key: createIdempotencyKey(),
                change: item
                    ? {
                          kind: "SINGLE" as const,
                          sku_id: parsed.skuId,
                          price: parsed.value,
                      }
                    : { kind: "MARKUP" as const, percent: parsed.value },
            }
            request.current = payload
            setError("")
            try {
                await onSubmit(payload)
                onClose()
            } catch (cause) {
                const unknown =
                    !isApiError(cause) ||
                    cause.kind === "Network" ||
                    cause.kind === "Parse" ||
                    (cause.status ?? 0) >= 500
                setUncertain(unknown)
                if (!unknown) request.current = null
                setError(
                    unknown
                        ? "改价结果尚未确认。请点击「确认改价结果」查询本次操作，避免重复加价。"
                        : isApiError(cause)
                          ? cause.message
                          : "改价失败，请重试。",
                )
            }
        },
    })
    const disabled = busy || uncertain

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !busy && !uncertain) onClose()
            }}
        >
            <DialogContent closeButtonId="selection-price-close">
                <DialogHeader>
                    <DialogTitle>
                        {item ? "编辑客户售价" : "统一加价"}
                    </DialogTitle>
                    <DialogDescription>
                        {item
                            ? "同一商品在本册各套餐中使用相同售价，套餐合计自动重算。"
                            : "对本选品册内的全部商品（含当前筛选隐藏的商品）按当前客户售价统一调整；再次加价将在最新售价基础上计算。"}
                        仅影响本选品册对客户的含税售价，商品资料中的价格不受影响。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="grid gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {item && products.length > 1 ? (
                        <form.AppField
                            name="skuId"
                            children={(field) => (
                                <field.SelectField
                                    id="selection-price-sku"
                                    label="商品"
                                    required
                                    disabled={disabled}
                                    allowClear={false}
                                    options={products.map((product) => ({
                                        value: product.sku_id,
                                        label: product.name,
                                    }))}
                                    onValueChange={(id) =>
                                        form.setFieldValue(
                                            "value",
                                            products.find(
                                                (product) =>
                                                    product.sku_id === id,
                                            )?.price ?? "",
                                        )
                                    }
                                />
                            )}
                        />
                    ) : item ? (
                        <p className="text-sm font-medium">{item.name}</p>
                    ) : null}
                    {item ? (
                        <form.Subscribe
                            selector={(state) => state.values.skuId}
                            children={(skuId) => (
                                <div className="space-y-2">
                                    <p className="text-sm text-muted-foreground">
                                        当前客户售价：
                                        <MoneyValue
                                            value={
                                                products.find(
                                                    (product) =>
                                                        product.sku_id ===
                                                        skuId,
                                                )?.price ?? "0"
                                            }
                                            display="exact"
                                        />
                                    </p>
                                    <PriceComparison
                                        {...products.find(
                                            (product) =>
                                                product.sku_id === skuId,
                                        )}
                                    />
                                </div>
                            )}
                        />
                    ) : null}
                    <form.AppField
                        name="value"
                        children={(field) => (
                            <field.TextField
                                id="selection-price-value"
                                label={
                                    item
                                        ? "客户含税售价（元）"
                                        : "加价百分比（%）"
                                }
                                required
                                inputMode="decimal"
                                money={Boolean(item)}
                                disabled={disabled}
                                description={
                                    item
                                        ? "售价必须大于 0，最多两位小数。"
                                        : "例如填写 10，售价 100 元调整为 110 元。逐商品计算，四舍五入保留两位小数。"
                                }
                            />
                        )}
                    />
                    {!item ? (
                        <form.Subscribe
                            selector={(state) => state.values.value}
                            children={(value) => {
                                const parsed = positiveDecimal.safeParse(value)
                                return parsed.success ? (
                                    <p className="text-sm text-muted-foreground">
                                        示例：¥100.00 →{" "}
                                        <MoneyValue
                                            value={sumFixed(
                                                ["100", parsed.data],
                                                { maxScale: 2, outputScale: 2 },
                                            )}
                                            display="exact"
                                        />
                                    </p>
                                ) : null
                            }}
                        />
                    ) : null}
                    {error ? (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="selection-price-cancel"
                            type="button"
                            variant="outline"
                            disabled={disabled}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="selection-price-submit"
                                label={
                                    uncertain ? "确认改价结果" : "保存客户售价"
                                }
                                pendingLabel="保存中…"
                                loading={busy}
                                disabled={
                                    busy || Boolean(item && !products.length)
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
