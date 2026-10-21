"use client"

import * as React from "react"
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
import { getErrorMessage } from "@/lib/api/errors"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import { useContractSalesAccessMutation } from "@/features/contracts/hooks/queries"
import type { ContractCenterView } from "@/features/contracts/types"

const schema = z.object({
    users: z.array(z.string()).max(100),
    reason: z.string().trim().min(1, "请填写共享调整原因"),
})

/** 本合同共享单独维护；打开时固定版本，后台刷新不替换未保存名单。 */
export function ContractSalesAccessDialog({
    contract,
    open,
    onOpenChange,
}: {
    contract: ContractCenterView
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useContractSalesAccessMutation()
    const owners = useOwnerOptionsQuery()
    const version = React.useRef(contract.lockVersion)
    const form = useAppForm({
        defaultValues: { users: contract.sharedSalesUserIds, reason: "" },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            await mutation.mutateAsync({
                contractId: contract.contractId,
                version: version.current,
                sharedSalesUserIds: value.users,
                changeReason: value.reason,
            })
            onOpenChange(false)
        },
    })
    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) {
            version.current = contract.lockVersion
            form.reset({ users: contract.sharedSalesUserIds, reason: "" })
            mutation.reset()
        }
        wasOpen.current = open
    }, [
        open,
        contract.lockVersion,
        contract.sharedSalesUserIds,
        form,
        mutation,
    ])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="contracts-sales-access-close">
                <DialogHeader>
                    <DialogTitle>管理合同共享销售</DialogTitle>
                    <DialogDescription>
                        选中的销售在具备相应权限时可查看本合同及附件。选用合同新开单还须具有当前客户资格。共享不会开放其他销售单，也不会改变本合同的负责销售。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="users"
                        children={(field) => (
                            <field.MultiSelectField
                                id="contracts-sales-access-users"
                                label="共享销售"
                                options={(owners.data ?? []).map((owner) => ({
                                    value: owner.userId,
                                    label: owner.displayName,
                                }))}
                            />
                        )}
                    />
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="contracts-sales-access-reason"
                                label="调整原因"
                                required
                            />
                        )}
                    />
                    {mutation.isError ? (
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(
                                mutation.error,
                                "保存共享名单失败。你的输入仍已保留，请核对合同是否有更新。",
                            )}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="contracts-sales-access-cancel"
                            type="button"
                            variant="outline"
                            disabled={mutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="contracts-sales-access-save"
                                label="保存共享名单"
                                disabled={
                                    mutation.isPending ||
                                    owners.isPending ||
                                    owners.isError
                                }
                                loading={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
