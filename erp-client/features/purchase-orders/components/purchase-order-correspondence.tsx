"use client"
import { useRef, useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogFooter,
} from "@/components/ui/dialog"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import {
    usePurchasePaper,
    useSendPurchaseEmail,
    usePurchaseEmails,
} from "../hooks/use-purchase-correspondence"
import type { PurchaseOrderCenterView } from "../types"

export function PurchaseOrderCorrespondence({
    order,
}: {
    order: PurchaseOrderCenterView
}) {
    const [open, setOpen] = useState(false)
    const paper = usePurchasePaper(order.identity.purchaseOrderId)
    const profile = useAccountProfileQuery()
    const canSend = hasPermission(
        profile.data?.permissions,
        "purchase_order:send_email",
    )
    const effective =
        ["EFFECTIVE", "PARTIALLY_EXECUTED", "COMPLETED"].includes(
            order.identity.status,
        ) && order.currentContent.source === "REVISION"
    return (
        <>
            <Button
                id="purchase-order-download-paper"
                size="sm"
                variant="outline"
                disabled={paper.isPending}
                onClick={async () => {
                    try {
                        const file = await paper.mutateAsync()
                        const url = URL.createObjectURL(
                            new Blob([file.html], {
                                type: "text/html;charset=utf-8",
                            }),
                        )
                        const anchor = document.createElement("a")
                        anchor.id = "purchase-order-paper-file"
                        anchor.href = url
                        anchor.download = file.filename
                        anchor.click()
                        URL.revokeObjectURL(url)
                        toast.add({
                            title: "采购单已下载",
                            description:
                                "打开文件即可打印，也可在打印窗口另存为 PDF。",
                            type: "success",
                        })
                    } catch (error) {
                        toast.add({
                            title: "下载失败",
                            description: getErrorMessage(error),
                            type: "error",
                        })
                    }
                }}
            >
                {paper.isPending ? "正在生成…" : "下载纸质采购单"}
            </Button>
            {canSend ? (
                <Button
                    id="purchase-order-email-open"
                    size="sm"
                    variant="outline"
                    disabled={!effective}
                    title={!effective ? "审批生效后可发送给供应商" : undefined}
                    onClick={() => setOpen(true)}
                >
                    发送采购单邮件
                </Button>
            ) : null}
            {open ? (
                <PurchaseEmailDialog
                    order={order}
                    onClose={() => setOpen(false)}
                />
            ) : null}
        </>
    )
}
function PurchaseEmailDialog({
    order,
    onClose,
}: {
    order: PurchaseOrderCenterView
    onClose: () => void
}) {
    const send = useSendPurchaseEmail(order.identity.purchaseOrderId)
    const history = usePurchaseEmails(order.identity.purchaseOrderId)
    const command = useRef<{ recipient: string; key: string } | null>(null)
    const [result, setResult] = useState<string>()
    const [attempted, setAttempted] = useState(false)
    const form = useAppForm({
        defaultValues: { recipient: "" },
        validators: {
            onSubmit: z.object({
                recipient: z.email("请填写有效的供应商邮箱"),
            }),
        },
        onSubmit: async ({ value }) => {
            if (!command.current)
                command.current = {
                    recipient: value.recipient.trim(),
                    key: crypto.randomUUID(),
                }
            setAttempted(true)
            try {
                const response = await send.mutateAsync({
                    expected_version: order.identity.lockVersion,
                    recipient: command.current.recipient,
                    idempotency_key: command.current.key,
                })
                const labels = {
                    ACCEPTED: "发件服务已受理，请以邮件送达记录为准。",
                    FAILED: "发件服务拒绝了本次发送，请联系管理员检查配置。",
                    UNKNOWN:
                        "发送结果暂无法确认，请联系管理员核对发件记录，避免重复发送。",
                    SENDING:
                        "本次邮件已进入发送处理，请联系管理员核对发件记录，避免重复发送。",
                }
                setResult(labels[response.status])
            } catch (error) {
                setResult(
                    getErrorMessage(
                        error,
                        "发送结果暂无法确认，可使用下方按钮查询原请求结果。",
                    ),
                )
            }
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(next) => {
                if (!next && !send.isPending) onClose()
            }}
        >
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>发送采购单邮件</DialogTitle>
                    <DialogDescription>
                        将采购单 {order.identity.purchaseNo}（v
                        {order.identity.revisionNo}）发送给{" "}
                        {order.header.supplierSnapshot}
                        。邮件包含采购内容和可打印附件。
                    </DialogDescription>
                </DialogHeader>
                <form
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                    className="space-y-4"
                >
                    <form.AppField name="recipient">
                        {(field) => (
                            <field.TextField
                                id="purchase-order-email-recipient"
                                label="供应商收件邮箱"
                                type="email"
                                disabled={attempted}
                            />
                        )}
                    </form.AppField>
                    {result ? (
                        <p className="text-sm" role="status">
                            {result}
                        </p>
                    ) : null}
                    <section
                        className="space-y-2 text-xs"
                        aria-label="最近发送记录"
                    >
                        <h3 className="font-medium">
                            最近发送记录（最多 20 条）
                        </h3>
                        {history.isError ? (
                            <p>
                                发送记录读取失败，请刷新后核对，避免重复发送。
                            </p>
                        ) : history.isPending ? (
                            <p>正在读取…</p>
                        ) : !history.data?.length ? (
                            <p className="text-muted-foreground">
                                暂无发送记录
                            </p>
                        ) : (
                            <ul className="max-h-40 space-y-2 overflow-auto">
                                {history.data.map((row) => (
                                    <li key={row.id}>
                                        {new Date(
                                            row.created_at * 1000,
                                        ).toLocaleString("zh-CN")}{" "}
                                        · {row.recipient} ·{" "}
                                        {
                                            {
                                                SENDING: "发送处理中",
                                                ACCEPTED: "服务已受理",
                                                FAILED: "发送失败",
                                                UNKNOWN: "结果待核对",
                                            }[row.status]
                                        }
                                    </li>
                                ))}
                            </ul>
                        )}
                    </section>
                    <DialogFooter>
                        <Button
                            id="purchase-order-email-close"
                            type="button"
                            variant="outline"
                            disabled={send.isPending}
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            id="purchase-order-email-send"
                            type="submit"
                            disabled={
                                send.isPending ||
                                Boolean(send.data) ||
                                history.isPending ||
                                history.isError
                            }
                        >
                            {send.isPending
                                ? "正在发送…"
                                : attempted
                                  ? "查询本次发送结果"
                                  : "确认发送"}
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
