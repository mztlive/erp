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
            const btn = Array.from(
                zone?.querySelectorAll<HTMLElement>(
                    "button:not([disabled]), summary",
                ) ?? [],
            ).find((element) => element.checkVisibility())
            if (btn) {
                btn.focus()
            } else {
                headingRef.current?.focus()
            }
        }, 250)
    }, [])

    const itemId = item?.identity.id
    const prevItemIdRef = React.useRef<string | undefined>(undefined)
    const prevResultRef = React.useRef<IntegrationFormalResult | null>(null)

    React.useEffect(() => {
        if (lastResult && lastResult !== prevResultRef.current) {
            prevResultRef.current = lastResult
            resultRef.current?.focus()
            return
        }
        if (!lastResult) {
            prevResultRef.current = null
        }
        if (itemId && itemId !== prevItemIdRef.current) {
            prevItemIdRef.current = itemId
            headingRef.current?.focus()
        } else if (!itemId) {
            prevItemIdRef.current = undefined
        }
    }, [itemId, lastResult])

    return { resultRef, headingRef, actionZoneRef, focusFirstAction }
}
