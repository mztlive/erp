"use client"
import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import { Button } from "@/components/ui/button"
import { portalAvailability } from "../api"
import { usePortalCommand } from "../hooks/queries"
import { optionalQuantity } from "../lib/forms"
import { commandKey } from "../lib/presentation"
import type { PortalOffering } from "../types"
import { usePortalProfile } from "./portal-session"
import { PortalError } from "./surface"
const schema = z.object({
    quantity: optionalQuantity,
    status: z.enum(["AVAILABLE", "OUT_OF_STOCK"]),
    reason: z.string().trim().min(1, "请填写报送说明"),
})
export function PortalAvailabilityForm({
    offering,
    onReload,
}: {
    offering: PortalOffering
    onReload: () => Promise<PortalOffering | undefined>
}) {
    const profile = usePortalProfile()
    const mutation = usePortalCommand((body: Record<string, unknown>) =>
        portalAvailability(offering.id, body),
    )
    const [error, setError] = useState<unknown>(null)
    const [saved, setSaved] = useState(false)
    const [expected, setExpected] = useState(offering.availability_version)
    const [latestVersion, setLatestVersion] = useState<number | null>(null)
    const [reloading, setReloading] = useState(false)
    const key = useRef<string | null>(null)
    const frozen = useRef<Record<string, unknown> | null>(null)
    const form = useAppForm({
        defaultValues: {
            quantity: offering.available_quantity ?? "",
            status:
                offering.availability_status === "AVAILABLE"
                    ? "AVAILABLE"
                    : "OUT_OF_STOCK",
            reason: "",
        },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            setError(null)
            setSaved(false)
            key.current ??= commandKey("availability")
            frozen.current ??= {
                expected_version: expected,
                available_quantity: value.quantity.trim() || null,
                availability_status: value.status,
                reason: value.reason.trim(),
                idempotency_key: key.current,
            }
            try {
                await mutation.mutateAsync(frozen.current)
                key.current = null
                frozen.current = null
                setSaved(true)
            } catch (cause) {
                if (commandFailureDisposition(cause) === "rejected") {
                    key.current = null
                    frozen.current = null
                }
                setError(cause)
            }
        },
    })
    const disabled =
        profile?.role !== "maintainer" ||
        offering.source_type === "API" ||
        offering.writable === false ||
        mutation.isPending ||
        saved
    return (
        <form
            className="space-y-4 rounded-xl border p-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <h2 className="font-semibold">更新可供情况</h2>
            <p className="text-sm text-muted-foreground">
                立即报送可供事实；恢复有货后，采购暂停、停止关系和其他供货限制继续生效。
            </p>
            <PortalError error={error} />
            {saved && (
                <p role="status" className="text-sm text-success">
                    可供情况已保存。重新核对最新资料后可继续报送。
                </p>
            )}
            <div className="grid gap-4 md:grid-cols-2">
                <form.AppField name="status">
                    {(field) => (
                        <field.SelectField
                            id="supplier-portal-availability-status"
                            label="可供状态"
                            options={[
                                { value: "AVAILABLE", label: "有货" },
                                { value: "OUT_OF_STOCK", label: "临时缺货" },
                            ]}
                            allowClear={false}
                            disabled={disabled || !!frozen.current}
                        />
                    )}
                </form.AppField>
                <form.AppField name="quantity">
                    {(field) => (
                        <field.TextField
                            id="supplier-portal-availability-quantity"
                            label="可供数量"
                            description="空白表示未提供；0表示明确无货"
                            disabled={disabled || !!frozen.current}
                        />
                    )}
                </form.AppField>
            </div>
            <form.AppField name="reason">
                {(field) => (
                    <field.TextField
                        id="supplier-portal-availability-reason"
                        label="报送说明"
                        required
                        disabled={disabled || !!frozen.current}
                    />
                )}
            </form.AppField>
            <div className="flex flex-wrap gap-2">
                <form.AppForm>
                    <form.SubmitButton
                        id="supplier-portal-availability-save"
                        label={frozen.current ? "重试原报送" : "保存可供情况"}
                        disabled={disabled}
                    />
                </form.AppForm>
                <Button
                    id="supplier-portal-availability-reload"
                    type="button"
                    variant="outline"
                    disabled={mutation.isPending || reloading}
                    onClick={() => {
                        setReloading(true)
                        void onReload()
                            .then((fresh) => {
                                if (!fresh) return
                                setLatestVersion(fresh.availability_version)
                                if (saved) {
                                    setExpected(fresh.availability_version)
                                    setSaved(false)
                                }
                            })
                            .finally(() => setReloading(false))
                    }}
                >
                    重新核对最新资料
                </Button>
                {commandFailureDisposition(error) === "conflict" && (
                    <Button
                        id="supplier-portal-availability-apply-version"
                        type="button"
                        variant="outline"
                        disabled={mutation.isPending || latestVersion == null}
                        onClick={() => {
                            setExpected(latestVersion!)
                            setLatestVersion(null)
                            frozen.current = null
                            key.current = null
                            setError(null)
                        }}
                    >
                        已核对，保留输入重新报送
                    </Button>
                )}
            </div>
        </form>
    )
}
