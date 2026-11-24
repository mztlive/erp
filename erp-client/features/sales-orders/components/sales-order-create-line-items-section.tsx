"use client"

import * as React from "react"
import { PackageSearchIcon, PlusIcon } from "lucide-react"

import { DetailWorkspaceSection, QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
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
import {
    inquiryDeliveryOptions,
    type InquirySalesBasis,
    type InquirySalesSourceSnapshot,
} from "../api/inquiry-basis"
import { useDeliveryCoverage } from "../hooks/use-delivery-coverage"
import type { SalesLineSkuDetails } from "../hooks/use-sales-line-images-query"
import type { SalesLineFilterKey } from "../lib/sales-order-create-line-health"
import { supplyReferencePrice } from "../lib/sales-procurement-price"
import { SalesDeliverySummaryBar } from "./sales-delivery-allocation-editor"
import {
    SalesDeliveryWorkspaceSheet,
    type DeliveryWorkspaceTab,
} from "./sales-delivery-workspace-sheet"
import { SalesOrderCreatePriceBasisBatchMenu } from "./sales-order-create-price-basis-batch-menu"

export type SalesOrderCreateLineItemsSectionProps = {
    form: SalesOrderCreateFormApi
    skuDetails: SalesLineSkuDetails
    draftId?: string
    inquirySource?: InquirySalesBasis | InquirySalesSourceSnapshot
    inquiryLocked?: boolean
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
    procurementFetching?: boolean
    procurementError?: boolean
    activeFilter?: SalesLineFilterKey
    onActiveFilterChange?: (filter: SalesLineFilterKey) => void
    /** 从底部汇总栏触发打开收货地址总览工作台。 */
    deliverySheetRequest?: {
        nonce: number
        tab: DeliveryWorkspaceTab
        rowKey?: string | null
    } | null
}

type SkuPickerState = { mode: "add" } | { mode: "replace"; rowIndex: number }

export function SalesOrderCreateLineItemsSection({
    form,
    skuDetails,
    draftId,
    inquirySource,
    inquiryLocked = false,
    procurementOwners,
    procurementFetching = false,
    procurementError = false,
    activeFilter: controlledFilter,
    onActiveFilterChange,
    deliverySheetRequest,
}: SalesOrderCreateLineItemsSectionProps) {
    const [picker, setPicker] = React.useState<SkuPickerState | null>(null)
    const [uncontrolledFilter, setUncontrolledFilter] =
        React.useState<SalesLineFilterKey>("all")
    const activeFilter = controlledFilter ?? uncontrolledFilter
    const setActiveFilter = onActiveFilterChange ?? setUncontrolledFilter

    const [deliveryOpen, setDeliveryOpen] = React.useState(false)
    const [deliveryTab, setDeliveryTab] =
        React.useState<DeliveryWorkspaceTab>("product")
    const [activeDeliveryRowKey, setActiveDeliveryRowKey] = React.useState<
        string | null
    >(null)

    React.useEffect(() => {
        if (!deliverySheetRequest) return
        setDeliveryTab(deliverySheetRequest.tab)
        if (deliverySheetRequest.rowKey !== undefined) {
            setActiveDeliveryRowKey(deliverySheetRequest.rowKey)
        }
        setDeliveryOpen(true)
    }, [deliverySheetRequest])

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
            if (
                picker?.mode === "add" &&
                (activeFilter === "negative_profit" ||
                    activeFilter === "address_issue")
            ) {
                setActiveFilter("all")
            }
        },
        [activeFilter, form, picker, setActiveFilter],
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

    const openProductDelivery = React.useCallback((rowKey: string) => {
        setActiveDeliveryRowKey(rowKey)
        setDeliveryTab("product")
        setDeliveryOpen(true)
    }, [])

    const openOverviewDelivery = React.useCallback(() => {
        setDeliveryTab(inquirySource ? "product" : "overview")
        setDeliveryOpen(true)
    }, [inquirySource])

    return (
        <form.Subscribe
            selector={(state) => ({
                nature: state.values.nature,
                lines: state.values.lineItems,
            })}
        >
            {({ nature, lines }) => (
                <LineItemsSectionInner
                    form={form}
                    skuDetails={skuDetails}
                    nature={nature}
                    lines={lines}
                    draftId={draftId}
                    inquirySource={inquirySource}
                    inquiryLocked={inquiryLocked}
                    procurementOwners={procurementOwners}
                    procurementFetching={procurementFetching}
                    procurementError={procurementError}
                    activeFilter={activeFilter}
                    onActiveFilterChange={setActiveFilter}
                    picker={picker}
                    onPickerChange={setPicker}
                    deliveryOpen={deliveryOpen}
                    onDeliveryOpenChange={setDeliveryOpen}
                    deliveryTab={deliveryTab}
                    onDeliveryTabChange={setDeliveryTab}
                    activeDeliveryRowKey={activeDeliveryRowKey}
                    onActiveDeliveryRowKeyChange={setActiveDeliveryRowKey}
                    onConfirmPicks={handleConfirmPicks}
                    onApplyDueDate={handleApplyDueDate}
                    onApplyPriceBasis={handleApplyPriceBasis}
                    onOpenProductDelivery={openProductDelivery}
                    onOpenOverviewDelivery={openOverviewDelivery}
                />
            )}
        </form.Subscribe>
    )
}

