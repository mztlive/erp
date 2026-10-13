"use client"

import { Trash2Icon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { FormalActionConfirmDialog } from "./workflow"

/**
 * 作废改单草稿的入口与确认层；销售与采购改单共用。
 *
 * 只负责确认与回调，不发请求；审批中的改单须先撤回才会出现本入口。
 */
export function ChangeOrderVoidButton({
    id,
    sourceLabel,
    pending,
    onConfirm,
}: {
    id: string
    /** 原单称谓，例如「销售单」「采购单」。 */
    sourceLabel: string
    pending: boolean
    onConfirm: () => void | Promise<void>
}) {
    return (
        <FormalActionConfirmDialog
            idPrefix={id}
            actionVariant="destructive"
            layout="compact"
            title="作废改单"
            actionLabel="作废改单"
            confirmLabel="确认作废"
            cancelLabel="暂不作废"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "已作废", tone: "destructive" }}
            effects={[
                `${sourceLabel}保持当前生效版本，不受本次改单影响`,
                "作废后可以重新发起改单",
            ]}
            irreversibleEffects={["作废的改单不能恢复或再次提交"]}
            pending={pending}
            onConfirm={onConfirm}
            trigger={
                <Button
                    id={`${id}-trigger`}
                    type="button"
                    variant="outline"
                    disabled={pending}
                >
                    <Trash2Icon data-icon="inline-start" aria-hidden="true" />
                    作废改单
                </Button>
            }
        />
    )
}
