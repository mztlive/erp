"use client"

/**
 * 工作台全局快捷键事件桥与多 Iframe 激活态守卫。
 * 放在 `lib/` 以避免 `features/approval-workflow` 与 `features/workspace` 之间的循环依赖。
 */

export type WorkspaceHotkeyAction =
    | "approve"
    | "reject"
    | "materials"
    | "history"
    | "toggle-rail"

export const WORKSPACE_HOTKEY_ACTION_EVENT = "erp-workspace-hotkey-action"
export const WORKSPACE_HOTKEY_CHEATSHEET_EVENT =
    "erp-workspace-hotkey-cheatsheet"

/**
 * 多 Iframe 激活态守卫：
 * - 运行在工作台子 iframe 内时，仅当宿主将该 iframe 标记为 `data-workspace-active="true"` 才响应快捷键；
 * - 独立顶层页面（无 `window.frameElement`）始终视为激活。
 */
export function isWorkspaceFrameActive(): boolean {
    if (typeof window === "undefined") return false
    try {
        const frame = window.frameElement
        if (frame) {
            return frame.getAttribute("data-workspace-active") === "true"
        }
    } catch {
        return false
    }
    return true
}

/** 在当前激活工作台页内广播一条语义化快捷动作。 */
export function dispatchWorkspaceHotkeyAction(
    action: WorkspaceHotkeyAction,
): void {
    if (typeof window === "undefined" || !isWorkspaceFrameActive()) return
    window.dispatchEvent(
        new CustomEvent<{ action: WorkspaceHotkeyAction }>(
            WORKSPACE_HOTKEY_ACTION_EVENT,
            { detail: { action } },
        ),
    )
}

/** 订阅当前工作台页内的语义化快捷动作。 */
export function subscribeWorkspaceHotkeyAction(
    listener: (action: WorkspaceHotkeyAction) => void,
): () => void {
    if (typeof window === "undefined") return () => {}
    const onEvent = (event: Event) => {
        if (!isWorkspaceFrameActive()) return
        const action = (
            event as CustomEvent<{ action?: WorkspaceHotkeyAction }>
        ).detail?.action
        if (action) listener(action)
    }
    window.addEventListener(WORKSPACE_HOTKEY_ACTION_EVENT, onEvent)
    return () =>
        window.removeEventListener(WORKSPACE_HOTKEY_ACTION_EVENT, onEvent)
}

/** 切换当前页内的快捷键速查弹窗。 */
export function dispatchWorkspaceHotkeyCheatSheetToggle(): void {
    if (typeof window === "undefined" || !isWorkspaceFrameActive()) return
    window.dispatchEvent(new Event(WORKSPACE_HOTKEY_CHEATSHEET_EVENT))
}

/** 订阅当前页内的快捷键速查弹窗切换事件。 */
export function subscribeWorkspaceHotkeyCheatSheetToggle(
    listener: () => void,
): () => void {
    if (typeof window === "undefined") return () => {}
    const onEvent = () => {
        if (!isWorkspaceFrameActive()) return
        listener()
    }
    window.addEventListener(WORKSPACE_HOTKEY_CHEATSHEET_EVENT, onEvent)
    return () =>
        window.removeEventListener(WORKSPACE_HOTKEY_CHEATSHEET_EVENT, onEvent)
}
