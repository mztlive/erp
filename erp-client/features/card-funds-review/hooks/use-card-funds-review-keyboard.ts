"use client"

import * as React from "react"

import type { CardFundsReviewItemView } from "@/features/card-funds-review/types"

/**
 * 队列页键盘导航：j/↓ 下一条、k/↑ 上一条、⌘/Ctrl ↵ 打开通过确认。
 * 证据未保存时先挂起跳转（setPendingNav），由页面弹放弃确认。
 */
export function useCardFundsReviewKeyboard(args: {
    task: CardFundsReviewItemView | undefined
    evidenceOk: boolean
    evidenceDirty: boolean
    neighborId: (delta: number) => string | undefined
    goToWorkItem: (workItemId: string | undefined | null) => void
    onShortcutSubmit: () => void
    setPendingNav: React.Dispatch<React.SetStateAction<number | null>>
}): void {
    const {
        task,
        evidenceOk,
        evidenceDirty,
        neighborId,
        goToWorkItem,
        onShortcutSubmit,
        setPendingNav,
    } = args

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            const inField =
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable

            if (
                (event.metaKey || event.ctrlKey) &&
                event.key === "Enter" &&
                !inField
            ) {
                event.preventDefault()
                onShortcutSubmit()
                return
            }
            if (inField) return
            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                const next = neighborId(1)
                if (!next) return
                if (evidenceDirty) {
                    setPendingNav(1)
                    return
                }
                goToWorkItem(next)
            }
            if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                const prev = neighborId(-1)
                if (!prev) return
                if (evidenceDirty) {
                    setPendingNav(-1)
                    return
                }
                goToWorkItem(prev)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [
        task,
        evidenceOk,
        evidenceDirty,
        neighborId,
        goToWorkItem,
        onShortcutSubmit,
        setPendingNav,
    ])
}
