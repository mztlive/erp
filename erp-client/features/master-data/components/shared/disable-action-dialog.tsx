"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"

import { DiscardConfirmDialog, FormalActionResult } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    DateField,
    DialogScrollBody,
    disableSchema,
    newIdempotencyKey,
    notifySuccess,
} from "@/features/master-data/components/shared/action-dialog-shared"
import {
    masterDataKeys,
    useDisableMasterDataMutation,
} from "@/features/master-data/hooks/queries"
import { defaultImmediateEffectiveFrom } from "@/features/master-data/lib/resource-fields"
import {
    revisionTargetIds,
    type RevisionTarget,
} from "@/features/master-data/lib/revision-target"
import type {
    DisableMasterDataInput,
    MasterDataMutationResult,
    MasterDataResource,
} from "@/features/master-data/types"

export function DisableActionDialog({
    open,
    onOpenChange,
    target,
    submit,
    blockedBanner,
    submitDisabled,
    submitLabel = masterDataCopy.disableSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
    submit: (
        input: Omit<DisableMasterDataInput, "resource">,
    ) => Promise<MasterDataMutationResult>
    blockedBanner?: React.ReactNode
    submitDisabled?: boolean
    submitLabel?: string
}) {
    const queryClient = useQueryClient()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("disable"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)
    const ids = revisionTargetIds(target)

    const form = useAppForm({
        defaultValues: {
            changeReason: "",
            effectiveFrom: defaultImmediateEffectiveFrom(),
        },
        validators: { onChange: disableSchema },
        onSubmit: async ({ value }) => {
            if (!ids.stableId || !ids.baseRevisionId) return
            const response = await submit({
                stableId: ids.stableId,
                baseRevisionId: ids.baseRevisionId,
                expectedLockVersion: ids.lockVersion,
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
    }, [open, ids.stableId])

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
            <DialogContent className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg">
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

                <DialogScrollBody>
                    {blockedBanner}
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
                                    onClick={() => {
                                        void queryClient.invalidateQueries({
                                            queryKey: masterDataKeys.all,
                                        })
                                        setResult(null)
                                        setIdempotencyKey(
                                            newIdempotencyKey("disable"),
                                        )
                                    }}
                                >
                                    {masterDataCopy.reloadAction}
                                </Button>
                            }
                        />
                    ) : null}

                    {result?.outcome !== "succeeded" ? (
                        <form
                            className="grid gap-3"
                            onSubmit={(event) => {
                                event.preventDefault()
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
                                    disabled={
                                        submitDisabled ||
                                        !target
                                    }
                                >
                                    {submitLabel}
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

export function FixedResourceDisableDialog({
    resource,
    open,
    onOpenChange,
    target,
    blockedBanner,
    submitDisabled,
    submitLabel,
}: {
    resource: MasterDataResource
    open: boolean
    onOpenChange: (open: boolean) => void
    target: RevisionTarget | null
    blockedBanner?: React.ReactNode
    submitDisabled?: boolean
    submitLabel?: string
}) {
    const mutation = useDisableMasterDataMutation()
    const submit = React.useCallback(
        (input: Omit<DisableMasterDataInput, "resource">) =>
            mutation.mutateAsync({ resource, ...input }),
        [mutation, resource],
    )
    return (
        <DisableActionDialog
            open={open}
            onOpenChange={onOpenChange}
            target={target}
            submit={submit}
            blockedBanner={blockedBanner}
            submitDisabled={submitDisabled || mutation.isPending}
            submitLabel={submitLabel}
        />
    )
}

export function BrandDisableDialog(
    props: Omit<
        React.ComponentProps<typeof FixedResourceDisableDialog>,
        "resource"
    >,
) {
    return <FixedResourceDisableDialog resource="brands" {...props} />
}

export function UnitOfMeasureDisableDialog(
    props: Omit<
        React.ComponentProps<typeof FixedResourceDisableDialog>,
        "resource"
    >,
) {
    return (
        <FixedResourceDisableDialog resource="unit-of-measures" {...props} />
    )
}

export function CategoryDisableDialog(
    props: Omit<
        React.ComponentProps<typeof FixedResourceDisableDialog>,
        "resource"
    >,
) {
    return <FixedResourceDisableDialog resource="categories" {...props} />
}

export function ProductDisableDialog(
    props: Omit<
        React.ComponentProps<typeof FixedResourceDisableDialog>,
        "resource"
    >,
) {
    return <FixedResourceDisableDialog resource="products" {...props} />
}

export function SupplierDisableDialog(
    props: Omit<
        React.ComponentProps<typeof FixedResourceDisableDialog>,
        "resource"
    >,
) {
    return <FixedResourceDisableDialog resource="suppliers" {...props} />
}
