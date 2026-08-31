"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    CapabilityCode,
    ConnectionCenterView,
} from "@/features/supplier-api-connections/types"

const capabilityFormSchema = z.object({
    capabilities: z.array(
        z.object({
            code: z.enum([
                "CATALOG",
                "PRICE",
                "STOCK",
                "ORDER",
                "QUERY",
                "CANCEL",
                "REFUND",
                "LOGISTICS",
                "CALLBACK",
                "SETTLEMENT",
            ]),
            enabled: z.boolean(),
        }),
    ),
})

function capabilityDefaults(
    capabilities: ConnectionCenterView["capabilities"],
) {
    return {
        capabilities: capabilities.map((capability) => ({
            code: capability.capabilityCode,
            enabled: capability.status === "ENABLED",
        })),
    }
}

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
    const resetValues = React.useMemo(
        () => capabilityDefaults(conn.capabilities),
        [conn.capabilities],
    )
    const form = useAppForm({
        defaultValues: resetValues,
        validators: { onChange: capabilityFormSchema },
        onSubmit: async ({ value }) => {
            const currentByCode = new Map(
                conn.capabilities.map((capability) => [
                    capability.capabilityCode,
                    capability.status === "ENABLED",
                ]),
            )
            const changes = value.capabilities.flatMap((capability) => {
                const code = capability.code
                return currentByCode.get(code) === capability.enabled
                    ? []
                    : [{ code, enabled: capability.enabled }]
            })
            if (changes.length === 0) {
                onOpenChange(false)
                return
            }
            await onSubmit(changes)
        },
    })
    const dirty = useSelector(form.store, (state) => state.isDirty)
    const canSubmit = useSelector(form.store, (state) => state.canSubmit)
    const isSubmitting = useSelector(form.store, (state) => state.isSubmitting)

    React.useEffect(() => {
        if (!open) return
        form.reset(resetValues)
    }, [form, open, resetValues])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="supplier-api-connections-cap-config-close">
                <DialogHeader>
                    <DialogTitle>配置连接能力</DialogTitle>
                    <DialogDescription>
                        由系统管理员统一配置，配置后能力需重新验证；不复用采购确认写入口。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="contents"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="max-h-72 space-y-2 overflow-y-auto">
                        {conn.capabilities.map((capability, index) => (
                            <form.AppField
                                key={capability.capabilityCode}
                                name={`capabilities[${index}].enabled`}
                                children={(field) => (
                                    <label className="flex items-center justify-between gap-2 rounded-lg border px-3 py-2 text-sm">
                                        <span>
                                            {capability.capabilityLabel}
                                        </span>
                                        <input
                                            id={`supplier-api-connections-cap-config-${toAutomationIdSegment(capability.capabilityCode)}`}
                                            type="checkbox"
                                            checked={field.state.value}
                                            disabled={pending || isSubmitting}
                                            onBlur={field.handleBlur}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.checked,
                                                )
                                            }
                                            aria-label={`${
                                                field.state.value
                                                    ? "停用"
                                                    : "启用"
                                            } ${capability.capabilityLabel}`}
                                        />
                                    </label>
                                )}
                            />
                        ))}
                    </div>
                    <DialogFooter>
                        <Button
                            id="supplier-api-connections-cap-config-cancel"
                            type="button"
                            variant="outline"
                            disabled={pending || isSubmitting}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="supplier-api-connections-cap-config-submit"
                            type="submit"
                            disabled={
                                pending || isSubmitting || !dirty || !canSubmit
                            }
                        >
                            {pending || isSubmitting ? (
                                <Spinner
                                    className="size-4 animate-spin"
                                    aria-hidden="true"
                                />
                            ) : null}
                            {pending || isSubmitting
                                ? "提交中…"
                                : "提交能力配置"}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
