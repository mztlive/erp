"use client"

import {
    MoneyValue,
    QuantityValue,
    RateValue,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { CreationBasisSearchCombobox } from "@/features/purchase-orders/components/creation-basis-search-combobox"
import type { PurchaseCreationBasis } from "@/features/purchase-orders/types"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { multiplyFixed } from "@/lib/fixed-decimal"

function CreationBasisSummary({ basis }: { basis: PurchaseCreationBasis }) {
    return (
        <div className={`${surfaceInsetClassName} space-y-4 p-4`}>
            <div className="space-y-2">
                <p className="text-sm font-medium text-foreground">
                    销售与采购上下文
                </p>
                <DescriptionList columns="three" className="gap-y-3">
                    <DescriptionItem>
                        <DescriptionTerm>来源销售单</DescriptionTerm>
                        <DescriptionDetails className="num font-medium">
                            {basis.salesOrderNo}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>客户</DescriptionTerm>
                        <DescriptionDetails>
                            {basis.customerName}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>合同</DescriptionTerm>
                        <DescriptionDetails className="num">
                            {basis.contractNumber ?? "无合同"}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>负责销售</DescriptionTerm>
                        <DescriptionDetails>
                            {basis.salesOwnerName ?? "—"}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>拟采购供应商</DescriptionTerm>
                        <DescriptionDetails>
                            {basis.supplierName}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>采购方式</DescriptionTerm>
                        <DescriptionDetails>
                            {PURCHASE_TYPE_LABEL[basis.purchaseType]} ·{" "}
                            {
                                FULFILLMENT_RESPONSIBILITY_LABEL[
                                    basis.fulfillmentResponsibility
                                ]
                            }
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>付款条件</DescriptionTerm>
                        <DescriptionDetails>
                            {basis.paymentTermLabel}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>预计含税采购额</DescriptionTerm>
                        <DescriptionDetails>
                            <MoneyValue value={basis.estimatedGross} />
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>
            </div>

            <div className="space-y-2">
                <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-medium text-foreground">
                        销售明细与拟采购行
                    </p>
                    <span className="text-xs text-muted-foreground">
                        {basis.lines.length} 行
                    </span>
                </div>
                <div className="overflow-hidden rounded-lg border border-border bg-card">
                    <Table data-density="compact">
                        <TableHeader>
                            <TableRow>
                                <TableHead>销售项目</TableHead>
                                <TableHead data-align="end">采购数量</TableHead>
                                <TableHead data-align="end">含税成本</TableHead>
                                <TableHead data-align="end">进项税率</TableHead>
                                <TableHead data-align="end">预计交期</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {basis.lines.map((line) => (
                                <TableRow
                                    key={line.procurementConfirmationLineId}
                                >
                                    <TableCell className="max-w-[16rem] whitespace-normal">
                                        <div className="font-medium text-foreground">
                                            {line.itemName}
                                        </div>
                                        <div className="mt-0.5 text-xs text-muted-foreground">
                                            {[
                                                line.itemSku,
                                                line.salesAllocationLabel,
                                            ]
                                                .filter(Boolean)
                                                .join(" · ")}
                                        </div>
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={line.unit}
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={line.unitCostGross}
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <RateValue
                                            value={multiplyFixed(
                                                line.inputTaxRate,
                                                "100",
                                                {
                                                    leftMaxScale: 6,
                                                    rightMaxScale: 0,
                                                    outputScale: 2,
                                                },
                                            )}
                                            precision={2}
                                        />
                                    </TableCell>
                                    <TableCell data-align="end" className="num">
                                        {line.expectedDeliveryDate || "—"}
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
                <p className="text-xs text-muted-foreground">
                    创建后将带入供应商、成本、税率与交期；采购提交审批前仍须核对并按权限调整。
                </p>
            </div>
        </div>
    )
}

export type PurchaseOrdersCreateDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    openBases: readonly PurchaseCreationBasis[]
    basesPending: boolean
    basesFailed: boolean
    onRetryBases: () => void
    basisFromUrl: string | null
    salesOrderFromUrl: string | null
    selectedBasisId: string
    onSelectedBasisIdChange: (value: string) => void
    createPending: boolean
    onCreate: () => void
}

export function PurchaseOrdersCreateDialog({
    open,
    onOpenChange,
    openBases,
    basesPending,
    basesFailed,
    onRetryBases,
    basisFromUrl,
    salesOrderFromUrl,
    selectedBasisId,
    onSelectedBasisIdChange,
    createPending,
    onCreate,
}: PurchaseOrdersCreateDialogProps) {
    const selectedBasis = openBases.find((b) => b.basisId === selectedBasisId)
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-4xl">
                <DialogHeader>
                    <DialogTitle>从采购创建依据建单</DialogTitle>
                    <DialogDescription>
                        销售单生效后，系统按销售明细与当前合格供给生成候选依据。
                        采购先核对销售上下文，再选择供应商创建草稿；不能跨销售单或跨供应商混拼。
                    </DialogDescription>
                </DialogHeader>
                <div className="space-y-3">
                    {basesPending ? (
                        <p className="text-sm text-muted-foreground">
                            正在加载创建依据…
                        </p>
                    ) : basesFailed ? (
                        <div className="flex flex-wrap items-center gap-2">
                            <p className="text-sm text-destructive">
                                创建依据加载失败，请重试。
                            </p>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={onRetryBases}
                            >
                                重试
                            </Button>
                        </div>
                    ) : openBases.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            {basisFromUrl || salesOrderFromUrl
                                ? "该销售单当前没有可建采购依据。可能尚未生效、没有合格供给，或已经创建了采购单。"
                                : "当前没有可建采购依据。请从已生效销售单的履约页进入，或检查商品是否存在合格供给。"}
                        </p>
                    ) : (
                        <div className="grid gap-1.5 text-sm">
                            <span>选择创建依据</span>
                            <CreationBasisSearchCombobox
                                className="w-full"
                                items={openBases}
                                value={selectedBasisId}
                                onValueChange={(v) =>
                                    onSelectedBasisIdChange(
                                        v ?? selectedBasisId,
                                    )
                                }
                                allowClear={false}
                                aria-label="选择创建依据"
                                placeholder="选择创建依据"
                            />
                        </div>
                    )}
                    {selectedBasis ? (
                        <CreationBasisSummary basis={selectedBasis} />
                    ) : null}
                </div>
                <DialogFooter>
                    <DialogClose
                        render={<Button type="button" variant="outline" />}
                    >
                        取消
                    </DialogClose>
                    <Button
                        type="button"
                        disabled={!selectedBasis || createPending}
                        onClick={onCreate}
                    >
                        {createPending ? "创建中…" : "创建草稿并打开"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
