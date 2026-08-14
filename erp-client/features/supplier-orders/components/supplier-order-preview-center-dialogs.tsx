"use client"

import { FormalActionConfirmDialog, OptionCombobox } from "@/components/business"
import {
    AlertDialog,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { RELEASE_REASON_OPTIONS } from "@/features/supplier-orders/types"
import type { AfterSalesConfirmRequest } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"
import type { SupplierOrderCenterReleaseForm } from "@/features/supplier-orders/hooks/use-supplier-order-center-forms"

export function SupplierOrderCenterDialogs({
    order,
    taskVersion,
    completionEvidence,
    replayOpen,
    onReplayOpenChange,
    replayPending,
    onReplayConfirm,
    releaseOpen,
    onReleaseOpenChange,
    releaseForm,
    completeOpen,
    onCompleteOpenChange,
    completePending,
    onCompleteConfirm,
    afterSalesRequest,
    onAfterSalesRequestChange,
    afterSalesPending,
    onAfterSalesConfirm,
}: {
    order: SupplierOrderDetailView["order"]
    taskVersion?: string
    completionEvidence?: NonNullable<
        SupplierOrderDetailView["lastInvestigation"]
    >
    replayOpen: boolean
    onReplayOpenChange: (open: boolean) => void
    replayPending: boolean
    onReplayConfirm: () => void | Promise<void>
    releaseOpen: boolean
    onReleaseOpenChange: (open: boolean) => void
    releaseForm: SupplierOrderCenterReleaseForm
    completeOpen: boolean
    onCompleteOpenChange: (open: boolean) => void
    completePending: boolean
    onCompleteConfirm: () => void | Promise<void>
    afterSalesRequest: AfterSalesConfirmRequest | null
    onAfterSalesRequestChange: (request: AfterSalesConfirmRequest | null) => void
    afterSalesPending: boolean
    onAfterSalesConfirm: () => void | Promise<void>
}) {
    return (
        <>
            <FormalActionConfirmDialog
                open={replayOpen}
                onOpenChange={onReplayOpenChange}
                actionLabel="安全重发"
                title="确认沿用原任务号重新提交"
                description="仅在确认无结果且系统判定可安全重试时允许。重发不会新建业务订单。"
                fromStatus={{
                    label: order.fulfillmentLabel,
                    tone: order.fulfillmentTone,
                }}
                toStatus={{ label: "重发后待确认", tone: "info" }}
                effects={[
                    `订单 ${order.orderNo}`,
                    `供应商 ${order.supplierName}`,
                    "沿用原下单任务号",
                    "任务保持待处理，不会自动完成",
                ]}
                irreversibleEffects={["将再次向供应商发起下单"]}
                pending={replayPending}
                onConfirm={onReplayConfirm}
            />

            <AlertDialog open={releaseOpen} onOpenChange={onReleaseOpenChange}>
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>退回团队</AlertDialogTitle>
                        <AlertDialogDescription>
                            清空当前个人责任并保持正式任务开放；不会写入暂停状态，也不会完成任务。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void releaseForm.handleSubmit()
                        }}
                    >
                        <div className="space-y-1">
                            <Label>原因</Label>
                            <releaseForm.AppField
                                name="reasonCode"
                                children={(field) => (
                                    <OptionCombobox
                                        value={field.state.value}
                                        onValueChange={(v) =>
                                            field.handleChange(
                                                v ?? field.state.value,
                                            )
                                        }
                                        options={RELEASE_REASON_OPTIONS.map(
                                            (opt) => ({
                                                value: opt.value,
                                                label: opt.label,
                                            }),
                                        )}
                                        className="w-full"
                                        allowClear={false}
                                        aria-label="原因"
                                        placeholder="选择原因"
                                    />
                                )}
                            />
                        </div>
                        <div className="space-y-1">
                            <Label>说明（可选）</Label>
                            <releaseForm.AppField
                                name="comment"
                                children={(field) => (
                                    <Textarea
                                        value={field.state.value}
                                        onChange={(e) =>
                                            field.handleChange(e.target.value)
                                        }
                                        rows={2}
                                    />
                                )}
                            />
                        </div>
                        <AlertDialogFooter>
                            <AlertDialogCancel
                                type="button"
                                onClick={() => onReleaseOpenChange(false)}
                            >
                                取消
                            </AlertDialogCancel>
                            <releaseForm.AppForm>
                                <releaseForm.SubmitButton label="确认退回团队" />
                            </releaseForm.AppForm>
                        </AlertDialogFooter>
                    </form>
                </AlertDialogContent>
            </AlertDialog>

            <FormalActionConfirmDialog
                open={completeOpen}
                onOpenChange={onCompleteOpenChange}
                actionLabel="完成正式任务"
                title="确认处理结果并完成任务"
                description="提交时将重新核对供应商动作结果、订单数据和当前处理权；任一不一致都保持原任务待处理。"
                fromStatus={{
                    label: order.fulfillmentLabel,
                    tone: order.fulfillmentTone,
                }}
                toStatus={{ label: "任务已完成", tone: "success" }}
                lockedFields={[
                    `订单 ${order.orderNo}`,
                    `任务版本 ${taskVersion ?? "—"}`,
                    `处理凭证 ${completionEvidence?.verifiedSupplierActionResultId ?? "—"}`,
                ]}
                effects={["保存已核实的业务结果", "一并完成当前任务"]}
                pending={completePending}
                onConfirm={onCompleteConfirm}
            />

            <FormalActionConfirmDialog
                open={Boolean(afterSalesRequest)}
                onOpenChange={(open) => {
                    if (!open) onAfterSalesRequestChange(null)
                }}
                actionLabel={
                    afterSalesRequest?.action === "CANCEL"
                        ? "提交取消"
                        : "提交退款"
                }
                title={
                    afterSalesRequest?.action === "CANCEL"
                        ? "确认向供应商提交取消"
                        : "确认向供应商提交退款"
                }
                description={
                    afterSalesRequest
                        ? `将向供应商发起${
                              afterSalesRequest.action === "CANCEL"
                                  ? "取消"
                                  : "退款"
                          }请求，引用售后请求 ${afterSalesRequest.mallRequestRef}；重复提交返回原结果。`
                        : undefined
                }
                fromStatus={{
                    label: "当前状态",
                    tone: "neutral",
                }}
                toStatus={{
                    label:
                        afterSalesRequest?.action === "CANCEL"
                            ? "取消处理中"
                            : "退款处理中",
                    tone: "info",
                }}
                effects={[
                    `引用售后请求 ${afterSalesRequest?.mallRequestRef ?? "—"}`,
                    "重复提交返回原结果，不会重复发起",
                ]}
                irreversibleEffects={[
                    `将向供应商发起${
                        afterSalesRequest?.action === "CANCEL"
                            ? "取消"
                            : "退款"
                    }请求`,
                ]}
                pending={afterSalesPending}
                onConfirm={onAfterSalesConfirm}
            />
        </>
    )
}
