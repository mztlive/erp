"use client"

import * as React from "react"

/** 未保存离开保护：刷新 / 关闭标签页 / 返回列表。 */
export function useProductDirtyGuard(getIsDirty: () => boolean) {
    React.useEffect(() => {
        const onBeforeUnload = (event: BeforeUnloadEvent) => {
            if (getIsDirty()) {
                event.preventDefault()
            }
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅挂载时注册一次
    }, [])
}
