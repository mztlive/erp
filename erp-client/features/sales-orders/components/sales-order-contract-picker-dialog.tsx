"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import { useSelector } from "@tanstack/react-form"
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
import { Field, FieldLabel } from "@/components/ui/field"
import { Spinner } from "@/components/ui/spinner"
import { ContractSearchCombobox } from "@/features/entity-selectors"
import { useContractCenterQuery } from "@/features/contracts/queries"

/** 候选合同只在确认后写入销售单，取消选择不改变当前开单信息。 */
export function SalesOrderContractPickerDialog({
    customerId,
    initialContractId,
    onClose,
    onConfirm,
}: {
    customerId?: string
    initialContractId: string
    onClose: () => void
    onConfirm: (contractId: string) => void
}) {
    const form = useAppForm({
        defaultValues: { contractId: initialContractId },
    })
    const contractId = useSelector(
        form.store,
        (state) => state.values.contractId,
    )
    const query = useContractCenterQuery(contractId)
    const contract =
        query.data?.contractId === contractId ? query.data : undefined
    const canConfirm = Boolean(
        contract?.selectableForNewSalesOrder &&
        (!customerId || contract.customer.id === customerId) &&
        !query.isFetching &&
        !query.isError,
    )

    return (
        <Dialog open onOpenChange={(open) => !open && onClose()}>
            <DialogContent
                className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-xl"
                closeButtonId="sales-contract-picker-close"
            >
                <DialogHeader>
                    <DialogTitle>选择已有合同</DialogTitle>
                    <DialogDescription>
                        选择已归档的有效合同，确认后自动带入客户、结算主体与合同条款。
                    </DialogDescription>
                </DialogHeader>
                <form.AppField name="contractId">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor="sales-contract-picker-contract">
                                有效合同
                                <RequiredMark />
                            </FieldLabel>
                            <ContractSearchCombobox
                                id="sales-contract-picker-contract"
                                value={field.state.value || undefined}
                                onValueChange={(id) =>
                                    field.handleChange(id ?? "")
                                }
                                customerId={customerId}
                                selectableOnly
                                placeholder="搜索合同编号或客户"
                                emptyLabel="暂无可用合同，请返回上传并导入合同"
                            />
                        </Field>
                    )}
                </form.AppField>
                <div aria-live="polite">
                    {contractId && query.isFetching ? (
                        <p className="flex items-center gap-2 text-sm text-muted-foreground">
                            <Spinner />
                            正在读取合同信息…
                        </p>
                    ) : contractId && (query.isError || !contract) ? (
                        <div className="space-y-3">
                            <p
                                role="alert"
                                className="text-sm text-destructive"
                            >
                                合同信息加载失败，请重试或选择其他合同。
                            </p>
                            <Button
                                id="sales-contract-picker-retry"
                                type="button"
                                variant="outline"
                                onClick={() => void query.refetch()}
                            >
                                重新加载
                            </Button>
                        </div>
                    ) : contract ? (
                        <div className="space-y-3">
                            <dl className="grid grid-cols-1 gap-4 rounded-lg bg-muted/40 p-4 text-sm sm:grid-cols-2">
                                {[
                                    ["客户", contract.customer.displayName],
                                    [
                                        "合同编号",
                                        `${contract.contractNo} · v${contract.currentRevision.revisionNo}`,
                                    ],
                                    [
                                        "默认结算主体",
                                        contract.currentRevision
                                            .signingCompanyName ||
                                            contract.currentRevision
                                                .settlementParty.displayName,
                                    ],
                                    [
                                        "付款条件",
                                        contract.currentRevision
                                            .paymentTermSnapshot.label,
                                    ],
                                    [
                                        "开票要求",
                                        contract.currentRevision
                                            .invoiceRequirementSnapshot
                                            .titleType,
                                    ],
                                    [
                                        "税率",
                                        contract.currentRevision.taxPoint
                                            ? `${contract.currentRevision.taxPoint.replace(/[%％]$/, "")}%`
                                            : undefined,
                                    ],
                                ].map(([label, value]) => (
                                    <div key={label} className="min-w-0">
                                        <dt className="text-xs text-muted-foreground">
                                            {label}
                                        </dt>
                                        <dd className="mt-1 break-words">
                                            {value || "未提供"}
                                        </dd>
                                    </div>
                                ))}
                            </dl>
                            {!contract.selectableForNewSalesOrder ? (
                                <p
                                    role="alert"
                                    className="text-sm text-destructive"
                                >
                                    {contract.selectableBlocker ||
                                        "该合同暂不可用于新建销售单，请选择其他有效合同。"}
                                </p>
                            ) : null}
                        </div>
                    ) : null}
                </div>
                <DialogFooter>
                    <Button
                        id="sales-contract-picker-cancel"
                        type="button"
                        variant="outline"
                        onClick={onClose}
                    >
                        取消
                    </Button>
                    <Button
                        id="sales-contract-picker-confirm"
                        type="button"
                        disabled={!canConfirm}
                        onClick={() => canConfirm && onConfirm(contractId)}
                    >
                        确认并带入
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
