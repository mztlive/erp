import * as React from "react"

import type {
    IntegrationFormalResult,
    IntegrationResolutionItemView,
} from "../../types"

export function useIntegrationActionFocus({
    item,
    lastResult,
}: {
    item: IntegrationResolutionItemView | undefined
    lastResult: IntegrationFormalResult | null
}) {
    const resultRef = React.useRef<HTMLDivElement>(null)
    const headingRef = React.useRef<HTMLHeadingElement>(null)
    const actionZoneRef = React.useRef<HTMLDivElement>(null)

    const focusFirstAction = React.useCallback(() => {
        actionZoneRef.current?.scrollIntoView({
            behavior: "smooth",
            block: "start",
        })
        window.setTimeout(() => {
            const zone = actionZoneRef.current
            const btn = zone?.querySelector<HTMLButtonElement>(
                "button:not([disabled])",
            )
            if (btn) {
                btn.focus()
            } else {
                headingRef.current?.focus()
            }
        }, 250)
    }, [])

    React.useEffect(() => {
        if (lastResult) resultRef.current?.focus()
        else if (item) headingRef.current?.focus()
    }, [item, lastResult])

    return { resultRef, headingRef, actionZoneRef, focusFirstAction }
}