function LineItemsSectionInner({
    form,
    skuDetails,
    nature,
    lines,
    draftId,
    inquirySource,
    inquiryLocked,
    procurementOwners,
    procurementFetching,
    procurementError,
    activeFilter,
    onActiveFilterChange,
    picker,
    onPickerChange,
    deliveryOpen,
    onDeliveryOpenChange,
    deliveryTab,
    onDeliveryTabChange,
    activeDeliveryRowKey,
    onActiveDeliveryRowKeyChange,
    onConfirmPicks,
    onApplyDueDate,
    onApplyPriceBasis,
    onOpenProductDelivery,
    onOpenOverviewDelivery,
}: {
    form: SalesOrderCreateFormApi
    skuDetails: SalesLineSkuDetails
    nature: "physical_service" | "card_voucher"
    lines: ReturnType<SalesOrderCreateFormApi["getFieldValue"]> extends never
        ? never
        : Parameters<typeof applyDueDateToLines>[0] extends readonly (infer T)[]
          ? T[]
          : never
    draftId?: string
    inquirySource?: InquirySalesBasis | InquirySalesSourceSnapshot
    inquiryLocked: boolean
    procurementOwners?: ReadonlyMap<string, SalesLineProcurementResponsibility>
    procurementFetching: boolean
    procurementError: boolean
    activeFilter: SalesLineFilterKey
    onActiveFilterChange: (filter: SalesLineFilterKey) => void
    picker: SkuPickerState | null
    onPickerChange: (state: SkuPickerState | null) => void
    deliveryOpen: boolean
    onDeliveryOpenChange: (open: boolean) => void
    deliveryTab: DeliveryWorkspaceTab
    onDeliveryTabChange: (tab: DeliveryWorkspaceTab) => void
    activeDeliveryRowKey: string | null
    onActiveDeliveryRowKeyChange: (key: string | null) => void
    onConfirmPicks: (picks: readonly SellableSkuPick[]) => void
    onApplyDueDate: (dueDate: string) => void
    onApplyPriceBasis: (basis: SalesProcurementPriceBasis) => void
    onOpenProductDelivery: (rowKey: string) => void
    onOpenOverviewDelivery: () => void
}) {
    const coverageHint = useDeliveryCoverage(
        nature === "physical_service" && !inquiryLocked ? lines : [],
    )
    const inquiryDelivery = React.useMemo(
        () => inquirySource && inquiryDeliveryOptions(inquirySource),
        [inquirySource],
    )
    const inquiryDeliveryLineKeys = React.useMemo(
        () => new Set(inquiryDelivery?.keys()),
        [inquiryDelivery],
    )
    const skuCount = lines.filter((line) => line.sku.trim()).length

    return (
        <>
            <DetailWorkspaceSection
                id="sales-line-items-section"
                tabIndex={-1}
                title="销售明细"
                headingId="sales-create-lines-title"
                width="full"
                className="py-4 last:pb-6"
                contentClassName="space-y-3"
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
                    nature === "physical_service" && !inquiryLocked ? (
                        <div
                            role="group"
                            aria-label="销售明细操作"
                            className="flex flex-wrap items-center gap-1.5"
                        >
                            {lines.length > 0 ? (
                                <>
                                    <SalesOrderCreateDueDateBatchBar
                                        lineCount={lines.length}
                                        onApply={onApplyDueDate}
                                    />
                                    <SalesOrderCreatePriceBasisBatchMenu
                                        lineCount={lines.length}
                                        onApply={onApplyPriceBasis}
                                    />
                                </>
                            ) : null}
                            <Button
                                id="sales-orders-create-line-items-add"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => onPickerChange({ mode: "add" })}
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
                {nature === "physical_service" && lines.length > 0 ? (
                    <SalesDeliverySummaryBar
                        unitNames={skuDetails.unitNames}
                        form={form}
                        lines={lines}
                        draftId={draftId}
                        inquiryLocked={inquiryLocked}
                        coverageHint={coverageHint}
                        onOpenOverview={onOpenOverviewDelivery}
                        onFilterLines={onActiveFilterChange}
                    />
                ) : null}

                {nature === "physical_service" && lines.length === 0 ? (
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
                                从公司商品池批量选择商品，填写数量、含税成交单价、期望交付日与收货地址
                            </p>
                        </div>
                        {!inquiryLocked ? (
                            <Button
                                id="sales-orders-create-line-items-empty-add"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => onPickerChange({ mode: "add" })}
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
                    skuDetails={skuDetails}
                    inquiryLocked={inquiryLocked}
                    inquiryDeliveryLineKeys={inquiryDeliveryLineKeys}
                    procurementOwners={procurementOwners}
                    procurementFetching={procurementFetching}
                    procurementError={procurementError}
                    coverageHint={coverageHint}
                    activeFilter={activeFilter}
                    onActiveFilterChange={onActiveFilterChange}
                    onAddProducts={() => onPickerChange({ mode: "add" })}
                    onOpenDelivery={onOpenProductDelivery}
                    onPickSku={(rowIndex) =>
                        onPickerChange({ mode: "replace", rowIndex })
                    }
                />

                <SellableSkuSelectDialog
                    open={picker != null}
                    onOpenChange={(open) => {
                        if (!open) onPickerChange(null)
                    }}
                    multiple={picker?.mode !== "replace"}
                    excludeProductKind="VOUCHER"
                    title={
                        picker?.mode === "replace" ? "更换销售商品" : "添加商品"
                    }
                    onConfirm={onConfirmPicks}
                />
            </DetailWorkspaceSection>

            {nature === "physical_service" ? (
                <SalesDeliveryWorkspaceSheet
                    open={deliveryOpen}
                    unitNames={skuDetails.unitNames}
                    onOpenChange={onDeliveryOpenChange}
                    tab={deliveryTab}
                    onTabChange={onDeliveryTabChange}
                    activeRowKey={activeDeliveryRowKey}
                    onActiveRowKeyChange={onActiveDeliveryRowKeyChange}
                    form={form}
                    lines={lines}
                    inquiryDelivery={inquiryDelivery}
                    onJumpToTableFilter={onActiveFilterChange}
                />
            ) : null}
        </>
    )
}
