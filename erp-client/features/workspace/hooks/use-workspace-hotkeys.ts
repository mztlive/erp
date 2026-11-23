"use client"

import { useEffect, useRef } from "react"

import {
    WORKSPACE_HOTKEY_ACTION_EVENT,
    WORKSPACE_HOTKEY_CHEATSHEET_EVENT,
    dispatchWorkspaceHotkeyAction,
    dispatchWorkspaceHotkeyCheatSheetToggle,
    isWorkspaceFrameActive,
    subscribeWorkspaceHotkeyAction,
    type WorkspaceHotkeyAction,
} from "@/lib/workspace-hotkeys"

export {
    WorkspaceHotkeyCheatSheetButton,
    type WorkspaceHotkeyCheatSheetButtonProps,
    type WorkspaceHotkeyCheatSheetScope,
} from "../components/workspace-hotkey-cheat-sheet"

export {
    WORKSPACE_HOTKEY_ACTION_EVENT,
    WORKSPACE_HOTKEY_CHEATSHEET_EVENT,
    dispatchWorkspaceHotkeyAction,
    dispatchWorkspaceHotkeyCheatSheetToggle,
    isWorkspaceFrameActive,
    subscribeWorkspaceHotkeyAction,
    type WorkspaceHotkeyAction,
}

const EDITABLE_TARGET_SELECTOR =
    'input, textarea, select, [contenteditable="true"], [role="textbox"]'

const TASK_ACTION_SHEET_SELECTOR =
    '[data-business-component="task-action-sheet"]:not([data-closed])'

const BLOCKING_OVERLAY_SELECTOR = [
    '[role="dialog"]:not([data-closed]):not([data-slot="toast"]):not([data-business-component="task-action-sheet"])',
    '[role="alertdialog"]:not([data-closed])',
    '[role="menu"]:not([data-closed])',
    '[role="listbox"]:not([data-closed]):not([data-slot="table-container"])',
    '[data-slot="combobox-content"][data-open]',
    '[data-slot="select-content"][data-open]',
    '[data-slot="dropdown-menu-content"][data-open]',
    '[data-slot="popover-content"][data-open]',
].join(", ")

const ARROW_WIDGET_SELECTOR = [
    '[role="combobox"]',
    '[role="menuitem"]',
    '[role="option"]',
    '[role="slider"]',
    '[role="tree"]',
].join(", ")

function isEditableElement(target: EventTarget | null): boolean {
    if (!(target instanceof Element)) return false
    if (target instanceof HTMLElement && target.isContentEditable) return true
    return target.closest(EDITABLE_TARGET_SELECTOR) !== null
}

function getOpenBlockingModalElement(): Element | null {
    return document.querySelector(BLOCKING_OVERLAY_SELECTOR)
}

function hasOpenTaskActionSheet(): boolean {
    return document.querySelector(TASK_ACTION_SHEET_SELECTOR) !== null
}

function isCheatSheetOnlyModal(modal: Element | null): boolean {
    if (!modal) return false
    const modals = document.querySelectorAll(BLOCKING_OVERLAY_SELECTOR)
    return (
        modals.length === 1 &&
        modal.getAttribute("data-slot") === "workspace-hotkey-cheatsheet"
    )
}

export type UseWorkspaceHotkeysOptions = Readonly<{
    /** 是否启用快捷键监听（默认 `true`）。 */
    enabled?: boolean
    /** `J` / `ArrowDown`：切换下一项。 */
    onNext?: () => void
    /** `K` / `ArrowUp`：切换上一项。 */
    onPrev?: () => void
    /** `A`：触发或聚焦审批通过动作。 */
    onApprove?: () => void
    /** `R`：展开或聚焦审批驳回动作。 */
    onReject?: () => void
    /** `P`：开关 / 聚焦右侧附件对照面板。 */
    onMaterials?: () => void
    /** `H`：开关 / 聚焦右侧审批轨迹面板。 */
    onHistory?: () => void
    /** `]`：折叠 / 展开右侧上下文轨。 */
    onToggleRail?: () => void
    /** `/`：聚焦队列搜索框。 */
    onFocusSearch?: () => void
    /** `Esc`：收起检查器、抽屉或取消多选。 */
    onEscape?: () => void
    /** 存在本地模态浮层时是否仍允许 `Esc` 回调（默认 `false`，让渡给浮层自身关闭逻辑）。 */
    allowEscapeInModal?: boolean
    /** `?`（Shift+`/`）：开关快捷键速查弹窗；未提供时自动广播切换当前页 `WorkspaceHotkeyCheatSheetButton`。 */
    onToggleCheatSheet?: () => void
    /** 是否启用 `?` 打开速查弹窗（默认 `true`）。 */
    enableCheatSheet?: boolean
}>

