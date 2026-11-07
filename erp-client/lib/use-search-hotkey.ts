"use client"

import * as React from "react"

/** 浮层打开时不抢背景搜索框焦点；各列表页共用同一组判定。 */
const DEFAULT_OVERLAY_SELECTOR = '[role="dialog"], [data-slot="sheet"]'

type SearchHotkeyTarget = React.RefObject<HTMLInputElement | null> | string

export interface SearchHotkeyOptions {
    /**
     * 自定义浮层选择器；默认拦截 Dialog / Sheet。传 `null` 可关闭浮层判定。
     */
    overlaySelector?: string | null
}

function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false
    return (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable
    )
}

/**
 * `/` 聚焦列表搜索框。
 *
 * 输入控件内输入、带修饰键或 Dialog / Sheet 打开时不抢焦点。搜索框由父组件
 * 持有 ref 时传 ref；只能按 DOM 选择器定位时传选择器字符串。
 *
 * @param target 搜索输入框 ref 或 CSS 选择器。
 * @param options 浮层判定配置。
 */
export function useSearchHotkey(
    target: SearchHotkeyTarget,
    options: SearchHotkeyOptions = {},
) {
    const overlaySelector =
        options.overlaySelector === undefined
            ? DEFAULT_OVERLAY_SELECTOR
            : options.overlaySelector

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            if (isEditableTarget(event.target)) return
            if (overlaySelector && document.querySelector(overlaySelector)) {
                return
            }
            event.preventDefault()
            const input =
                typeof target === "string"
                    ? document.querySelector<HTMLInputElement>(target)
                    : target.current
            input?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [overlaySelector, target])
}
