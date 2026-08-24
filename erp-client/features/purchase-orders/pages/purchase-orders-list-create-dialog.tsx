"use client"

import * as React from "react"
import { z } from "zod"

import {
    MoneyValue,
    QuantityValue,
    RateValue,
    surfaceInsetClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
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
import { FieldGroup } from "@/components/ui/field"
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

type PurchaseBasisLineInput = {
    salesOrderLineId: string
    quantity: string
}

function buildLineInputs(
    basis?: PurchaseCreationBasis,
): PurchaseBasisLineInput[] {
    return (
        basis?.lines.map((line) => ({
            salesOrderLineId: line.salesOrderLineId,
            quantity: line.maxCreateQuantity,
        })) ?? []
    )
}

function buildCreationSchema(basis?: PurchaseCreationBasis) {
    return z
        .object({
            lines: z
                .array(
                    z.object({
                        salesOrderLineId: z.string().min(1),
                        quantity: z.string().trim(),
                    }),
                )
                .min(1, "请至少选择一条可采购明细"),
        })
        .superRefine((value, context) => {
            value.lines.forEach((line, index) => {
                const quantity = Number(line.quantity)
                const max = Number(
                    basis?.lines[index]?.maxCreateQuantity ?? "0",
                )
                if (!Number.isFinite(quantity) || quantity <= 0) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "quantity"],
                        message: "本次采购数量必须大于 0",
                    })
                    return
                }
                if (!Number.isFinite(max) || quantity > max) {
                    context.addIssue({
                        code: "custom",
                        path: ["lines", index, "quantity"],
                        message: `本次采购数量不能超过 ${basis?.lines[index]?.maxCreateQuantity ?? "0"}`,
                    })
                }
            })
        })
}

function CreationBasisSummary({
    basis,
    quantityField,
}: {
    basis: PurchaseCreationBasis
    quantityField: (index: number) => React.ReactNode
}) {
    return (
        <div
            className={`${surfaceInsetClassName} flex flex-col gap-4 p-4`}
            data-testid={`purchase-basis-${basis.basisId}`}
        >
            <div className="flex flex-col gap-2">
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

            <div className="flex flex-col gap-2">
                <div className="flex items-center justify-between gap-2">
                    <p className="text-sm font-medium text-foreground">
                        销售明细与本次采购数量
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
                                <TableHead data-align="end">销售数量</TableHead>
                                <TableHead data-align="end">已覆盖</TableHead>
                                <TableHead data-align="end">剩余数量</TableHead>
                                <TableHead data-align="end">
                                    本次采购数量
                                </TableHead>
                                <TableHead data-align="end">含税成本</TableHead>
                                <TableHead data-align="end">进项税率</TableHead>
                                <TableHead data-align="end">预计交期</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {basis.lines.map((line, index) => (
                                <TableRow key={line.salesOrderLineId}>
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
                                            value={line.salesQuantity}
                                            unit={line.unit}
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <QuantityValue
                                            value={line.coveredQuantity}
                                            unit={line.unit}
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <QuantityValue
                                            value={line.remainingQuantity}
                                            unit={line.unit}
                                        />
                                    </TableCell>
                                    <TableCell className="min-w-36">
                                        {quantityField(index)}
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
                    本次采购数量默认取最大可采购量，必须大于
                    0，且不能超过当前剩余数量。
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
    onCreate: (lines: PurchaseBasisLineInput[]) => Promise<void> | void
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
    const lineInputs = React.useMemo(
        () => buildLineInputs(selectedBasis),
        [selectedBasis],
    )
    const schema = React.useMemo(
        () => buildCreationSchema(selectedBasis),
        [selectedBasis],
    )
    const form = useAppForm({
        defaultValues: { lines: [] as PurchaseBasisLineInput[] },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            await onCreate(value.lines)
        },
    })

    React.useEffect(() => {
        form.reset({ lines: lineInputs })
    }, [form, lineInputs])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-6xl">
                <DialogHeader>
                    <DialogTitle>从采购创建依据建单</DialogTitle>
                    <DialogDescription>
                        销售单生效后，系统按销售明细与当前合格供给生成候选依据。
                        采购先核对销售上下文，再选择供应商和本次数量创建草稿；不能跨销售单或跨供应商混拼。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <FieldGroup className="gap-3">
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
                                    ? "该销售单当前没有可建采购依据。可能尚未生效、没有合格供给，或待采购数量已覆盖。"
                                    : "当前没有可建采购依据。请从已生效销售单的履约页进入，或检查商品是否存在合格供给。"}
                            </p>
                        ) : (
                            <div className="grid gap-1.5 text-sm">
                                <span>选择创建依据</span>
                                <CreationBasisSearchCombobox
                                    className="w-full"
                                    items={openBases}
                                    value={selectedBasisId}
                                    onValueChange={(value) =>
                                        onSelectedBasisIdChange(
                                            value ?? selectedBasisId,
                                        )
                                    }
                                    allowClear={false}
                                    aria-label="选择创建依据"
                                    placeholder="选择创建依据"
                                />
                            </div>
                        )}
                        {selectedBasis ? (
                            <CreationBasisSummary
                                basis={selectedBasis}
                                quantityField={(index) => (
                                    <form.AppField
                                        name={`lines[${index}].quantity`}
                                    >
                                        {(field) => (
                                            <field.TextField
                                                label={`本次采购数量，最大 ${selectedBasis.lines[index]?.maxCreateQuantity ?? "0"}`}
                                                hideLabel
                                                type="number"
                                                inputClassName="num text-right"
                                                testId={`purchase-basis-line-quantity-${selectedBasis.lines[index]?.salesOrderLineId ?? index}`}
                                            />
                                        )}
                                    </form.AppField>
                                )}
                            />
                        ) : null}
                    </FieldGroup>
                    <DialogFooter>
                        <DialogClose
                            render={<Button type="button" variant="outline" />}
                        >
                            取消
                        </DialogClose>
                        <form.Subscribe selector={(state) => state.canSubmit}>
                            {(canSubmit) => (
                                <Button
                                    type="submit"
                                    data-testid="purchase-create-from-basis"
                                    disabled={
                                        !selectedBasis ||
                                        !canSubmit ||
                                        createPending
                                    }
                                >
                                    {createPending ? "创建中…" : "创建采购草稿"}
                                </Button>
                            )}
                        </form.Subscribe>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
