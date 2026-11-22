"use client"

import { useEffect, useRef } from "react"
import { isWorkspaceFrame } from "@/lib/workspace-tabs"

type GuardedForm = {
    state: { isDefaultValue: boolean; isSubmitting: boolean }
    store: { subscribe: (listener: () => void) => { unsubscribe: () => void } }
}

const forms = new Map<object, { dirty: boolean; busy: boolean }>()

function notifyWorkspaceFormState(): void {
    window.dispatchEvent(new Event("erp-workspace-form-state"))
}

export function workspaceFormState(): { dirty: boolean; busy: boolean } {
    return {
        dirty: [...forms.values()].some((form) => form.dirty),
        busy: [...forms.values()].some((form) => form.busy),
    }
}

/** Conservative guard: a successful form reset establishes the next saved baseline. */
export function useWorkspaceFormGuard(form: GuardedForm): void {
    useEffect(() => {
        const update = () => {
            const previous = forms.get(form)
            const next = {
                dirty: !form.state.isDefaultValue,
                busy: form.state.isSubmitting,
            }
            forms.set(form, next)
            if (
                previous?.dirty !== next.dirty ||
                previous?.busy !== next.busy
            ) {
                notifyWorkspaceFormState()
            }
        }
        update()
        const unsubscribe = form.store.subscribe(update)
        return () => {
            unsubscribe.unsubscribe()
            forms.delete(form)
            notifyWorkspaceFormState()
        }
    }, [form])
}

/**
 * 通用脏状态上报 Hook：适用于未走 `useAppForm` 或基于业务字段 diff 判定脏态的详情编辑页。
 * 同步点亮顶部工作页签琥珀未保存圆点与宿主页签关闭拦截，并在独立打开态自动挂载 `beforeunload`。
 */
export function useWorkspaceDirtyState({
    dirty,
    busy = false,
    warnBeforeUnload = true,
}: {
    dirty: boolean
    busy?: boolean
    warnBeforeUnload?: boolean
}): void {
    const tokenRef = useRef<object | null>(null)
    if (tokenRef.current === null) {
        tokenRef.current = {}
    }

    useEffect(() => {
        const token = tokenRef.current
        if (!token) return
        const previous = forms.get(token)
        const next = { dirty, busy }
        forms.set(token, next)
        if (previous?.dirty !== next.dirty || previous?.busy !== next.busy) {
            notifyWorkspaceFormState()
        }
    }, [dirty, busy])

    useEffect(() => {
        const token = tokenRef.current
        return () => {
            if (!token) return
            forms.delete(token)
            notifyWorkspaceFormState()
        }
    }, [])

    useEffect(() => {
        if (!dirty || !warnBeforeUnload || isWorkspaceFrame()) return
        const handleBeforeUnload = (event: BeforeUnloadEvent) => {
            event.preventDefault()
            event.returnValue = ""
        }
        window.addEventListener("beforeunload", handleBeforeUnload)
        return () =>
            window.removeEventListener("beforeunload", handleBeforeUnload)
    }, [dirty, warnBeforeUnload])
}
