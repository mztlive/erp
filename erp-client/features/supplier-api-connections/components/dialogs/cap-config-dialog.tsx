"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import type {
    CapabilityCode,
    ConnectionCenterView,
} from "@/features/supplier-api-connections/types"

export function CapConfigDialog({
    open,
    onOpenChange,
    conn,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (o: boolean) => void
    conn: ConnectionCenterView
    pending: boolean
    onSubmit: (
        changes: Array<{ code: CapabilityCode; enabled: boolean }>,
    ) => Promise<void>
}) {
    const [draft, setDraft] = React.useState<Record<string, boolean>>({})

    React.useEffect(() => {
        if (open) {
            const next: Record<string, boolean> = {}
            for (const c of conn.capabilities) {
                next[c.capabilityCode] = c.status === "ENABLED"
            }
            setDraft(next)
        }
    }, [open, conn.capabilities])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>配置连接能力</DialogTitle>
                    <DialogDescription>
                        由系统管理员统一配置，配置后能力需重新验证；不复用采购确认写入口。
                    </DialogDescription>
                </DialogHeader>
                <div className="max-h-72 space-y-2 overflow-y-auto">
                    {conn.capabilities.map((c) => (
                        <label
                            key={c.capabilityCode}
                            className="flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm"
                        >
                            <span>{c.capabilityLabel}</span>
                            <input
                                type="checkbox"
                                checked={draft[c.capabilityCode] ?? false}
                                onChange={(e) =>
                                    setDraft((d) => ({
                                        ...d,
                                        [c.capabilityCode]: e.target.checked,
                                    }))
                                }
                                aria-label={`${
                                    (draft[c.capabilityCode] ?? false)
                                        ? "停用"
                                        : "启用"
                                } ${c.capabilityLabel}`}
                            />
                        </label>
                    ))}
                </div>
                <DialogFooter>
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={pending}
                        onClick={() => {
                            const changes = conn.capabilities
                                .filter(
                                    (c) =>
                                        (draft[c.capabilityCode] ?? false) !==
                                        (c.status === "ENABLED"),
                                )
                                .map((c) => ({
                                    code: c.capabilityCode,
                                    enabled: draft[c.capabilityCode] ?? false,
                                }))
                            if (changes.length === 0) {
                                onOpenChange(false)
                                return
                            }
                            void onSubmit(changes)
                        }}
                    >
                        提交能力配置
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
