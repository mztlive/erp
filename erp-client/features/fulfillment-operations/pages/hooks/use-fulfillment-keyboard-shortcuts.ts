"use client"

import * as React from "react"

export type FulfillmentKeyboardShortcutsOptions = {
    /** 有未保存修改时，方向键导航会被拦截 */
    dirty: boolean
    canPost: boolean
    formalPending: boolean
    canExecute: boolean
    supportsSave: boolean
    onSave: () => void | Promise<unknown>
    onConfirm: () => void
    onNavigate: (delta: 1 | -1) => void
    onToggleShortcuts: () => void
}

/**
 * W09 处理面快捷键：
 * Ctrl/Cmd+S 保存草稿、Ctrl/Cmd+Enter 打开确认、J/K 与方向键切换单据、? 切换帮助。
 */
export function useFulfillmentKeyboardShortcuts({
    dirty,
    canPost,
    formalPending,
    canExecute,
    supportsSave,
    onSave,
    onConfirm,
    onNavigate,
    onToggleShortcuts,
}: FulfillmentKeyboardShortcutsOptions) {
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
                if (canExecute && supportsSave) void onSave()
                return
            }
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key === "Enter" &&
                !inField
            ) {
                event.preventDefault()
                if (canPost && !formalPending) onConfirm()
                return
            }
            if (inField) return
            if (event.key === "?") {
                event.preventDefault()
                onToggleShortcuts()
                return
            }
            if (
                (event.key === "ArrowDown" || event.key === "ArrowUp") &&
                target instanceof HTMLButtonElement
            ) {
                // 焦点在队列列表按钮上时保留原生滚动，不劫持方向键
                return
            }
            if (event.key === "j" || event.key === "ArrowDown") {
                event.preventDefault()
                if (dirty) return
                onNavigate(1)
            }
            if (event.key === "k" || event.key === "ArrowUp") {
                event.preventDefault()
                if (dirty) return
                onNavigate(-1)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [
        canPost,
        dirty,
        formalPending,
        onSave,
        onConfirm,
        onNavigate,
        onToggleShortcuts,
        canExecute,
        supportsSave,
    ])
}
