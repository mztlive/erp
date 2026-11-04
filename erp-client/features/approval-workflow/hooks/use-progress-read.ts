"use client"

import { useEffect, useRef } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { queryKeyRoots } from "@/lib/query-key-roots"
import { markApprovalProgressRead } from "../api"

/** 进展已在前台页面展示后确认对应版本；预取和后台标签页不消除提醒。 */
export function useApprovalProgressRead(
    instanceId: string,
    observedVersion: string | undefined,
) {
    const queryClient = useQueryClient()
    const attempted = useRef<string | undefined>(undefined)
    const { mutate } = useMutation({
        mutationFn: markApprovalProgressRead,
        retry: 2,
        onSuccess: () =>
            queryClient.invalidateQueries({
                queryKey: [
                    ...queryKeyRoots.workspaceHome,
                    "navigation-count",
                    "started-unread",
                ],
            }),
    })

    useEffect(() => {
        if (!instanceId || !observedVersion) return
        const token = `${instanceId}:${observedVersion}`
        const confirm = () => {
            if (
                document.visibilityState !== "visible" ||
                attempted.current === token
            )
                return
            attempted.current = token
            mutate({ instanceId, observedVersion })
        }
        const frame = requestAnimationFrame(confirm)
        document.addEventListener("visibilitychange", confirm)
        return () => {
            cancelAnimationFrame(frame)
            document.removeEventListener("visibilitychange", confirm)
        }
    }, [instanceId, observedVersion, mutate])
}
