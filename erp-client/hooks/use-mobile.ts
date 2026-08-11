/**
 * 响应式断点 hook：判断当前视口是否为移动端，供布局按断点切换。
 */
import * as React from "react"

/** 移动端断点宽度（px），与桌面布局切换保持一致。 */
const MOBILE_BREAKPOINT = 1024
/** matchMedia 查询串：视口宽度小于等于断点减 1 即视为移动端。 */
const MOBILE_QUERY = `(max-width: ${MOBILE_BREAKPOINT - 1}px)`

/**
 * 订阅移动端查询结果变化，供 useSyncExternalStore 使用。
 *
 * @param onStoreChange 查询结果变化时触发的回调。
 * @returns 取消订阅函数。
 */
function subscribe(onStoreChange: () => void) {
    const mediaQuery = window.matchMedia(MOBILE_QUERY)
    mediaQuery.addEventListener("change", onStoreChange)

    return () => mediaQuery.removeEventListener("change", onStoreChange)
}

/** 读取当前视口是否匹配移动端查询。 */
function getSnapshot() {
    return window.matchMedia(MOBILE_QUERY).matches
}

/**
 * 返回当前视口是否为移动端（宽度 < MOBILE_BREAKPOINT）。
 *
 * 视口变化时自动触发重渲染；SSR 服务端快照固定返回 false。
 */
export function useIsMobile() {
    return React.useSyncExternalStore(subscribe, getSnapshot, () => false)
}
