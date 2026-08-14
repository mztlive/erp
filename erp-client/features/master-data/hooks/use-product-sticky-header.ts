"use client"

import * as React from "react"

/**
 * 商品编辑页吸顶卡片高度测量；分区锚点需额外留一点空隙避免贴边。
 * 依赖商品身份变化时重新测量（新建/加载完成切换时卡片结构变化）。
 */
export function useProductStickyHeader(
    isCreate: boolean,
    stableId: string | undefined,
    lockVersion: number | null | undefined,
) {
    const stickyHeaderRef = React.useRef<HTMLElement>(null)
    const [stickyHeaderHeight, setStickyHeaderHeight] = React.useState(160)

    React.useLayoutEffect(() => {
        const el = stickyHeaderRef.current
        if (!el) return
        const update = () => {
            setStickyHeaderHeight(Math.ceil(el.getBoundingClientRect().height))
        }
        update()
        const observer = new ResizeObserver(update)
        observer.observe(el)
        return () => observer.disconnect()
    }, [isCreate, stableId, lockVersion])

    const sectionScrollMarginPx = stickyHeaderHeight + 12

    return { stickyHeaderRef, stickyHeaderHeight, sectionScrollMarginPx }
}
