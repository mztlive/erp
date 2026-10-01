"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
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
                        保存后需重新验证所选能力。
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
                                        <Checkbox
                                            id={`supplier-api-connections-cap-config-${toAutomationIdSegment(capability.capabilityCode)}`}
                                            nativeButton
                                            render={
                                                <button
                                                    type="button"
                                                    aria-label={`${
                                                        field.state.value
                                                            ? "停用"
                                                            : "启用"
                                                    } ${capability.capabilityLabel}`}
                                                />
                                            }
                                            checked={field.state.value}
                                            disabled={pending || isSubmitting}
                                            onBlur={field.handleBlur}
                                            onCheckedChange={(checked) =>
                                                field.handleChange(checked)
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
                        <LoadingButton
                            id="supplier-api-connections-cap-config-submit"
                            loading={pending || isSubmitting}
                            type="submit"
                            disabled={
                                pending || isSubmitting || !dirty || !canSubmit
                            }
                        >
                            {pending || isSubmitting ? "提交中…" : "保存配置"}
                        </LoadingButton>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
