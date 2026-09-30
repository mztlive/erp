"use client"
import * as React from "react"
import type { PersonScopeInput } from "../api/person-data-scopes"

export type PersonalGrantDraft = {
    values: PersonScopeInput
    policyVersion: number
}

/** 保留用户输入而非服务端缓存；权限清缓存卸载表单时仍保留原始保存版本。 */
export function usePersonalGrantDraft(accountId: string) {
    const [drafts, setDrafts] = React.useState<
        Record<string, PersonalGrantDraft | undefined>
    >({})
    const draft = drafts[accountId] ?? null
    const setDraft = React.useCallback(
        (next: PersonalGrantDraft | null) => {
            setDrafts((current) => ({
                ...current,
                [accountId]: next ?? undefined,
            }))
        },
        [accountId],
    )
    React.useEffect(() => {
        if (!draft) return
        const confirmLeave = () => {
            if (!window.confirm("人员数据范围尚未保存，确定放弃选择并离开？"))
                return false
            setDraft(null)
            return true
        }
        const guard = (event: BeforeUnloadEvent) => {
            event.preventDefault()
            event.returnValue = ""
        }
        const navigation = (window as Window & { navigation?: EventTarget })
            .navigation
        const navigate = (event: Event) => {
            if (event.cancelable && !confirmLeave()) event.preventDefault()
        }
        const linkGuard = (event: MouseEvent) => {
            if (navigation || !(event.target instanceof Element)) return
            if (event.target.closest("a[href]") && !confirmLeave()) {
                event.preventDefault()
                event.stopPropagation()
            }
        }
        window.addEventListener("beforeunload", guard)
        navigation?.addEventListener("navigate", navigate)
        document.addEventListener("click", linkGuard, true)
        return () => {
            window.removeEventListener("beforeunload", guard)
            navigation?.removeEventListener("navigate", navigate)
            document.removeEventListener("click", linkGuard, true)
        }
    }, [draft, setDraft])
    return { draft, setDraft }
}
