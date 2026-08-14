"use client"

import * as React from "react"

import type { AccessView } from "@/features/access-audit/types"

export type ExplainSubject = { type: "ROLE" | "USER"; id: string }

type AccessDetailPanelsInput = {
    view: AccessView
    subjectTypeParam?: string
    subjectIdParam?: string
    eventIdParam?: string
    patchUrl: (
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) => void
}

/**
 * 详情弹层状态：有效权限解释（subject）与审计事件详情（event）。
 * 开关弹层时同步修补 URL，并在关闭动画后恢复行焦点。
 */
function useAccessDetailPanels({
    view,
    subjectTypeParam,
    subjectIdParam,
    eventIdParam,
    patchUrl,
}: AccessDetailPanelsInput) {
    const [explainSubject, setExplainSubject] =
        React.useState<ExplainSubject | null>(
            subjectIdParam &&
                (view === "roles" || view === "users" || view === "scopes")
                ? {
                      type:
                          subjectTypeParam === "USER" || view === "users"
                              ? "USER"
                              : "ROLE",
                      id: subjectIdParam,
                  }
                : null,
        )
    const [eventOpenId, setEventOpenId] = React.useState<string | null>(
        eventIdParam ?? null,
    )
    const rowFocusRef = React.useRef<Map<string, HTMLButtonElement | null>>(
        new Map(),
    )
    const restoreFocusIdRef = React.useRef<string | null>(null)

    const restoreRowFocus = React.useCallback(() => {
        const id = restoreFocusIdRef.current
        if (!id) return
        window.requestAnimationFrame(() => {
            const element = rowFocusRef.current.get(id)
            if (!element) return
            element.focus()
            restoreFocusIdRef.current = null
        })
    }, [])

    React.useEffect(() => {
        setEventOpenId(eventIdParam ?? null)
    }, [eventIdParam])

    React.useEffect(() => {
        if (subjectIdParam) {
            setExplainSubject({
                type:
                    subjectTypeParam === "USER" || view === "users"
                        ? "USER"
                        : "ROLE",
                id: subjectIdParam,
            })
        }
    }, [subjectIdParam, subjectTypeParam, view])

    const openExplain = React.useCallback(
        (type: "ROLE" | "USER", id: string) => {
            restoreFocusIdRef.current = id
            setExplainSubject({ type, id })
            patchUrl(
                {
                    subjectType: type,
                    subjectId: id,
                    eventId: null,
                },
                { replace: true },
            )
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [patchUrl],
    )

    const closeExplain = React.useCallback(() => {
        setExplainSubject(null)
        patchUrl({ subjectId: null, subjectType: null }, { replace: true })
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    const openEvent = React.useCallback(
        (eventId: string) => {
            restoreFocusIdRef.current = eventId
            setEventOpenId(eventId)
            patchUrl({ eventId }, { replace: true })
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [patchUrl],
    )

    const closeEvent = React.useCallback(() => {
        setEventOpenId(null)
        patchUrl({ eventId: null }, { replace: true })
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    return {
        explainSubject,
        setExplainSubject,
        eventOpenId,
        setEventOpenId,
        rowFocusRef,
        restoreRowFocus,
        openExplain,
        closeExplain,
        openEvent,
        closeEvent,
    }
}

export { useAccessDetailPanels }
