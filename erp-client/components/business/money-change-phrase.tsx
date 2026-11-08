"use client"

import { compareDecimal } from "@/lib/fixed-decimal"

import { MoneyValue } from "./values"

/** 变更后金额相对当前生效版本的增减。delta 为空时不渲染。 */
export function MoneyChangePhrase({ delta }: { delta: string | null }) {
    if (delta === null) return null
    const direction = compareDecimal(delta, "0", 2)
    return (
        <>
            <span className="mx-1.5">·</span>
            {direction === 0 ? (
                "金额未变"
            ) : (
                <>
                    {direction > 0 ? "增加 " : "减少 "}
                    <MoneyValue
                        value={delta.replace(/^-/, "")}
                        className="text-foreground"
                    />
                </>
            )}
        </>
    )
}
