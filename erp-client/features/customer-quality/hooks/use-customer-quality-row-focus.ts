"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import type { CustomerQualityView } from "../types"

export function useCustomerQualityRowFocus({
    focusCustomerId,
    focusMetric,
    data,
    scrollToTableTop,
}: {
    focusCustomerId?: string
    focusMetric?: string
    data?: CustomerQualityView
    scrollToTableTop: () => void
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    // Restore row focus after returning from drill
    React.useEffect(() => {
        if (!focusCustomerId || !data) return
        const frame = window.requestAnimationFrame(() => {
            window.requestAnimationFrame(() => {
                const row = document.querySelector<HTMLElement>(
                    `[data-customer-row="${CSS.escape(focusCustomerId)}"]`,
                )
                const metricTarget = focusMetric
                    ? document.querySelector<HTMLElement>(
                          `[data-customer-id="${CSS.escape(focusCustomerId)}"][data-focus-metric="${CSS.escape(focusMetric)}"]`,
                      )
                    : null
                if (metricTarget ?? row) {
                    ;(metricTarget ?? row)?.focus()
                } else {
                    scrollToTableTop()
                }
            })
        })
        // D3：focus 参数用完即清理（replace，不滞留 URL；重新进入/分享时不再无谓定位）
        const sp = new URLSearchParams(searchParams.toString())
        if (sp.has("focusCustomerId") || sp.has("focusMetric")) {
            sp.delete("focusCustomerId")
            sp.delete("focusMetric")
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        }
        return () => window.cancelAnimationFrame(frame)
    }, [
        focusCustomerId,
        focusMetric,
        data,
        scrollToTableTop,
        pathname,
        router,
        searchParams,
    ])
}
