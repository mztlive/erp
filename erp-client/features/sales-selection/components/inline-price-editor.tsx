"use client"

import * as React from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Spinner } from "@/components/ui/spinner"
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

/** 表格内售价在回车或失焦时保存；结果未知时固定原请求主动确认，按 SKU 局部 loading。 */
export function InlinePriceEditor({
    id,
    skuId,
    name,
    price,
    disabled = false,
    saving = false,
    context,
}: {
    id: string
    skuId: string
    name: string
    price: string
    disabled?: boolean
    saving?: boolean
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
    const locked = disabled || saving
    const form = useAppForm({
        defaultValues: { price },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (locked) return
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
    }, [context.batchId, context.version, price, locked, form])

    return (
        <form
            className="grid min-w-36 gap-1 text-left"
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                if (!locked && !form.state.isSubmitting)
                    void form.handleSubmit()
            }}
        >
            <div className="flex items-center gap-1.5">
                <form.AppField
                    name="price"
                    children={(field) => (
                        <field.TextField
                            id={id}
                            label={`${name}客户售价`}
                            hideLabel
                            type="text"
                            inputMode="decimal"
                            disabled={locked || uncertain}
                            inputClassName="text-right num"
                            aria-describedby={`${id}-save-hint`}
                            onBlur={() => {
                                if (
                                    !locked &&
                                    !uncertain &&
                                    !form.state.isSubmitting
                                ) {
                                    void form.handleSubmit()
                                }
                            }}
                        />
                    )}
                />
                {saving && !uncertain ? (
                    <Spinner
                        className="size-3.5 shrink-0 text-muted-foreground"
                        aria-label="正在保存客户售价"
                    />
                ) : null}
                {uncertain ? (
                    <form.AppForm>
                        <form.SubmitButton
                            id={`${id}-confirm`}
                            size="sm"
                            variant="outline"
                            label="确认结果"
                            pendingLabel="确认中…"
                            loading={saving}
                            disabled={locked}
                        />
                    </form.AppForm>
                ) : null}
            </div>
            <span id={`${id}-save-hint`} className="sr-only">
                按回车或离开输入框保存客户售价。
            </span>
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
