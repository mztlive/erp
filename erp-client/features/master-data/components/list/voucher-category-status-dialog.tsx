"use client"

import { useState } from "react"
import { CirclePauseIcon, CirclePlayIcon, LoaderCircleIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useVoucherCategoryStatusMutation } from "@/features/master-data/hooks/queries"
import { defaultImmediateEffectiveFrom } from "@/features/master-data/lib/resource-fields"
import type { MasterDataListItem } from "@/features/master-data/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"

/** 列表与详情使用同一启停权限和状态文案。 */
export function VoucherCategoryStatusButton({
    row,
    surface,
    onClick,
}: {
    row: MasterDataListItem
    surface: "table" | "sheet"
    onClick: () => void
}) {
    const account = useAccountProfileQuery()
    const canUpdate = hasPermission(
        account.data?.permissions,
        "voucher_category_profile:update",
    )
    const disabling = row.lifecycleStatus === "ENABLED"
    const action = disabling ? "停用" : "启用"
    const ActionIcon = disabling ? CirclePauseIcon : CirclePlayIcon
    return (
        <Button
            id={`master-data-voucher-category-${surface}-${toAutomationIdSegment(row.stableId)}-status`}
            type="button"
            variant={surface === "table" ? "ghost" : "outline"}
            size={surface === "table" ? "xs" : "default"}
            disabled={!canUpdate}
            title={!canUpdate ? "当前账号没有卡券类目更新权限" : undefined}
            onClick={(event) => {
                event.stopPropagation()
                onClick()
            }}
        >
            <ActionIcon data-icon="inline-start" aria-hidden />
            {action}
        </Button>
    )
}

/** 启停确认仅追加新版本，不改变既有单据和历史描述。 */
export function VoucherCategoryStatusDialog({
    target,
    onClose,
}: {
    target: MasterDataListItem | null
    onClose: () => void
}) {
    const mutation = useVoucherCategoryStatusMutation()
    const account = useAccountProfileQuery()
    const [error, setError] = useState<string | null>(null)
    const [conflict, setConflict] = useState(false)
    const disabling = target?.lifecycleStatus === "ENABLED"
    const action = disabling ? "停用" : "启用"
    const ActionIcon = disabling ? CirclePauseIcon : CirclePlayIcon
    const prefix = "master-data-voucher-category-status"
    const canUpdate = hasPermission(
        account.data?.permissions,
        "voucher_category_profile:update",
    )

    const submit = async () => {
        if (!target || !canUpdate || mutation.isPending || conflict) return
        setError(null)
        try {
            const result = await mutation.mutateAsync({
                status: disabling ? "disabled" : "active",
                input: {
                    resource: "voucher-categories",
                    stableId: target.stableId,
                    baseRevisionId: target.currentRevisionId,
                    expectedLockVersion: target.lockVersion,
                    name: target.name,
                    effectiveFrom: defaultImmediateEffectiveFrom(),
                    changeReason: action,
                    fields: {
                        voucherNo: target.stableNo,
                        description:
                            target.keyFacts.find(
                                (fact) => fact.label === "说明",
                            )?.value ?? target.name,
                    },
                    idempotencyKey: `${prefix}-${target.currentRevisionId}-${action}`,
                },
            })
            if (result.outcome !== "succeeded") {
                setConflict(result.outcome === "conflict")
                setError(
                    result.outcome === "conflict"
                        ? "资料已更新，请关闭弹窗后重新操作。"
                        : result.message,
                )
                return
            }
            toast.add({
                title: `已${action}卡券类目`,
                description: `${target.stableNo} · ${target.name}`,
                type: "success",
            })
            onClose()
        } catch (cause) {
            setError(getErrorMessage(cause, `${action}失败，请重试。`))
        }
    }

    return (
        <Dialog
            open={target != null}
            onOpenChange={(open) => {
                if (!open && !mutation.isPending) {
                    setError(null)
                    setConflict(false)
                    onClose()
                }
            }}
        >
            <DialogContent
                closeButtonId={`${prefix}-close`}
                className="sm:max-w-md"
            >
                <DialogHeader>
                    <DialogTitle>{action}卡券类目</DialogTitle>
                    <DialogDescription>
                        {disabling
                            ? "停用后，该类目及关联商品、SKU 将停用，SKU 同时下架。历史版本和既有单据保留。"
                            : "启用后，该类目及关联商品、SKU 恢复启用。SKU 保持下架，需要销售时请在商品资料中重新上架。"}
                    </DialogDescription>
                </DialogHeader>
                <p className="break-words text-sm">
                    <span className="num">{target?.stableNo}</span> ·{" "}
                    {target?.name}
                </p>
                {error ? (
                    <p role="alert" className="text-sm text-destructive">
                        {error}
                    </p>
                ) : null}
                <DialogFooter>
                    <Button
                        id={`${prefix}-cancel`}
                        variant="outline"
                        disabled={mutation.isPending}
                        onClick={() => {
                            setError(null)
                            setConflict(false)
                            onClose()
                        }}
                    >
                        取消
                    </Button>
                    <Button
                        id={`${prefix}-confirm`}
                        variant={disabling ? "destructive" : "default"}
                        disabled={!canUpdate || mutation.isPending || conflict}
                        onClick={() => void submit()}
                    >
                        {mutation.isPending ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                className="animate-spin"
                                aria-hidden
                            />
                        ) : (
                            <ActionIcon data-icon="inline-start" aria-hidden />
                        )}
                        {mutation.isPending ? "提交中…" : `确认${action}`}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
