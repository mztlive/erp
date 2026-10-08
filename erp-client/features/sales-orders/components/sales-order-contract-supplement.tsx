"use client"

import { useEffect, useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { useQueryClient } from "@tanstack/react-query"
import { z } from "zod"

import { useAppForm, toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { ContractUploadDialog } from "@/features/contracts/contract-upload-dialog"
import { useContractCenterQuery } from "@/features/contracts/queries"
import {
    ContractSearchCombobox,
    entitySelectorKeys,
} from "@/features/entity-selectors"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/contracts"
import { useSupplementSalesOrderContract } from "@/features/sales-orders/hooks/use-sales-order-contract"
import { useSalesOrderContractCheck } from "@/features/sales-orders/hooks/queries"
import { ContractSupplementCheck } from "./contract-supplement-check"
import { hasPermission } from "@/lib/permissions"

/** 概览内补录合同；客户必须一致，结算主体保留销售单选择。 */
export function SalesOrderContractSupplement({
    order,
}: {
    order: SalesOrderDetailView
}) {
    const [open, setOpen] = useState(false)
    const [uploadOpen, setUploadOpen] = useState(false)
    const profile = useAccountProfileQuery()
    const client = useQueryClient()
    const mutation = useSupplementSalesOrderContract(() => setOpen(false))
    const form = useAppForm({
        defaultValues: { contractId: "", requestedContractRevisionId: "" },
        validators: {
            onSubmit: z.object({
                contractId: z.string().trim().min(1, "请选择合同"),
                requestedContractRevisionId: z
                    .string()
                    .trim()
                    .min(1, "请等待合同信息加载完成"),
            }),
        },
        onSubmit: async ({ value }) => {
            if (
                !checkQuery.data?.matches ||
                checkQuery.isFetching ||
                checkQuery.isError ||
                contractQuery.isError
            )
                return
            await mutation.mutateAsync({
                salesOrderId: order.id,
                version: order.version,
                ...value,
            })
        },
    })
    const contractId = useSelector(
        form.store,
        (state) => state.values.contractId,
    )
    const requestedContractRevisionId = useSelector(
        form.store,
        (state) => state.values.requestedContractRevisionId,
    )
    const checkQuery = useSalesOrderContractCheck(
        {
            salesOrderId: order.id,
            version: order.version,
            contractId,
            requestedContractRevisionId,
        },
        open,
    )
    const contractQuery = useContractCenterQuery(contractId)
    useEffect(() => {
        if (!contractQuery.data || contractQuery.data.contractId !== contractId)
            return
        form.setFieldValue(
            "requestedContractRevisionId",
            contractQuery.data.currentRevision.revisionId,
        )
    }, [contractId, contractQuery.data, form])
    if (
        order.contractId ||
        order.originSystem !== "erp" ||
        order.primaryStatus.code === "voided" ||
        !hasPermission(profile.data?.permissions, "sales_order:update")
    )
        return null
    return (
        <>
            <Button
                id="sales-order-overview-supplement-contract"
                type="button"
                variant="outline"
                size="sm"
                onClick={() => {
                    form.reset()
                    mutation.reset()
                    setOpen(true)
                }}
            >
                补录合同
            </Button>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (!mutation.isPending) setOpen(next)
                }}
            >
                <DialogContent
                    closeButtonId="sales-order-contract-supplement-close"
                    className="sm:max-w-xl"
                >
                    <DialogHeader>
                        <DialogTitle>补录销售合同</DialogTitle>
                        <DialogDescription>
                            合同客户及付款、开票、税率必须与原销售单一致。结算主体保留原单选择，补录不会修改原单内容。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                        className="space-y-4"
                    >
                        <form.AppField name="contractId">
                            {(field) => (
                                <Field
                                    data-invalid={
                                        !field.state.meta.isValid || undefined
                                    }
                                >
                                    <FieldLabel htmlFor="sales-order-contract-supplement-select">
                                        销售合同
                                    </FieldLabel>
                                    <ContractSearchCombobox
                                        id="sales-order-contract-supplement-select"
                                        customerId={order.customerId}
                                        selectableOnly
                                        value={field.state.value || undefined}
                                        onValueChange={(id) => {
                                            field.handleChange(id ?? "")
                                            form.setFieldValue(
                                                "requestedContractRevisionId",
                                                "",
                                            )
                                        }}
                                        placeholder="搜索该客户的合同"
                                    />
                                    {!field.state.meta.isValid ? (
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    ) : null}
                                </Field>
                            )}
                        </form.AppField>
                        <form.AppField name="requestedContractRevisionId">
                            {(field) =>
                                !field.state.meta.isValid ? (
                                    <FieldError
                                        errors={toFieldErrors(
                                            field.state.meta.errors,
                                        )}
                                    />
                                ) : null
                            }
                        </form.AppField>
                        <Button
                            id="sales-order-contract-supplement-upload"
                            type="button"
                            variant="outline"
                            disabled={mutation.isPending}
                            onClick={() => setUploadOpen(true)}
                        >
                            上传新合同
                        </Button>
                        {contractId ? (
                            <ContractSupplementCheck
                                data={checkQuery.data}
                                loading={
                                    contractQuery.isFetching ||
                                    checkQuery.isFetching ||
                                    !requestedContractRevisionId
                                }
                                error={contractQuery.error ?? checkQuery.error}
                            />
                        ) : null}
                        <DialogFooter>
                            <Button
                                id="sales-order-contract-supplement-cancel"
                                type="button"
                                variant="outline"
                                disabled={mutation.isPending}
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id="sales-order-contract-supplement-submit"
                                    label="确认补录"
                                    pendingLabel="正在补录…"
                                    loading={mutation.isPending}
                                    disabled={
                                        mutation.isPending ||
                                        contractQuery.isFetching ||
                                        contractQuery.isError ||
                                        checkQuery.isFetching ||
                                        checkQuery.isError ||
                                        !requestedContractRevisionId ||
                                        !checkQuery.data?.matches
                                    }
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <ContractUploadDialog
                open={uploadOpen}
                onOpenChange={setUploadOpen}
                initialCustomerId={order.customerId}
                autoAccept
                onSuccess={(result) => {
                    void client.invalidateQueries({
                        queryKey: entitySelectorKeys.all,
                    })
                    form.setFieldValue("contractId", result.contractId)
                    form.setFieldValue(
                        "requestedContractRevisionId",
                        result.revisionId,
                    )
                }}
            />
        </>
    )
}
