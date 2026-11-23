"use client"

import * as React from "react"
import { ChevronDownIcon, PackageSearchIcon, PlusIcon } from "lucide-react"

import {
    DetailWorkspaceSection,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { toast } from "@/components/ui/toast"
import { SalesOrderCreateDueDateBatchBar } from "@/features/sales-orders/components/sales-order-create-due-date-batch-bar"
import { SalesOrderCreateLineItemTable } from "@/features/sales-orders/components/sales-order-create-line-item-table"
import { SellableSkuSelectDialog } from "@/features/sales-orders/components/sellable-sku-select-dialog"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { applyDueDateToLines } from "@/features/sales-orders/lib/sales-order-create-model"
import { applySellablePicksToLines } from "@/features/sales-orders/lib/sales-order-create-sku-picks"
import type { SellableSkuPick } from "@/features/sales-orders/lib/sellable-sku-pick"
import type {
    SalesLineProcurementResponsibility,
    SalesProcurementPriceBasis,
} from "@/features/sales-orders/types"
import type {
    InquirySalesBasis,
    InquirySalesSourceSnapshot,
} from "../api/inquiry-basis"
import { supplyReferencePrice } from "../lib/sales-procurement-price"
import { InquiryDeliveryAllocationEditor } from "./inquiry-delivery-allocation-editor"
import { SalesDeliveryAllocationEditor } from "./sales-delivery-allocation-editor"
import { SalesOrderCreatePriceBasisBatchMenu } from "./sales-order-create-price-basis-batch-menu"

export type SalesOrderCreateLineItemsSectionProps = {
    form: SalesOrderCreateFormApi
    draftId?: string
    inquirySource?: InquirySalesBasis | InquirySalesSourceSnapshot
    inquiryLocked?: boolean
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
    procurementFetching?: boolean
    procurementError?: boolean
}

type SkuPickerState = { mode: "add" } | { mode: "replace"; rowIndex: number }

export function SalesOrderCreateLineItemsSection({
    form,
    draftId,
    inquirySource,
    inquiryLocked = false,
    procurementOwners,
    procurementFetching = false,
    procurementError = false,
}: SalesOrderCreateLineItemsSectionProps) {
    const [remarkOpen, setRemarkOpen] = React.useState(
        Boolean(form.getFieldValue("remark")),
    )
    const [picker, setPicker] = React.useState<SkuPickerState | null>(null)

    const handleConfirmPicks = React.useCallback(
        (picks: readonly SellableSkuPick[]) => {
            if (picks.length === 0) return
            const nature = form.getFieldValue("nature")
            const lineItems = form.getFieldValue("lineItems")
            form.setFieldValue(
                "lineItems",
                applySellablePicksToLines(
                    lineItems,
                    picks,
                    nature,
                    picker?.mode === "replace" ? picker.rowIndex : undefined,
                ),
            )
        },
        [form, picker],
    )

    const handleApplyDueDate = React.useCallback(
        (dueDate: string) => {
            const lineItems = form.getFieldValue("lineItems")
            const next = applyDueDateToLines(lineItems, dueDate)
            form.setFieldValue("lineItems", next)
            toast.add({
                title: "已批量设置交期",
                description: `已将 ${next.length} 条明细的期望交付日设为 ${dueDate}。`,
                type: "success",
                timeout: 3000,
            })
        },
        [form],
    )

    const handleApplyPriceBasis = React.useCallback(
        (basis: SalesProcurementPriceBasis) => {
            if (
                inquiryLocked ||
                form.getFieldValue("nature") !== "physical_service"
            )
                return
            const lines = form.getFieldValue("lineItems")
            if (lines.length === 0) return
            const next = lines.map((line) => ({
                ...line,
                procurementPriceBasis: basis,
                procurementSupplyPrice: supplyReferencePrice(
                    line.referencePrices,
                    basis,
                ),
            }))
            form.setFieldValue("lineItems", next)
            const missingCount = next.filter(
                (line) => !line.procurementSupplyPrice.trim(),
            ).length
            const label = basis === "BULK" ? "集采" : "一件代发"
            toast.add({
                title: "已批量设置取价方式",
                description: `已将 ${next.length} 条明细设为${label}并更新供给单价。${missingCount > 0 ? `其中 ${missingCount} 条缺少对应参考价，请补填供给单价。` : ""}`,
                type: "success",
                timeout: missingCount > 0 ? 6000 : 3000,
            })
        },
        [form, inquiryLocked],
    )

    return (
        <>
            <form.Subscribe
                selector={(state) => ({
                    nature: state.values.nature,
                    lines: state.values.lineItems,
                })}
            >
                {({ nature, lines }) => {
                    const skuCount = lines.filter((line) =>
                        line.sku.trim(),
                    ).length
                    return (
                        <DetailWorkspaceSection
                            id="sales-line-items-section"
                            tabIndex={-1}
                            title="销售明细"
                            headingId="sales-create-lines-title"
                            width="full"
                            badge={
                                <span className="num rounded-sm bg-muted/80 px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                                    {nature === "card_voucher" ? (
                                        "卡券 · 单行明细"
                                    ) : (
                                        <>
                                            <QuantityValue
                                                unit=""
                                                value={String(skuCount)}
                                            />{" "}
                                            项商品
                                        </>
                                    )}
                                </span>
                            }
                            actions={
                                nature === "physical_service" &&
                                !inquiryLocked ? (
                                    <div
                                        role="group"
                                        aria-label="销售明细操作"
                                        className="flex flex-wrap items-center gap-1.5"
                                    >
                                        {lines.length > 0 ? (
                                            <>
                                                <SalesOrderCreateDueDateBatchBar
                                                    lineCount={lines.length}
                                                    onApply={handleApplyDueDate}
                                                />
                                                <SalesOrderCreatePriceBasisBatchMenu
                                                    lineCount={lines.length}
                                                    onApply={
                                                        handleApplyPriceBasis
                                                    }
                                                />
                                            </>
                                        ) : null}
                                        <Button
                                            id="sales-orders-create-line-items-add"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() =>
                                                setPicker({ mode: "add" })
                                            }
                                        >
                                            <PlusIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            添加商品
                                        </Button>
                                    </div>
                                ) : undefined
                            }
                        >
                            {nature === "physical_service" &&
                            lines.length === 0 ? (
                                <div className="flex flex-col items-center justify-center gap-3 border-y border-border/70 py-10 text-center">
                                    <div className="flex size-10 items-center justify-center rounded-full bg-muted/70 text-muted-foreground">
                                        <PackageSearchIcon
                                            className="size-5"
                                            aria-hidden="true"
                                        />
                                    </div>
                                    <div className="space-y-1">
                                        <p className="font-medium text-foreground">
                                            尚未添加销售商品
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            从公司商品池选择商品后，填写数量、含税成交单价与期望交付日
                                        </p>
                                    </div>
                                    {!inquiryLocked ? (
                                        <Button
                                            id="sales-orders-create-line-items-empty-add"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() =>
                                                setPicker({ mode: "add" })
                                            }
                                        >
                                            <PlusIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            添加商品
                                        </Button>
                                    ) : null}
                                </div>
                            ) : null}

                            <SalesOrderCreateLineItemTable
                                form={form}
                                inquiryLocked={inquiryLocked}
                                procurementOwners={procurementOwners}
                                procurementFetching={procurementFetching}
                                procurementError={procurementError}
                                onPickSku={(rowIndex) =>
                                    setPicker({ mode: "replace", rowIndex })
                                }
                            />

                            <SellableSkuSelectDialog
                                open={picker != null}
                                onOpenChange={(open) => {
                                    if (!open) setPicker(null)
                                }}
                                multiple={picker?.mode !== "replace"}
                                excludeProductKind="VOUCHER"
                                title={
                                    picker?.mode === "replace"
                                        ? "更换销售商品"
                                        : "添加商品"
                                }
                                onConfirm={handleConfirmPicks}
                            />
                        </DetailWorkspaceSection>
                    )
                }}
            </form.Subscribe>

            {inquirySource ? (
                <InquiryDeliveryAllocationEditor
                    form={form}
                    source={inquirySource}
                />
            ) : (
                <SalesDeliveryAllocationEditor form={form} draftId={draftId} />
            )}

            <form.AppField name="remark">
                {(field) => (
                    <DetailWorkspaceSection width="form" className="py-4!">
                        <Collapsible
                            open={
                                remarkOpen || field.state.meta.errors.length > 0
                            }
                            onOpenChange={setRemarkOpen}
                        >
                            <CollapsibleTrigger
                                id="sales-orders-create-remark-toggle"
                                render={
                                    <Button
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        className={`-ml-2.5 ${detailWorkspaceStyles.quietButton}`}
                                    />
                                }
                            >
                                <ChevronDownIcon
                                    className="size-4 transition-transform in-data-[panel-open]:rotate-180"
                                    aria-hidden="true"
                                />
                                {field.state.value
                                    ? "内部说明（已填写）"
                                    : "添加内部说明（选填）"}
                            </CollapsibleTrigger>
                            <CollapsibleContent keepMounted>
                                <div
                                    data-slot="detail-form-grid"
                                    className="pt-2.5"
                                >
                                    <field.TextareaField
                                        id="sales-orders-create-remark"
                                        label="内部说明（选填）"
                                        hideLabel
                                        placeholder="补充客户确认、交付或内部协同说明"
                                        rows={2}
                                    />
                                </div>
                            </CollapsibleContent>
                        </Collapsible>
                    </DetailWorkspaceSection>
                )}
            </form.AppField>
        </>
    )
}
