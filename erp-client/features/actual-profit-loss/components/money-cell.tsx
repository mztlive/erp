import Link from "next/link"

import { formatMoneyDisplay } from "@/features/actual-profit-loss/lib/presentation"
import { compareDecimal } from "@/lib/fixed-decimal"

export function MoneyCell({
    id,
    value,
    negativeAsText = true,
    href,
    onClick,
    ariaLabel,
}: {
    id?: string
    value: string | undefined
    negativeAsText?: boolean
    href?: string
    onClick?: () => void
    ariaLabel?: string
}) {
    const display = formatMoneyDisplay(value)
    let isNeg = false
    if (negativeAsText && value != null && value !== "—") {
        try {
            isNeg = compareDecimal(value, "0", 6) < 0
        } catch {
            isNeg = false
        }
    }
    const content = (
        <span
            className={`num text-sm ${isNeg ? "text-destructive" : ""}`}
            aria-label={
                ariaLabel ??
                (value == null
                    ? "金额不可用"
                    : `人民币 ${display}，不含税${isNeg ? "，负值" : ""}`)
            }
        >
            {isNeg ? `亏损 ${display}` : display}
        </span>
    )
    if (onClick) {
        return (
            <button
                id={id}
                type="button"
                className="text-left underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                onClick={onClick}
            >
                {content}
            </button>
        )
    }
    if (href) {
        return (
            <Link
                id={id}
                href={href}
                className="underline-offset-2 hover:underline"
                target="_blank"
                rel="noreferrer"
            >
                {content}
            </Link>
        )
    }
    return content
}
