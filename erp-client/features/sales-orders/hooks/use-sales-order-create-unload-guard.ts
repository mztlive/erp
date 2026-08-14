"use client"

import * as React from "react"

/** 表单有未提交输入时拦截刷新/关闭，避免输入丢失。 */
export function useSalesOrderCreateUnloadGuard(dirty: boolean): void {
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
