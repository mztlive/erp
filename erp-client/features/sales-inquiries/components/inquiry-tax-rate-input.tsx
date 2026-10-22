"use client"

import * as React from "react"
import { Input } from "@/components/ui/input"
import {
    compactFixed,
    formatScaled,
    multiplyFixed,
    parseDecimal,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

function displayPercent(rate: string) {
    if (!rate) return ""
    try {
        return compactFixed(
            multiplyFixed(rate, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 4,
            }),
        )
    } catch {
        return rate
    }
}

/** Displays percentage points while keeping the form's decimal-ratio contract. */
export function InquiryTaxRateInput({
    value,
    onValueChange,
    className,
    onBlur,
    ...props
}: Omit<React.ComponentProps<typeof Input>, "value" | "onChange" | "type"> & {
    value: string
    onValueChange: (rate: string) => void
}) {
    // Preserve intermediate typing such as "13."; bulk updates replace this draft.
    const [draft, setDraft] = React.useState(() => ({
        rate: value,
        text: displayPercent(value),
    }))
    if (draft.rate !== value) {
        setDraft({ rate: value, text: displayPercent(value) })
    }
    return (
        <div className="relative min-w-0">
            <Input
                {...props}
                type="text"
                inputMode="decimal"
                className={cn("pr-7 tabular-nums", className)}
                value={draft.text}
                onChange={(event) => {
                    const text = event.target.value.trim().replace(/%$/, "")
                    if (!/^\d*(?:\.\d{0,4})?$/.test(text)) return
                    let rate = ""
                    if (text !== "") {
                        const normalized = text
                            .replace(/^0+(?=\d)/, "")
                            .replace(/^\./, "0.")
                            .replace(/\.$/, "")
                        const decimal = parseDecimal(normalized, {
                            maxScale: 4,
                        })
                        rate = compactFixed(
                            formatScaled(decimal.unscaled, decimal.scale + 2),
                        )
                    }
                    setDraft({ rate, text })
                    onValueChange(rate)
                }}
                onBlur={(event) => {
                    setDraft({ rate: value, text: displayPercent(value) })
                    onBlur?.(event)
                }}
            />
            <span
                aria-hidden="true"
                className="pointer-events-none absolute inset-y-0 right-2 flex items-center text-sm text-muted-foreground"
            >
                %
            </span>
        </div>
    )
}
