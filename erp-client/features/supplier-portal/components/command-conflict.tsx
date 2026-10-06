"use client"

import { useEffect, useState, type ReactNode } from "react"
import { useMutation } from "@tanstack/react-query"
import { Button } from "@/components/ui/button"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { PortalError } from "./surface"

/** 明确版本冲突须主动核对当前对象后才解除旧操作，输入继续保留。 */
export function PortalCommandConflict({
    error,
    id,
    disabled,
    onRecheck,
    onConfirmed,
    children,
    currentSummary,
}: {
    error: unknown
    id: string
    disabled?: boolean
    onRecheck: () => Promise<void>
    onConfirmed: () => void
    children?: ReactNode
    currentSummary?: string
}) {
    const [ready, setReady] = useState(false)
    useEffect(() => setReady(false), [error])
    const recheck = useMutation({
        mutationFn: onRecheck,
        retry: false,
        onSuccess: () => setReady(true),
    })
    if (commandFailureDisposition(error) !== "conflict") return null
    return (
        <div className="space-y-2 rounded-lg border p-3">
            <p className="text-sm text-muted-foreground">
                资料已变化。输入继续保留；请核对最新状态，再修改或重新保存。
            </p>
            <PortalError error={recheck.error} />
            <Button
                id={`${id}-read`}
                type="button"
                variant="outline"
                disabled={disabled || recheck.isPending}
                onClick={() => recheck.mutate()}
            >
                {recheck.isPending ? "正在读取…" : "读取当前状态"}
            </Button>
            {ready && (
                <>
                    {currentSummary && (
                        <p className="whitespace-pre-line text-sm">
                            {currentSummary}
                        </p>
                    )}
                    {children}
                    <Button
                        id={`${id}-confirm`}
                        type="button"
                        variant="outline"
                        disabled={disabled || recheck.isPending}
                        onClick={() => {
                            onConfirmed()
                            setReady(false)
                        }}
                    >
                        已核对当前状态，保留输入继续
                    </Button>
                </>
            )}
        </div>
    )
}