/**
 * 工作台全局分诊与审批快捷键引擎（适配多 Iframe 工作区壳）：
 * - 严格校验 `window.frameElement.getAttribute("data-workspace-active") === "true"`，后台隐藏页签绝不误触；
 * - 在 IME 组合输入、组合修饰键、表单可编辑控件或模态弹层打开期间自动让渡。
 */
export function useWorkspaceHotkeys(options: UseWorkspaceHotkeysOptions): void {
    const optionsRef = useRef(options)
    optionsRef.current = options

    const enabled = options.enabled ?? true

    useEffect(() => {
        if (!enabled) return

        const onKeyDown = (event: KeyboardEvent) => {
            if (!isWorkspaceFrameActive()) return
            if (event.defaultPrevented) return
            if (event.isComposing || event.keyCode === 229) return
            if (event.metaKey || event.ctrlKey || event.altKey) return

            const current = optionsRef.current
            if (current.enabled === false) return

            const targetEditable =
                isEditableElement(event.target) ||
                isEditableElement(document.activeElement)
            const openBlockingModal = getOpenBlockingModalElement()
            const openTaskActionSheet = hasOpenTaskActionSheet()

            if (event.key === "Escape") {
                if (!current.onEscape) return
                if (targetEditable) return
                if (
                    (openBlockingModal || openTaskActionSheet) &&
                    !current.allowEscapeInModal
                ) {
                    return
                }
                event.preventDefault()
                current.onEscape()
                return
            }

            if (targetEditable) return

            // `?` 允许在速查弹窗自身打开时再次按下直接关闭，也允许在轻作业抽屉打开时查看快捷键；
            // 其他阻塞模态层（弹窗/菜单/下拉/气泡）打开时让渡。
            if (event.key === "?") {
                if (current.enableCheatSheet === false) return
                if (
                    openBlockingModal &&
                    !isCheatSheetOnlyModal(openBlockingModal)
                ) {
                    return
                }
                if (event.repeat) return
                event.preventDefault()
                if (current.onToggleCheatSheet) {
                    current.onToggleCheatSheet()
                } else {
                    dispatchWorkspaceHotkeyCheatSheetToggle()
                }
                return
            }

            if (openBlockingModal) return

            if (event.key === "/" && !event.shiftKey) {
                if (openTaskActionSheet) return
                if (!current.onFocusSearch) return
                if (event.repeat) return
                event.preventDefault()
                current.onFocusSearch()
                return
            }

            if (event.shiftKey) return

            const key = event.key.toLowerCase()

            if (key === "j" || event.key === "ArrowDown") {
                if (!current.onNext) return
                if (
                    event.key === "ArrowDown" &&
                    event.target instanceof Element &&
                    event.target.closest(ARROW_WIDGET_SELECTOR)
                ) {
                    return
                }
                if (event.repeat) return
                event.preventDefault()
                current.onNext()
                return
            }

            if (key === "k" || event.key === "ArrowUp") {
                if (!current.onPrev) return
                if (
                    event.key === "ArrowUp" &&
                    event.target instanceof Element &&
                    event.target.closest(ARROW_WIDGET_SELECTOR)
                ) {
                    return
                }
                if (event.repeat) return
                event.preventDefault()
                current.onPrev()
                return
            }

            if (key === "a") {
                if (!current.onApprove || event.repeat) return
                event.preventDefault()
                current.onApprove()
                return
            }

            if (key === "r") {
                if (!current.onReject || event.repeat) return
                event.preventDefault()
                current.onReject()
                return
            }

            if (key === "p") {
                if (!current.onMaterials || event.repeat) return
                event.preventDefault()
                current.onMaterials()
                return
            }

            if (key === "h") {
                if (!current.onHistory || event.repeat) return
                event.preventDefault()
                current.onHistory()
                return
            }

            if (event.key === "]") {
                if (!current.onToggleRail || event.repeat) return
                event.preventDefault()
                current.onToggleRail()
            }
        }

        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [enabled])
}
