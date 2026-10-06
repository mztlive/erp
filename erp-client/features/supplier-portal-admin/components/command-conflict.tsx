"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { PortalError } from "@/features/supplier-portal/components/surface"

export function PortalCommandConflict({
    idPrefix,
    onReload,
    onConfirmed,
}: {
    idPrefix: string
    onReload: () => Promise<unknown>
    onConfirmed: () => void
}) {
    const [loaded, setLoaded] = useState(false)
    const [confirmed, setConfirmed] = useState(false)
    const [busy, setBusy] = useState(false)
    const [error, setError] = useState<unknown>(null)
    return (
        <div className="space-y-3 rounded-lg border p-4">
            <p className="text-sm">
                资料或处理资格已变化。本次填写保留，请重新读取并核对，再修正内容。
            </p>
            <PortalError error={error} />
            <Button
                id={`${idPrefix}-reload`}
                type="button"
                variant="outline"
                disabled={busy}
                onClick={async () => {
                    setBusy(true)
                    setError(null)
                    setLoaded(false)
                    setConfirmed(false)
                    try {
                        await onReload()
                        setLoaded(true)
                    } catch (cause) {
                        setError(cause)
                    } finally {
                        setBusy(false)
                    }
                }}
            >
                重新读取最新资料
            </Button>
            {loaded && (
                <label
                    htmlFor={`${idPrefix}-confirmed`}
                    className="flex items-center gap-2 text-sm"
                >
                    <NativeCheckbox
                        id={`${idPrefix}-confirmed`}
                        checked={confirmed}
                        onCheckedChange={(value) =>
                            setConfirmed(value === true)
                        }
                    />
                    已核对最新资料，将重新确认受影响的内容
                </label>
            )}
            <Button
                id={`${idPrefix}-resume`}
                type="button"
                disabled={busy || !loaded || !confirmed}
                onClick={onConfirmed}
            >
                保留填写并继续修正
            </Button>
        </div>
    )
}
