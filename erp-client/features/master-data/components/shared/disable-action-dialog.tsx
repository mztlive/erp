"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

import * as React from "react"
import Link from "next/link"
import { useStore } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"

import { FormalActionResult } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
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
    const account = useAccountProfileQuery()
    const canReadStock =
        account.isSuccess &&
        hasPermission(account.data.permissions, "stock_balance:list")
    const queryClient = useQueryClient()
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey("disable"),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
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

    const isSubmitting = useStore(form.store, (state) => state.isSubmitting)

    React.useEffect(() => {
        if (open) {
            setResult(null)
            setIdempotencyKey(newIdempotencyKey("disable"))
            form.reset()
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, ids.stableId])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && isSubmitting) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="md"
                scroll="body"
                closeButtonId="master-data-shared-disable-close"
                showCloseButton={!isSubmitting}
            >
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

                <form
                    className="flex min-h-0 flex-1 flex-col overflow-hidden"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <DialogBody className="space-y-3">
                        {blockedBanner}
                        {result?.outcome === "blocked" ? (
                            <FormalActionResult
                                status="blocked"
                                title={masterDataCopy.disableBlockedTitle}
                                description={result.message}
                                facts={[
                                    ...(result.detail
                                        ? [
                                              {
                                                  label: "说明",
                                                  value: result.detail,
                                              },
                                          ]
                                        : []),
                                    ...(canReadStock && result.drillHref
                                        ? [
                                              {
                                                  label: "库存台账",
                                                  value: (
                                                      <Link
                                                          id="master-data-disable-open-inventory"
                                                          className="text-primary underline-offset-4 hover:underline"
                                                          href={
                                                              result.drillHref
                                                          }
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
                                        id="master-data-shared-disable-action-dialog-button-1"
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
                            <div className="grid gap-3">
                                <form.AppField
                                    name="effectiveFrom"
                                    children={(field) => (
                                        <DateField
                                            id="master-data-shared-disable-effective-from-picker"
                                            label={
                                                masterDataCopy.fieldDisableAt
                                            }
                                            field={field}
                                            required
                                        />
                                    )}
                                />
                                <form.AppField
                                    name="changeReason"
                                    children={(field) => (
                                        <field.TextareaField
                                            id="master-data-shared-disable-reason"
                                            label={
                                                masterDataCopy.fieldDisableReason
                                            }
                                            required
                                        />
                                    )}
                                />
                            </div>
                        ) : null}
                    </DialogBody>
                    {result?.outcome !== "succeeded" ? (
                        <DialogFooter>
                            <DialogClose
                                render={
                                    <Button
                                        id="master-data-shared-disable-action-dialog-button-2"
                                        type="button"
                                        variant="outline"
                                        disabled={isSubmitting}
                                    />
                                }
                            >
                                关闭
                            </DialogClose>
                            <form.AppForm>
                                <form.SubmitButton
                                    disabled={submitDisabled || !target}
                                    label={submitLabel}
                                    pendingLabel="提交中…"
                                />
                            </form.AppForm>
                        </DialogFooter>
                    ) : null}
                </form>
            </DialogContent>
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
    return <FixedResourceDisableDialog resource="unit-of-measures" {...props} />
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
