"use client"

import * as React from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { isApiError } from "@/lib/api/errors"
import { compareDecimal } from "@/lib/fixed-decimal"
import { positiveDecimal } from "../lib/price-validation"
import { createIdempotencyKey } from "../lib/validation"
import type { UpdateSelectionPricesInput } from "../api/books"

const schema = z.object({ price: positiveDecimal })

export type InlinePriceEditorContext = {
    batchId: string
    version: number
    onSubmit: (input: UpdateSelectionPricesInput) => Promise<unknown>
}

/** 表格内按 SKU 保存售价；结果未知时固定原请求重试。 */
export function InlinePriceEditor({
    id,
    skuId,
    name,
    price,
    pending,
    context,
}: {
    id: string
    skuId: string
    name: string
    price: string
    pending: boolean
    context: InlinePriceEditorContext
}) {
    const [error, setError] = React.useState("")
    const [uncertain, setUncertain] = React.useState(false)
    const request = React.useRef<UpdateSelectionPricesInput | null>(null)
    const baseline = React.useRef({
        batchId: context.batchId,
        version: context.version,
    })
    const savedPrice = React.useRef(price)
    const form = useAppForm({
        defaultValues: { price },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (pending) return
            const parsed = schema.parse(value)
            if (
                !request.current &&
                compareDecimal(parsed.price, price, 2) === 0
            )
                return
            const payload = request.current ?? {
                batch_id: baseline.current.batchId,
                expected_version: baseline.current.version,
                idempotency_key: createIdempotencyKey(),
                change: {
                    kind: "SINGLE" as const,
                    sku_id: skuId,
                    price: parsed.price,
                },
            }
            request.current = payload
            setError("")
            try {
                await context.onSubmit(payload)
                request.current = null
                setUncertain(false)
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
                        ? "保存结果尚未确认，请确认本次结果后继续。"
                        : cause.message,
                )
            }
        },
    })

    React.useEffect(() => {
        if (request.current) return
        const changedBatch = baseline.current.batchId !== context.batchId
        baseline.current = {
            batchId: context.batchId,
            version: context.version,
        }
        if (changedBatch || savedPrice.current !== price) {
            savedPrice.current = price
            form.reset({ price })
        }
    }, [context.batchId, context.version, price, pending, form])

    return (
        <form
            className="grid min-w-40 gap-1 text-left"
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                void form.handleSubmit()
            }}
        >
            <div className="flex items-start gap-1">
                <form.AppField
                    name="price"
                    children={(field) => (
                        <field.TextField
                            id={id}
                            label={`${name}客户售价`}
                            hideLabel
                            type="text"
                            inputMode="decimal"
                            disabled={pending || uncertain}
                            inputClassName="text-right num"
                        />
                    )}
                />
                <form.AppForm>
                    <form.SubmitButton
                        id={`${id}-save`}
                        size="sm"
                        variant="outline"
                        label={uncertain ? "确认结果" : "保存"}
                        pendingLabel="保存中…"
                        loading={pending}
                        disabled={pending}
                    />
                </form.AppForm>
            </div>
            {error ? (
                <p
                    id={`${id}-save-error`}
                    role="alert"
                    className="text-xs text-destructive"
                >
                    {error}
                </p>
            ) : null}
        </form>
    )
}
