"use client"

import * as React from "react"
import { z } from "zod"

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
import { toast } from "@/components/ui/toast"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { defaultImmediateEffectiveFrom } from "@/features/master-data/lib/resource-fields"
import {
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
} from "@/features/master-data/hooks/queries"
import type {
    MasterDataListItem,
    MasterDataMutationResult,
    VoucherCategoryFields,
} from "@/features/master-data/types"

/**
 * 卡券类目新建 / 编辑 Dialog。
 * 用户只填编号（新建）/ 名称 / 描述；分类（共用卡券根）、品牌（福尚云）、单位（张）
 * 由后端默认补齐。编辑时编号只读。
 */

function newIdempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

const formSchema = z.object({
    voucherNo: z.string().trim().min(1, "请填写卡券类目编号"),
    name: z.string().trim().min(2, "请填写卡券类目名称"),
    description: z.string().trim().min(1, "请填写卡券类目描述"),
})

type FormValues = {
    voucherNo: string
    name: string
    description: string
}

function defaultFormValues(): FormValues {
    return {
        voucherNo: "",
        name: "",
        description: "",
    }
}

function descriptionFromRow(target: MasterDataListItem): string {
    const fact = target.keyFacts.find(
        (item) =>
            item.label === "说明" || item.label === masterDataCopy.fDescription,
    )
    return fact?.value ?? target.name
}

export function VoucherCategoryFormDialog({
    open,
    onOpenChange,
    target = null,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    /** 非空为编辑；空为新建。 */
    target?: MasterDataListItem | null
}) {
    const isEdit = target != null
    const createMutation = useCreateMasterDataMutation()
    const reviseMutation = useCreateRevisionMutation()
    const mutationPending = createMutation.isPending || reviseMutation.isPending

    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey(
            isEdit ? "revise-voucher-category" : "create-voucher-category",
        ),
    )
    const [result, setResult] = React.useState<MasterDataMutationResult | null>(
        null,
    )
    const [discardOpen, setDiscardOpen] = React.useState(false)

    const form = useAppForm({
        defaultValues: defaultFormValues(),
        validators: { onChange: formSchema },
        onSubmit: async ({ value }) => {
            const fields: VoucherCategoryFields = {
                voucherNo: value.voucherNo.trim(),
                description: value.description.trim(),
            }
            if (isEdit && target) {
                const response = await reviseMutation.mutateAsync({
                    resource: "voucher-categories",
                    stableId: target.stableId,
                    baseRevisionId: target.currentRevisionId,
                    expectedLockVersion: target.lockVersion,
                    name: value.name.trim(),
                    effectiveFrom: defaultImmediateEffectiveFrom(),
                    changeReason: "更新",
                    fields,
                    idempotencyKey,
                })
                if (response.outcome === "succeeded") {
                    toast.add({
                        title: masterDataCopy.reviseSuccessTitle,
                        description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
                        type: "success",
                        timeout: 4000,
                    })
                    reset()
                    onOpenChange(false)
                    return
                }
                setResult(response)
                setIdempotencyKey(newIdempotencyKey("revise-voucher-category"))
                return
            }
            const response = await createMutation.mutateAsync({
                resource: "voucher-categories",
                name: value.name.trim(),
                effectiveFrom: defaultImmediateEffectiveFrom(),
                changeReason: "新建",
                fields,
                idempotencyKey,
            })
            if (response.outcome === "succeeded") {
                toast.add({
                    title: masterDataCopy.createSuccessTitle,
                    description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
                    type: "success",
                    timeout: 4000,
                })
                reset()
                onOpenChange(false)
                return
            }
            setResult(response)
            setIdempotencyKey(newIdempotencyKey("create-voucher-category"))
        },
    })

    const reset = () => {
        setResult(null)
        setIdempotencyKey(
            newIdempotencyKey(
                isEdit ? "revise-voucher-category" : "create-voucher-category",
            ),
        )
        form.reset()
    }

    const requestClose = (next: boolean) => {
        if (next) {
            onOpenChange(true)
            return
        }
        if (result?.outcome === "succeeded") {
            reset()
            onOpenChange(false)
            return
        }
        if (form.state.isDirty || result) {
            setDiscardOpen(true)
            return
        }
        onOpenChange(false)
    }

    React.useEffect(() => {
        if (!open) return
        setResult(null)
        setIdempotencyKey(
            newIdempotencyKey(
                isEdit ? "revise-voucher-category" : "create-voucher-category",
            ),
        )
        if (target) {
            form.setFieldValue("voucherNo", target.stableNo)
            form.setFieldValue("name", target.name)
            form.setFieldValue("description", descriptionFromRow(target))
        } else {
            form.reset()
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps -- only when dialog opens / target changes
    }, [open, target?.stableId, target?.currentRevisionId])

    const title = isEdit
        ? masterDataCopy.reviseTitle
        : masterDataCopy.createTitle("卡券类目")
    const description = isEdit
        ? "修改名称与描述。编号不可改；分类 / 品牌 / 单位沿用创建时默认。"
        : "只需填写编号、名称与描述。分类挂到共用「卡券」根分类，品牌固定「福尚云」，单位固定「张」。"
    const blockedTitle = isEdit
        ? masterDataCopy.reviseBlockedTitle
        : masterDataCopy.createBlockedTitle
    const submitLabel = isEdit
        ? masterDataCopy.reviseSubmit
        : masterDataCopy.createSubmit

    return (
        <Dialog open={open} onOpenChange={requestClose}>
            <DialogContent className="flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>
                        {description}
                        {isEdit && target ? (
                            <>
                                {" "}
                                资料编号{" "}
                                <span className="num">{target.stableNo}</span>
                            </>
                        ) : null}
                    </DialogDescription>
                </DialogHeader>

                <div className="min-h-0 flex-1 overflow-y-auto pr-1">
                    {result?.outcome === "blocked" ? (
                        <FormalActionResult
                            status="blocked"
                            title={blockedTitle}
                            description={result.message}
                            facts={
                                result.detail
                                    ? [{ label: "说明", value: result.detail }]
                                    : undefined
                            }
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
                                name="voucherNo"
                                children={(field) => (
                                    <field.TextField
                                        label="卡券类目编号"
                                        placeholder="全局唯一，同时作为商品与 SKU 编号"
                                        disabled={isEdit}
                                    />
                                )}
                            />
                            <form.AppField
                                name="name"
                                children={(field) => (
                                    <field.TextField label="卡券类目名称" />
                                )}
                            />
                            <form.AppField
                                name="description"
                                children={(field) => (
                                    <field.TextareaField
                                        label={masterDataCopy.fDescription}
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
                                    disabled={mutationPending}
                                >
                                    {submitLabel}
                                </Button>
                            </DialogFooter>
                        </form>
                    ) : null}
                </div>
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
                    reset()
                    onOpenChange(false)
                }}
            />
        </Dialog>
    )
}
