"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { z } from "zod"

import {
    CategoryCombobox,
    DiscardConfirmDialog,
    FormalActionResult,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/toast"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    WAREHOUSE_WRITE_MESSAGE,
    resourceLabel,
} from "@/features/master-data/lib/data"
import {
    RESOURCE_FIELDS,
    buildResourceFields,
    buildResourceSchema,
    currentResourceFieldValues,
    defaultImmediateEffectiveFrom,
    emptyResourceFieldValues,
    usesEffectivePeriod,
    usesWideDialog,
    type ResourceFormValues,
} from "@/features/master-data/lib/resource-fields"
import {
    collectDescendantIds,
    buildCategoryForest,
} from "@/features/master-data/lib/category-tree-model"
import {
    masterDataKeys,
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
    useDisableMasterDataMutation,
    useMasterDataListQuery,
} from "@/features/master-data/hooks/queries"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataMutationResult,
    MasterDataResource,
} from "@/features/master-data/types"
import type { BrandFields } from "@/features/master-data/types"
import { getErrorMessage } from "@/lib/api/errors"
import {
    DateField,
    DialogScrollBody,
    ResourceFieldsSection,
    dialogContentClass,
    disableSchema,
    newIdempotencyKey,
    notifySuccess,
    resolveBrandLogoFields,
} from "@/features/master-data/components/shared/action-dialog-shared"

export function MasterDataDisableDialog({
    open,
    onOpenChange,
    resource,
    target,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    resource: MasterDataResource
    target: MasterDataListItem | MasterDataCenterView | null
}) {
    const mutation = useDisableMasterDataMutation()
    const queryClient = useQueryClient()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("disable"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)

    const isWarehouse = resource === "warehouses"
    const stableId = target?.stableId ?? ""
    const baseRevisionId =
        target && "currentRevisionId" in target
            ? target.currentRevisionId
            : target && "currentRevision" in target
              ? target.currentRevision.revisionId
              : ""
    const lockVersion = target?.lockVersion ?? 0

    const form = useAppForm({
        defaultValues: {
            changeReason: "",
            effectiveFrom: defaultImmediateEffectiveFrom(),
        },
        validators: { onChange: disableSchema },
        onSubmit: async ({ value }) => {
            if (!stableId || !baseRevisionId) return
            const response = await mutation.mutateAsync({
                resource,
                stableId,
                baseRevisionId,
                expectedLockVersion: lockVersion,
                changeReason: value.changeReason.trim(),
                effectiveFrom: value.effectiveFrom,
                idempotencyKey,
            })
            if (response.outcome === "succeeded") {
                notifySuccess(masterDataCopy.disableSuccessTitle, response)
                onOpenChange(false)
                return
            }
            setResult(response)
        },
    })

    React.useEffect(() => {
        if (open) {
            setResult(null)
            setIdempotencyKey(newIdempotencyKey("disable"))
            form.reset()
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, stableId])

    const reloadLatest = React.useCallback(() => {
        void queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
        setResult(null)
        setIdempotencyKey(newIdempotencyKey("disable"))
    }, [queryClient])

    const requestClose = (next: boolean) => {
        if (next) {
            onOpenChange(true)
            return
        }
        if (result?.outcome === "succeeded") {
            onOpenChange(false)
            return
        }
        if (form.state.isDirty || result) {
            setDiscardOpen(true)
            return
        }
        onOpenChange(false)
    }

    return (
        <Dialog open={open} onOpenChange={requestClose}>
            <DialogContent className={dialogContentClass(resource)}>
                <DialogHeader>
                    <DialogTitle>{masterDataCopy.disableTitle}</DialogTitle>
                    <DialogDescription>
                        {masterDataCopy.disableDesc}
                        {target ? (
                            <>
                                {" "}
                                资料编号{" "}
                                <span className="num">{target.stableNo}</span>
                            </>
                        ) : null}
                    </DialogDescription>
                </DialogHeader>

                <DialogScrollBody wide={false}>
                    {isWarehouse ? (
                        <Alert variant="destructive">
                            <AlertTitle>
                                {masterDataCopy.warehouseWriteTitle}
                            </AlertTitle>
                            <AlertDescription>
                                {WAREHOUSE_WRITE_MESSAGE}
                                {target &&
                                "warehouseStockSummary" in target &&
                                target.warehouseStockSummary?.hasBlockingStock
                                    ? ` 另：在库 ${target.warehouseStockSummary.onHandQty} / 预占 ${target.warehouseStockSummary.reservedQty} 时也不可停用。`
                                    : null}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {result?.outcome === "blocked" ? (
                        <FormalActionResult
                            status="blocked"
                            title={masterDataCopy.disableBlockedTitle}
                            description={result.message}
                            facts={[
                                ...(result.detail
                                    ? [{ label: "说明", value: result.detail }]
                                    : []),
                                ...(result.drillHref
                                    ? [
                                          {
                                              label: "库存台账",
                                              value: (
                                                  <Link
                                                      className="text-primary underline-offset-4 hover:underline"
                                                      href={result.drillHref}
                                                  >
                                                      打开库存台账
                                                  </Link>
                                              ),
                                          },
                                      ]
                                    : []),
                            ]}
                        />
                    ) : null}

                    {result?.outcome === "conflict" ? (
                        <FormalActionResult
                            status="blocked"
                            title={masterDataCopy.reviseConflictTitle}
                            description={
                                result.message ||
                                masterDataCopy.reviseConflictHint
                            }
                            facts={[
                                {
                                    label: "当前版本",
                                    value: `v${result.serverRevisionNo}`,
                                },
                            ]}
                            actions={
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={reloadLatest}
                                >
                                    {masterDataCopy.reloadAction}
                                </Button>
                            }
                        />
                    ) : null}

                    {result?.outcome !== "succeeded" ? (
                        <form
                            className="grid gap-3"
                            onSubmit={(e) => {
                                e.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <form.AppField
                                name="effectiveFrom"
                                children={(field) => (
                                    <DateField
                                        id="dis-ef-from"
                                        label={masterDataCopy.fieldDisableAt}
                                        field={field}
                                    />
                                )}
                            />
                            <form.AppField
                                name="changeReason"
                                children={(field) => (
                                    <field.TextareaField
                                        label={
                                            masterDataCopy.fieldDisableReason
                                        }
                                    />
                                )}
                            />
                            <DialogFooter>
                                <DialogClose
                                    render={
                                        <Button
                                            type="button"
                                            variant="outline"
                                        />
                                    }
                                >
                                    关闭
                                </DialogClose>
                                <Button
                                    type="submit"
                                    disabled={mutation.isPending || !target}
                                >
                                    {isWarehouse
                                        ? masterDataCopy.createSubmitRejected
                                        : masterDataCopy.disableSubmit}
                                </Button>
                            </DialogFooter>
                        </form>
                    ) : null}
                </DialogScrollBody>
            </DialogContent>

            <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃本次填写？"
                description="关闭后本次填写的内容将丢失。"
                confirmLabel="放弃填写"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setDiscardOpen(false)
                    onOpenChange(false)
                }}
            />
        </Dialog>
    )
}
