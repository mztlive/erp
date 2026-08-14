import * as React from "react"

import type { IntegrationResolutionItemView } from "../../types"
import { derivePosition, type IntegrationItemPosition } from "../lib/selection"

export function useIntegrationItemNavigation({
    items,
    queueItems,
    item,
    focusMode,
    replaceUrl,
    onBeforeNavigate,
}: {
    items: IntegrationResolutionItemView[]
    queueItems: IntegrationResolutionItemView[]
    item: IntegrationResolutionItemView | undefined
    focusMode: boolean
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
    onBeforeNavigate: () => void
}): IntegrationItemPosition & {
    goToItem: (next: IntegrationResolutionItemView | null | undefined) => void
    neighbor: (delta: number) => IntegrationResolutionItemView | null
} {
    const position = React.useMemo(
        () => derivePosition(item, items, queueItems, focusMode),
        [item, items, queueItems, focusMode],
    )

    const goToItem = React.useCallback(
        (next: IntegrationResolutionItemView | null | undefined) => {
            onBeforeNavigate()
            if (!next) {
                replaceUrl({ taskId: null, differenceId: null })
                return
            }
            if (next.identity.itemType === "ERROR_TASK") {
                replaceUrl({
                    taskId: next.identity.id,
                    differenceId: null,
                })
            } else {
                replaceUrl({
                    differenceId: next.identity.id,
                    taskId: null,
                })
            }
        },
        [onBeforeNavigate, replaceUrl],
    )

    const neighbor = React.useCallback(
        (delta: number) => {
            const idx = position.currentIndex + delta
            if (idx < 0 || idx >= items.length) return null
            return items[idx] ?? null
        },
        [position.currentIndex, items],
    )

    return { ...position, goToItem, neighbor }
}
