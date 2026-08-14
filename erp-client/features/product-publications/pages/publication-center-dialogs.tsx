"use client"

import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { FormalActionConfirmDialog } from "@/components/business"
import type { SessionEdit } from "@/features/product-publications/lib/publish-form"
import { SALE_STATUS_LABEL } from "@/features/product-publications/types"
import type { ProductPublicationView } from "@/features/product-publications/types"
import type { PublicationCenterFormApi } from "./publication-center-session"

export function PublicationCenterDialogs({
    data,
    sessionEdit,
    form,
    confirmOpen,
    onConfirmOpenChange,
    pauseOpen,
    onPauseOpenChange,
    pauseReasonOpen,
    onPauseReasonOpenChange,
    pauseReason,
    onPauseReasonChange,
    publishPending,
    pausePending,
    onConfirmPublish,
    onConfirmPause,
}: {
    data: ProductPublicationView
    sessionEdit: SessionEdit | null
    form: PublicationCenterFormApi
    confirmOpen: boolean
    onConfirmOpenChange: (open: boolean) => void
    pauseOpen: boolean
    onPauseOpenChange: (open: boolean) => void
    pauseReasonOpen: boolean
    onPauseReasonOpenChange: (open: boolean) => void
    pauseReason: string
    onPauseReasonChange: (reason: string) => void
    publishPending: boolean
    pausePending: boolean
    onConfirmPublish: () => void
    onConfirmPause: () => void
}) {
    return (
        <>
            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={onConfirmOpenChange}
                actionLabel="提交发布"
                confirmLabel="确认提交"
                title="确认提交发布"
                description="提交后将形成新发布版本并发送至目标商城，进入「等待商城确认」。"
                fromStatus={{ label: "本次编辑", tone: "warning" }}
                toStatus={{ label: "待商城确认", tone: "info" }}
                lockedFields={
                    sessionEdit
                        ? [
                              `目标商城 ${data.identity.targetMallName}`,
                              `含税销售价 ¥${form.state.values.salesPriceGross}`,
                              `销售状态 ${SALE_STATUS_LABEL[form.state.values.saleStatus]}`,
                              `固定供给 ${data.selectedRevision.fixedOffering.supplierName}`,
                              `最小购买量 ${form.state.values.minimumPurchaseQuantity}`,
                          ]
                        : []
                }
                effects={[
                    "形成新的发布版本并发送",
                    "商城确认前不显示为商城已生效",
                    "不覆盖历史修订",
                ]}
                nextDepartment="商城接收确认"
                irreversibleEffects={["写入新的发布版本号与发送编号"]}
                pending={publishPending}
                onConfirm={onConfirmPublish}
            />

            <FormalActionConfirmDialog
                open={pauseOpen}
                onOpenChange={onPauseOpenChange}
                actionLabel="人工暂停"
                confirmLabel="确认暂停"
                title="确认人工暂停"
                description="将形成新的暂停发布修订并发送至目标商城。"
                fromStatus={{ label: data.statusLabel, tone: data.statusTone }}
                toStatus={{ label: "已暂停", tone: "warning" }}
                lockedFields={[
                    `受影响商城 ${data.identity.targetMallName}`,
                    pauseReason.trim()
                        ? `原因 ${pauseReason.trim()}`
                        : "请填写暂停原因",
                ]}
                effects={["形成暂停修订", "发送至商城", "不覆盖历史版本"]}
                irreversibleEffects={["写入新的暂停修订与发送编号"]}
                pending={pausePending}
                onConfirm={onConfirmPause}
            />

            <AlertDialog
                open={pauseReasonOpen}
                onOpenChange={(open) => {
                    if (!open) onPauseReasonOpenChange(false)
                }}
            >
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>填写暂停原因</AlertDialogTitle>
                        <AlertDialogDescription>
                            原因将随暂停修订一起记录；必填，最多 100 字。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div className="space-y-3">
                        <div className="flex flex-wrap gap-1.5">
                            {[
                                "价格调整",
                                "库存不足",
                                "营销调整",
                                "商品下架",
                            ].map((quick) => (
                                <Button
                                    key={quick}
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    onClick={() => onPauseReasonChange(quick)}
                                >
                                    {quick}
                                </Button>
                            ))}
                        </div>
                        <Textarea
                            value={pauseReason}
                            onChange={(e) =>
                                onPauseReasonChange(
                                    e.target.value.slice(0, 100),
                                )
                            }
                            placeholder="请填写暂停原因"
                            aria-label="暂停原因"
                            rows={3}
                        />
                        {pauseReason.trim() ? (
                            <p className="text-xs text-muted-foreground">
                                {pauseReason.length}/100
                            </p>
                        ) : null}
                    </div>
                    <AlertDialogFooter>
                        <AlertDialogCancel
                            onClick={() => onPauseReasonOpenChange(false)}
                        >
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            disabled={!pauseReason.trim()}
                            onClick={() => {
                                onPauseReasonOpenChange(false)
                                onPauseOpenChange(true)
                            }}
                        >
                            下一步
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
