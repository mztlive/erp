"use client"

import * as React from "react"

import type { SettlementsUrlState } from "@/features/supplier-settlements/lib/url-state"

/**
 * 结算中心快捷键：按 d 直达差异处理。
 * 输入控件与 contentEditable 内不拦截；配合 command/ctrl 时不触发。
 */
function useSettlementSectionHotkey(
    patchUrl: (patch: Partial<SettlementsUrlState>) => void,
) {
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            if (
                target &&
                (target.tagName === "INPUT" ||
                    target.tagName === "TEXTAREA" ||
                    target.tagName === "SELECT" ||
                    target.isContentEditable)
            ) {
                return
            }
            if (event.key === "d" && !event.metaKey && !event.ctrlKey) {
                event.preventDefault()
                patchUrl({ section: "differences" })
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [patchUrl])
}

export { useSettlementSectionHotkey }
