"use client"

import * as React from "react"

/** `/` 聚焦结算列表搜索框；输入控件内不抢焦点。 */
export function useSettlementListSearchHotkey() {
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
                return
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            // 弹窗 / 抽屉打开时不得聚焦背景搜索框
            if (document.querySelector('[role="dialog"], [data-slot="sheet"]')) {
                return
            }
            event.preventDefault()
            document
                .querySelector<HTMLInputElement>(
                    '[data-slot="settlement-list-search"]',
                )
                ?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])
}
