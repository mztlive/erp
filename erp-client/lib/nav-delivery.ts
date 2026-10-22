/**
 * 提交成功后的「已投递」动画：把任务沿二次贝塞尔曲线送到侧栏对应入口。
 * 坐标计算与订阅保持纯函数，便于单测且不依赖 React。
 */

export type NavDeliveryPoint = {
    x: number
    y: number
}

/** 投递目标：决定落点的侧栏入口与飞行图标。 */
export type NavDeliveryKind =
    | "selection-booklet"
    | "background-task"
    | "file-task"
    | "portal-task"

/** 侧栏入口的 `data-workspace-nav` 标识（与入口 href 的自动化 id 段一致）。 */
export const NAV_DELIVERY_TARGETS: Record<NavDeliveryKind, string> = {
    "selection-booklet": "sales-selection",
    "background-task": "governance-background-jobs",
    "file-task": "account-data-transfers",
    "portal-task": "supplier-portal-tasks",
}

export type NavDeliveryRequest = {
    kind: NavDeliveryKind
    origin: NavDeliveryPoint
    animate?: boolean
    task?: { id: string; jobNo: string }
}

type NavDeliveryListener = (request: NavDeliveryRequest) => void

const listeners = new Set<NavDeliveryListener>()

/**
 * 订阅一次投递。返回取消订阅函数。
 * @param listener 收到起点后由 overlay 播放动画
 */
export function subscribeNavDelivery(
    listener: NavDeliveryListener,
): () => void {
    listeners.add(listener)
    return () => {
        listeners.delete(listener)
    }
}

/**
 * 从触发元素中心发出投递；没有元素时退化为视口中上部。
 * @param kind 投递目标类型
 * @param element 触发提交的按钮；弹窗关闭后仍在页面上
 */
export function launchNavDelivery(
    kind: NavDeliveryKind,
    element: Element | null,
): void {
    const origin = element
        ? rectCenter(element.getBoundingClientRect())
        : fallbackOrigin()
    dispatchNavDelivery({ kind, origin })
}

/** Deliver a request whose coordinates are already relative to this document. */
export function dispatchNavDelivery(request: NavDeliveryRequest): boolean {
    for (const listener of listeners) listener(request)
    return listeners.size > 0
}

/**
 * 矩形中心，用作飞行起点或菜单落点。
 * @param rect 元素的视口矩形
 */
export function rectCenter(rect: DOMRect): NavDeliveryPoint {
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
}

/**
 * 二次贝塞尔控制点：斜向目标并上弧，避免直线穿内容。
 * @param origin 起点
 * @param target 落点
 */
export function launchControlPoint(
    origin: NavDeliveryPoint,
    target: NavDeliveryPoint,
): NavDeliveryPoint {
    return {
        x: origin.x + (target.x - origin.x) * 0.35,
        y: Math.min(origin.y, target.y) - 120,
    }
}

/**
 * 二次贝塞尔上参数 t 的标量位置。
 * @param start 起点
 * @param control 控制点
 * @param end 终点
 * @param t 0 到 1
 */
export function quadraticBezier(
    start: number,
    control: number,
    end: number,
    t: number,
): number {
    const remaining = 1 - t
    return (
        remaining * remaining * start +
        2 * remaining * t * control +
        t * t * end
    )
}

function fallbackOrigin(): NavDeliveryPoint {
    if (typeof window === "undefined") return { x: 0, y: 0 }
    return { x: window.innerWidth * 0.5, y: window.innerHeight * 0.4 }
}

let recentOrigin: { point: NavDeliveryPoint; recordedAt: number } | null = null

/** 在业务事件执行前记录按钮或菜单项位置，弹窗关闭后仍保留本次操作起点。 */
export function trackNavDeliveryOrigin(): () => void {
    const remember = (event: Event) => {
        if (
            event instanceof KeyboardEvent &&
            !["Enter", " "].includes(event.key)
        )
            return
        const target =
            event instanceof SubmitEvent ? event.submitter : event.target
        const element =
            target instanceof Element
                ? target.closest(
                      'button, a, [role="button"], [role="menuitem"], input[type="submit"]',
                  )
                : null
        if (!element) return
        const rect = element.getBoundingClientRect()
        if (!rect.width || !rect.height) return
        recentOrigin = {
            point: rectCenter(rect),
            recordedAt: performance.now(),
        }
    }
    document.addEventListener("pointerdown", remember, true)
    document.addEventListener("click", remember, true)
    document.addEventListener("keydown", remember, true)
    document.addEventListener("submit", remember, true)
    return () => {
        document.removeEventListener("pointerdown", remember, true)
        document.removeEventListener("click", remember, true)
        document.removeEventListener("keydown", remember, true)
        document.removeEventListener("submit", remember, true)
        recentOrigin = null
    }
}

/** 在开始异步提交前取快照；每个请求保留自己的起点，互不覆盖。 */
export function captureNavDeliveryOrigin(): NavDeliveryPoint {
    if (typeof document === "undefined") return { x: 0, y: 0 }
    if (recentOrigin && performance.now() - recentOrigin.recordedAt < 5000)
        return { ...recentOrigin.point }
    const active = document.activeElement
    if (active?.matches('button, a, [role="button"], [role="menuitem"]')) {
        const rect = active.getBoundingClientRect()
        if (rect.width && rect.height) return rectCenter(rect)
    }
    return fallbackOrigin()
}
