"use client"

import * as React from "react"

/**
 * 表单有未保存输入时拦截页面卸载/刷新，避免静默丢输入。
 */
export function useCustomerFormDirtyGuard(dirty: boolean) {
    React.useEffect(() => {
        if (!dirty) return
        const onBeforeUnload = (e: BeforeUnloadEvent) => {
            e.preventDefault()
            e.returnValue = "当前输入尚未提交，刷新后将丢失。"
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    }, [dirty])
}
