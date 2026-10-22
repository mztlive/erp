"use client"

import { useEffect } from "react"

type GuardedForm = {
    state: { isDefaultValue: boolean; isSubmitting: boolean }
    store: { subscribe: (listener: () => void) => { unsubscribe: () => void } }
}

const forms = new Map<GuardedForm, { dirty: boolean; busy: boolean }>()

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
                window.dispatchEvent(new Event("erp-workspace-form-state"))
            }
        }
        update()
        const unsubscribe = form.store.subscribe(update)
        return () => {
            unsubscribe.unsubscribe()
            forms.delete(form)
            window.dispatchEvent(new Event("erp-workspace-form-state"))
        }
    }, [form])
}
