"use client"

import * as React from "react"

export type ProcurementKeyboardShortcutsOptions = {
    /** 当前任务允许的动作；⌘↵ 仅当包含 APPROVE 时打开通过确认。 */
    allowedActions: readonly string[] | undefined
    searchInputRef: React.RefObject<HTMLInputElement | null>
    onSave: () => void
    onConfirmApprove: () => void
    /** 先做脏检查再切换：delta 为 1 / -1 */
    onNavigate: (delta: 1 | -1) => void
}

/**
 * 键盘：无输入焦点时 j/k 切换；⌘S 保存；⌘↵ 打开通过；/ 聚焦单号搜索。
 */
export function useProcurementKeyboardShortcuts({
    allowedActions,
    searchInputRef,
    onSave,
    onConfirmApprove,
    onNavigate,
}: ProcurementKeyboardShortcutsOptions) {
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
                event.key.toLowerCase() === "s"
            ) {
                event.preventDefault()
                onSave()
                return
            }
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key === "Enter" &&
                !inField
            ) {
                event.preventDefault()
                if (allowedActions?.includes("APPROVE")) {
                    onConfirmApprove()
                }
                return
            }
            if (inField) return
            if (event.key === "/") {
                event.preventDefault()
                searchInputRef.current?.focus()
                return
            }
            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                onNavigate(1)
                return
            }
            if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                onNavigate(-1)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [allowedActions, onSave, onConfirmApprove, onNavigate, searchInputRef])
}
