/**
 * 发起选品成功后，把视觉反馈投到侧栏「选品册」。
 * 动画坐标计算与订阅放在这里，便于单测且不依赖 React。
 */

export const SELECTION_NAV_TARGET = "sales-selection"

export type SelectionNavPoint = {
    x: number
    y: number
}

type SelectionNavLaunchListener = (origin: SelectionNavPoint) => void

const listeners = new Set<SelectionNavLaunchListener>()

/**
 * 订阅一次投递。返回取消订阅函数。
 * @param listener 收到起点后由 overlay 播放动画
 */
export function subscribeSelectionNavLaunch(
    listener: SelectionNavLaunchListener,
): () => void {
    listeners.add(listener)
    return () => {
        listeners.delete(listener)
    }
}

/**
 * 从按钮中心发出投递；没有元素时退化为视口中上部。
 * @param element 发起选品按钮；关闭弹窗后仍在商品池工具条上
 */
export function launchSelectionNavFrom(element: Element | null): void {
    const origin = element
        ? rectCenter(element.getBoundingClientRect())
        : fallbackOrigin()
    for (const listener of listeners) listener(origin)
}

/**
 * 矩形中心，用作飞行起点或菜单落点。
 * @param rect 元素的视口矩形
 */
export function rectCenter(rect: DOMRect): SelectionNavPoint {
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
}

/**
 * 二次贝塞尔控制点：斜向目标并上弧，避免直线穿内容。
 * @param origin 起点
 * @param target 落点
 */
export function launchControlPoint(
    origin: SelectionNavPoint,
    target: SelectionNavPoint,
): SelectionNavPoint {
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

function fallbackOrigin(): SelectionNavPoint {
    if (typeof window === "undefined") return { x: 0, y: 0 }
    return { x: window.innerWidth * 0.5, y: window.innerHeight * 0.4 }
}
