"use client"

import { DiscardConfirmDialog } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
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
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { CircleAlertIcon } from "lucide-react"
import {
    uploadErrorMessage,
    useContractUploadForm,
} from "@/features/contracts/hooks/use-contract-upload-form"
import type { UploadContractPdfResult } from "@/features/contracts/types"
import {
    CustomerSearchCombobox,
    SettlementPartySearchCombobox,
} from "@/features/entity-selectors"

export type ContractUploadDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    /** 预选客户（客户中心 / 建单页带入） */
    initialCustomerId?: string
    /** 归档成功后回调；父层负责刷新列表、选中合同或展示结果 */
    onSuccess?: (result: UploadContractPdfResult) => void
}

export function ContractUploadDialog({
    open,
    onOpenChange,
    initialCustomerId = "",
    onSuccess,
}: ContractUploadDialogProps) {
    const {
        form,
        dirty,
        uploadMutation,
        canReadAllCustomers,
        customerPartyId,
        discardOpen,
        setDiscardOpen,
    } = useContractUploadForm({
        open,
        onOpenChange,
        initialCustomerId,
        onSuccess,
    })

    return (
        <>
            <Dialog
                open={open}
                onOpenChange={(next) => {
                    if (!next && dirty) {
                        setDiscardOpen(true)
                        return
                    }
                    if (!next) {
                        form.reset()
                        uploadMutation.reset()
                    }
                    onOpenChange(next)
                }}
            >
                <DialogContent className="flex max-h-[calc(100dvh-2rem)] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
                    <DialogHeader className="px-6 pt-6">
                        <DialogTitle>上传合同 PDF</DialogTitle>
                        <DialogDescription>
                            系统不新建或编辑合同正文；上传已签署电子档并补充检索信息后，形成可引用的合同版本。
                        </DialogDescription>
                    </DialogHeader>
                    <form
                        className="flex min-h-0 flex-1 flex-col"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
                            {uploadMutation.isError ? (
                                <Alert variant="destructive">
                                    <CircleAlertIcon aria-hidden="true" />
                                    <AlertTitle>合同 PDF 未归档</AlertTitle>
                                    <AlertDescription>
                                        {uploadErrorMessage(
                                            uploadMutation.error,
                                        )}
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            <div className="grid gap-4 md:grid-cols-2">
                                <div className="min-w-0">
                                    <form.AppField
                                        name="pdfFile"
                                        children={(field) => (
                                            <field.PdfUploadField
                                                id="card-contracts-upload-pdf"
                                                label="合同电子档"
                                            />
                                        )}
                                    />
                                </div>

                                <div className="grid min-w-0 content-start gap-3">
                                    <form.AppField
                                        name="contractNo"
                                        children={(field) => (
                                            <field.TextField
                                                id="card-contracts-upload-contract-no"
                                                label="合同编号"
                                            />
                                        )}
                                    />
                                    <form.AppField
                                        name="customerId"
                                        children={(field) => {
                                            const isInvalid =
                                                field.state.meta.isTouched &&
                                                !field.state.meta.isValid
                                            const errors = toFieldErrors(
                                                field.state.meta.errors,
                                            )
                                            return (
                                                <Field
                                                    data-invalid={
                                                        isInvalid || undefined
                                                    }
                                                >
                                                    <FieldLabel htmlFor="card-contracts-upload-customer">
                                                        客户
                                                        <span className="text-destructive">
                                                            *
                                                        </span>
                                                    </FieldLabel>
                                                    <CustomerSearchCombobox
                                                        id="card-contracts-upload-customer"
                                                        value={
                                                            field.state.value ||
                                                            undefined
                                                        }
                                                        onValueChange={(id) => {
                                                            field.handleChange(
                                                                id ?? "",
                                                            )
                                                        }}
                                                        onItemChange={(
                                                            customer,
                                                        ) => {
                                                            form.setFieldValue(
                                                                "customerName",
                                                                customer?.legalName ??
                                                                    "",
                                                            )
                                                        }}
                                                        scope={
                                                            canReadAllCustomers
                                                                ? "all_authorized"
                                                                : "assigned"
                                                        }
                                                        placeholder="搜索客户编号或名称"
                                                    />
                                                    {isInvalid ? (
                                                        <FieldError
                                                            errors={errors}
                                                        />
                                                    ) : null}
                                                </Field>
                                            )
                                        }}
                                    />
                                    <form.AppField
                                        name="settlementPartyId"
                                        children={(field) => {
                                            const isInvalid =
                                                field.state.meta.isTouched &&
                                                !field.state.meta.isValid
                                            const errors = toFieldErrors(
                                                field.state.meta.errors,
                                            )
                                            return (
                                                <Field
                                                    data-invalid={
                                                        isInvalid || undefined
                                                    }
                                                >
                                                    <FieldLabel htmlFor="card-contracts-upload-settlement-party">
                                                        结算主体
                                                        <span className="text-destructive">
                                                            *
                                                        </span>
                                                    </FieldLabel>
                                                    <SettlementPartySearchCombobox
                                                        id="card-contracts-upload-settlement-party"
                                                        value={
                                                            field.state.value ||
                                                            undefined
                                                        }
                                                        restrictToPartyId={
                                                            customerPartyId ||
                                                            undefined
                                                        }
                                                        onValueChange={(id) => {
                                                            field.handleChange(
                                                                id ?? "",
                                                            )
                                                        }}
                                                        onItemChange={(
                                                            party,
                                                        ) => {
                                                            form.setFieldValue(
                                                                "settlementPartyName",
                                                                party?.displayName ??
                                                                    "",
                                                            )
                                                        }}
                                                        placeholder="搜索结算主体"
                                                    />
                                                    {isInvalid ? (
                                                        <FieldError
                                                            errors={errors}
                                                        />
                                                    ) : null}
                                                </Field>
                                            )
                                        }}
                                    />
                                    <form.AppField
                                        name="paymentTerms"
                                        children={(field) => (
                                            <field.SelectField
                                                id="card-contracts-upload-payment-terms"
                                                label="付款条件"
                                                options={PAYMENT_TERM_OPTIONS}
                                                description="用于销售单快速带出；完整条款以 PDF 为准。"
                                                required
                                            />
                                        )}
                                    />
                                </div>
                            </div>

                            <div className="grid gap-3 sm:grid-cols-3">
                                <form.AppField
                                    name="signedAt"
                                    children={(field) => (
                                        <field.DateField
                                            id="card-contracts-upload-signed-at"
                                            label="签订日期"
                                        />
                                    )}
                                />
                                <form.AppField
                                    name="validFrom"
                                    children={(field) => (
                                        <field.DateField
                                            id="card-contracts-upload-valid-from"
                                            label="有效期起"
                                        />
                                    )}
                                />
                                <form.AppField
                                    name="validTo"
                                    children={(field) => (
                                        <field.DateField
                                            id="card-contracts-upload-valid-to"
                                            label="有效期止"
                                        />
                                    )}
                                />
                            </div>
                        </div>
                        <DialogFooter className="shrink-0 border-t px-6 py-4">
                            <DialogClose
                                render={
                                    <Button
                                        id="card-contracts-upload-cancel"
                                        type="button"
                                        variant="outline"
                                    />
                                }
                            >
                                取消
                            </DialogClose>
                            <form.AppForm>
                                <form.SubmitButton
                                    id="card-contracts-upload-submit"
                                    label={
                                        uploadMutation.isPending
                                            ? "上传中…"
                                            : "上传并归档"
                                    }
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>

            <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                onConfirm={() => {
                    setDiscardOpen(false)
                    form.reset()
                    uploadMutation.reset()
                    onOpenChange(false)
                }}
            />
        </>
    )
}
