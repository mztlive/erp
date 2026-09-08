"use client"

import * as React from "react"

export type UsePurchaseOrdersListKeyboardOptions = {
    createOpen: boolean
}

/**
 * 列表搜索快捷键：/ 聚焦搜索框。
 * / 聚焦忽略输入框、文本域、弹层（Dialog/Sheet）与建单弹框打开场景。
 */
export function usePurchaseOrdersListKeyboard({
    createOpen,
}: UsePurchaseOrdersListKeyboardOptions) {
    React.useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            const isTypingTarget =
                target != null &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)

            if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
                if (isTypingTarget || createOpen) return
                if (
                    document.querySelector(
                        '[role="dialog"], [data-slot="sheet"]',
                    )
                ) {
                    return
                }
                event.preventDefault()
                document
                    .querySelector<HTMLInputElement>(
                        '[data-slot="po-list-search"]',
                    )
                    ?.focus()
                return
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [createOpen])
}
