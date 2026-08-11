import { PlusIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import type { ReverseRequest } from "@/features/customer-receivables/customer-account-detail-preview"
import { ReceivableCounterpartySearchCombobox } from "@/features/customer-receivables/receivable-counterparty-search-combobox"
import type { AllocationMode } from "@/features/customer-receivables/types"

type ReceivableActionDialogsProps = Readonly<{
    partyPickerOpen: boolean
    partyPickerMode: AllocationMode
    selectedPartyId: string
    createPending: boolean
    onPartyPickerOpenChange: (open: boolean) => void
    onSelectedPartyIdChange: (partyId: string) => void
    onStartSession: (mode: AllocationMode, partyId: string) => void
    reverseRequest: ReverseRequest | null
    reverseReason: string
    reverseAmount: string
    reversePending: boolean
    onReverseOpenChange: (open: boolean) => void
    onReverseReasonChange: (reason: string) => void
    onReverseAmountChange: (amount: string) => void
    onCancelReverse: () => void
    onConfirmReverse: () => void
}>

export function ReceivableActionDialogs({
    partyPickerOpen,
    partyPickerMode,
    selectedPartyId,
    createPending,
    onPartyPickerOpenChange,
    onSelectedPartyIdChange,
    onStartSession,
    reverseRequest,
    reverseReason,
    reverseAmount,
    reversePending,
    onReverseOpenChange,
    onReverseReasonChange,
    onReverseAmountChange,
    onCancelReverse,
    onConfirmReverse,
}: ReceivableActionDialogsProps) {
    const redInvoiceAmountValid =
        reverseRequest?.kind !== "red_invoice" ||
        (Number(reverseAmount) > 0 &&
            Number(reverseAmount) <= Number(reverseRequest.amount ?? 0) + 1e-9)

    return (
        <>
            <Dialog
                open={partyPickerOpen}
                onOpenChange={onPartyPickerOpenChange}
            >
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>
                            {partyPickerMode === "receipt"
                                ? "登记回款 — 选择往来主体"
                                : "登记销项发票 — 选择往来主体"}
                        </DialogTitle>
                        <DialogDescription>
                            本次核销创建后锁定往来主体，中途不可更换。
                            经营客户与结算主体可能不同。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-2">
                        <Label htmlFor="pick-party">往来主体</Label>
                        <ReceivableCounterpartySearchCombobox
                            value={selectedPartyId || undefined}
                            onValueChange={(partyId) =>
                                onSelectedPartyIdChange(partyId ?? "")
                            }
                            purpose="form"
                            className="w-full"
                            aria-label="往来主体"
                            placeholder="请选择往来主体"
                        />
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => onPartyPickerOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={!selectedPartyId || createPending}
                            onClick={() =>
                                onStartSession(partyPickerMode, selectedPartyId)
                            }
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            打开核销工作区
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>

            <Dialog
                open={reverseRequest != null}
                onOpenChange={onReverseOpenChange}
            >
                <DialogContent>
                    <DialogHeader>
                        <DialogTitle>
                            {reverseRequest?.kind === "red_invoice"
                                ? "发起销项红票"
                                : reverseRequest?.kind === "refund"
                                  ? "发起客户退款"
                                  : "发起回款冲正"}
                        </DialogTitle>
                        <DialogDescription>
                            不编辑、不删除已确认记录与分配；仅追加反向记录。原单{" "}
                            {reverseRequest?.label}。
                            {reverseRequest?.kind === "receipt_reverse"
                                ? "冲正表示撤销本次回款记录。"
                                : reverseRequest?.kind === "refund"
                                  ? "退款表示向客户退回资金。"
                                  : "红票表示冲减原票的分配。"}
                        </DialogDescription>
                    </DialogHeader>
                    <div className="space-y-3">
                        {reverseRequest?.kind === "red_invoice" ? (
                            <div className="space-y-1.5">
                                <Label htmlFor="rev-amount">红票金额</Label>
                                <Input
                                    id="rev-amount"
                                    className="num"
                                    inputMode="decimal"
                                    value={reverseAmount}
                                    onChange={(event) =>
                                        onReverseAmountChange(
                                            event.target.value,
                                        )
                                    }
                                    placeholder={`不超过 ${reverseRequest.amount ?? ""}`}
                                />
                                <p className="text-xs text-muted-foreground">
                                    默认按原票有效净已分配全额；可输入部分金额。
                                </p>
                            </div>
                        ) : (
                            <p className="rounded-lg bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
                                将按原单全额追加反向记录
                                {reverseRequest?.amount ? (
                                    <>
                                        （
                                        <MoneyValue
                                            value={reverseRequest.amount}
                                        />
                                        ）
                                    </>
                                ) : (
                                    ""
                                )}
                                ，原记录保留。
                            </p>
                        )}
                        <div className="space-y-1.5">
                            <Label htmlFor="rev-reason">原因说明</Label>
                            <Textarea
                                id="rev-reason"
                                value={reverseReason}
                                onChange={(event) =>
                                    onReverseReasonChange(event.target.value)
                                }
                                placeholder="业务依据与说明"
                            />
                        </div>
                    </div>
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onCancelReverse}
                        >
                            取消
                        </Button>
                        <Button
                            type="button"
                            disabled={
                                reversePending ||
                                !reverseReason.trim() ||
                                !redInvoiceAmountValid
                            }
                            onClick={onConfirmReverse}
                        >
                            确认追加反向记录
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    )
}
