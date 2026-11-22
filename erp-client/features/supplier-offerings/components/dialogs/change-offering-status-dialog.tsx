"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"

import { FormalActionConfirmDialog } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { StatusTone } from "@/components/ui/status-badge"
import { toast } from "@/components/ui/toast"
import { useReviseSupplierOfferingMutation } from "@/features/supplier-offerings/hooks/queries"
import {
    buildStatusRevisionInput,
    errorMessage,
    idempotencyKey,
    statusRevisionSchema,
} from "@/features/supplier-offerings/lib/offering-forms"
import type { OfferingStatusIntent } from "@/features/supplier-offerings/lib/offering-status"
import type {
    OfferingStatus,
    ReviseSupplierOfferingInput,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"
import { OFFERING_STATUS_LABELS } from "@/features/supplier-offerings/types"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"

function offeringStatusTone(status: OfferingStatus): StatusTone {
    if (status === "ACTIVE") return "success"
    if (status === "STOPPED") return "destructive"
    return "neutral"
}

export function ChangeOfferingStatusDialog({
    offering,
    intent,
    onOpenChange,
}: {
    offering: SupplierOfferingView
    intent: OfferingStatusIntent
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useReviseSupplierOfferingMutation()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const ledgerRef = React.useRef<FormalCommandKeyLedger | null>(null)
    if (ledgerRef.current === null) {
        ledgerRef.current = new FormalCommandKeyLedger((prefix) =>
            idempotencyKey(prefix),
        )
    }

    const form = useAppForm({
        defaultValues: {
            changeReason: "",
        },
        validators: { onChange: statusRevisionSchema },
        onSubmit: async () => undefined,
    })
    const changeReason = useStore(
        form.store,
        (state) => state.values.changeReason,
    )

    const isStopping = intent.nextStatus === "STOPPED"
    const isPausing = intent.nextStatus === "PAUSED"

    return (
        <FormalActionConfirmDialog
            idPrefix="supplier-offerings-dialog-status"
            open
            onOpenChange={(next) => {
                if (!next && mutation.isPending) return
                onOpenChange(next)
            }}
            pending={mutation.isPending}
            title={intent.title}
            description={`变更后供货状态将更新为「${OFFERING_STATUS_LABELS[intent.nextStatus]}」，原报价、起订量及有效期保持不变。`}
            actionLabel={intent.label}
            confirmLabel={intent.submitLabel}
            cancelLabel="取消"
            actionVariant={isStopping ? "destructive" : "default"}
            fromStatus={{
                label: OFFERING_STATUS_LABELS[offering.status],
                tone: offeringStatusTone(offering.status),
            }}
            toStatus={{
                label: OFFERING_STATUS_LABELS[intent.nextStatus],
                tone: offeringStatusTone(intent.nextStatus),
            }}
            summary={[
                <span key="sku">
                    商品：
                    {offering.sku_name ?? offering.sku_no ?? offering.sku_id}
                </span>,
                <span key="supplier">
                    供应商：{offering.supplier_name ?? offering.supplier_no}
                </span>,
                <span key="code" className="num">
                    订货编码：{offering.supplier_sku_code}
                </span>,
            ]}
            effects={
                isPausing
                    ? [
                          "暂停期间该供给关系暂不参与采购选源与询价参考，可随时重新启用。",
                      ]
                    : !isStopping
                      ? [
                            "启用后该供给关系恢复可用，按当前有效条款参与采购选源。",
                        ]
                      : []
            }
            irreversibleEffects={
                isStopping
                    ? [
                          "停止供给后该供给关系不再参与采购选源与询价参考；正在执行的采购单仍按原单据推进。",
                      ]
                    : []
            }
            confirmDisabled={!changeReason.trim()}
            onConfirm={async () => {
                setSubmitError(null)
                const trimmedReason = changeReason.trim()
                if (!trimmedReason) {
                    setSubmitError("请填写变更原因")
                    throw new Error("请填写变更原因")
                }
                const ledger = ledgerRef.current!
                const slot = `status:${offering.id}:${intent.nextStatus}`
                const existing =
                    ledger.peek<
                        Omit<ReviseSupplierOfferingInput, "idempotency_key">
                    >(slot)
                const built = existing
                    ? {
                          ...existing.payload,
                          idempotency_key: existing.idempotencyKey,
                      }
                    : (() => {
                          const probe = buildStatusRevisionInput(
                              offering,
                              intent.nextStatus,
                              trimmedReason,
                              "pending",
                          )
                          if (!probe) return null
                          const { idempotency_key: _unused, ...rest } = probe
                          const command = ledger.acquire(
                              slot,
                              "revise-supplier-offering-status",
                              rest,
                          )
                          return {
                              ...command.payload,
                              idempotency_key: command.idempotencyKey,
                          }
                      })()
                if (!built) {
                    const message =
                        "当前条款不完整或看不到价格，无法保存新版本。请改用「修订条款」。"
                    setSubmitError(message)
                    throw new Error(message)
                }
                try {
                    await mutation.mutateAsync(built)
                    ledger.settle(slot, "succeeded")
                    toast.add({
                        title: `已${intent.label}供给关系`,
                        description: `当前状态已更新为「${OFFERING_STATUS_LABELS[intent.nextStatus]}」。`,
                        type: "success",
                    })
                    onOpenChange(false)
                } catch (error) {
                    ledger.settle(slot, classifyFormalCommandError(error))
                    setSubmitError(errorMessage(error, "供给条款保存失败"))
                    throw error
                }
            }}
            onConfirmError={() => undefined}
            formContent={
                <div className="space-y-3">
                    {submitError ? (
                        <Alert variant="destructive">
                            <AlertTitle>保存失败</AlertTitle>
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    ) : null}
                    <form.AppField name="changeReason">
                        {(field) => (
                            <field.TextField
                                id="supplier-offerings-dialog-status-reason"
                                label="变更原因"
                                placeholder={`例如：${intent.defaultReason}`}
                                required
                                disabled={mutation.isPending}
                            />
                        )}
                    </form.AppField>
                </div>
            }
        />
    )
}
