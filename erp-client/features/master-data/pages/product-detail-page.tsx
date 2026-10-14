"use client"

import { useChangeConfirmation } from "../hooks/use-change-confirmation"

import * as React from "react"
import {
    DiscardConfirmDialog,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    ProductBasicSection,
    ProductHistorySection,
    ProductMediaSection,
} from "@/features/master-data/components/product/product-editor-sections"
import { ProductSkuSection } from "@/features/master-data/components/product/product-sku-section"
import { ProductSupplySection } from "@/features/master-data/components/product/product-supply-section"
import { ProductDetailDialogs } from "@/features/master-data/components/product/product-detail-dialogs"
import { ProductDetailEntryGate } from "@/features/master-data/components/product/product-detail-entry-gate"
import { ProductDetailFeedback } from "@/features/master-data/components/product/product-detail-feedback"
import { ProductDetailHeader } from "@/features/master-data/components/product/product-detail-header"
import { ProductSectionTabs } from "@/features/master-data/components/product/product-detail-navigation"
import { ProductSaveDialog } from "@/features/master-data/components/product/product-save-dialog"
import { createProductFormBindings } from "@/features/master-data/lib/product-form-bindings"
import { productChangeSummary } from "@/features/master-data/lib/product-change-summary"
import { useProductEditor } from "@/features/master-data/hooks/use-product-editor"
import { cn } from "@/lib/utils"

/** 商品资料分区卡片：基础信息与规格各成一块，避免整页平铺。 */
const productCardClassName = "rounded-lg border bg-card p-5 md:p-6"

export function ProductDetailPage({ stableId }: { stableId: string }) {
    const editor = useProductEditor(stableId)
    const changeConfirmation = useChangeConfirmation(
        "master-data-product-change",
    )
    const [priceUndo, setPriceUndo] = React.useState<{
        undo: () => void
    } | null>(null)
    React.useEffect(() => {
        if (!priceUndo) return
        const timer = setTimeout(() => setPriceUndo(null), 8000)
        return () => clearTimeout(timer)
    }, [priceUndo])
    React.useEffect(() => {
        setPriceUndo(null)
    }, [stableId, editor.saveOpen])
    const [resetOpen, setResetOpen] = React.useState(false)
    const {
        isCreate,
        router,
        accountQuery,
        detailQuery,
        categoryListQuery,
        brandListQuery,
        categoryOptions,
        brandOptions,
        unitOptionsQuery,
        data,
        form,
        formError,
        formErrorTitle,
        checkPassed,
        result,
        disableOpen,
        setDisableOpen,
        discardOpen,
        setDiscardOpen,
        pendingNav,
        setPendingNav,
        supplierDialogSku,
        setSupplierDialogSku,
        inventoryOpen,
        inventoryInitialSkuId,
        activeSection,
        setActiveSection,
        errorRef,
        checkedSnapshotRef,
        rememberPendingFiles,
        rememberSkuFile,
        navigateAway,
        openInventoryPreview,
        handleInventoryOpenChange,
        listHref,
        pending,
        canCreate,
        canRevise,
        canDisable,
        reviseBlocker,
        disableBlocker,
        saveOpen,
        setSaveOpen,
        saveAttempted,
        requestSave,
        initialFormValues,
    } = editor

    React.useEffect(() => {
        if (activeSection !== "sku") return
        const frame = window.requestAnimationFrame(() => {
            document
                .getElementById("product-section-sku")
                ?.scrollIntoView({ block: "start" })
        })
        return () => window.cancelAnimationFrame(frame)
    }, [activeSection, data?.stableId])

    return (
        <ProductDetailEntryGate
            isCreate={isCreate}
            hasDetailData={Boolean(data)}
            detailQuery={detailQuery}
            accountQuery={accountQuery}
            canCreate={canCreate}
            listHref={listHref}
        >
            <form.Subscribe
                selector={(state) =>
                    [state.values, state.isSubmitting] as const
                }
            >
                {([values, isSubmitting]) => {
                    const bindings = createProductFormBindings(
                        form,
                        values,
                        isCreate,
                        data?.name,
                        changeConfirmation.confirm,
                        (undo) => setPriceUndo({ undo }),
                    )
                    const {
                        title,
                        fields,
                        inventoryPreviewSkus,
                        inventoryActionHint,
                        setName,
                        setEffectiveFrom,
                        setEffectiveTo,
                        setChangeReason,
                        setFields,
                        syncSpecDrafts,
                        applySpecDrafts,
                        resetSpecDrafts,
                        updateSku,
                        name,
                        effectiveFrom,
                        effectiveTo,
                        changeReason,
                        specDrafts,
                        activeSpecs,
                        applyBatchReferencePrices,
                    } = bindings
                    const saving = pending || isSubmitting
                    // Retain old deep links to the effective section, now shown as the save dialog.
                    const showSave = saveOpen || activeSection === "effective"
                    const section =
                        activeSection === "effective" || activeSection === "sku"
                            ? "basic"
                            : activeSection
                    const changes = productChangeSummary(
                        initialFormValues,
                        values,
                    )
                    const closeSave = (open: boolean) => {
                        setSaveOpen(open)
                        if (!open && activeSection === "effective")
                            setActiveSection("basic")
                    }
                    const feedback = (
                        <ProductDetailFeedback
                            isCreate={isCreate}
                            canRevise={canRevise}
                            reviseBlocker={reviseBlocker}
                            result={result}
                            formError={formError}
                            formErrorTitle={formErrorTitle}
                            checkPassed={checkPassed}
                            checkedSnapshotRef={checkedSnapshotRef}
                            values={values}
                            fields={fields}
                            errorRef={errorRef}
                        />
                    )
                    return (
                        <PageScaffold density="compact">
                            <form
                                id="product-detail-form"
                                className="min-w-0 space-y-5"
                                onSubmit={(event) => {
                                    event.preventDefault()
                                    requestSave(values)
                                }}
                            >
                                <ProductDetailHeader
                                    isCreate={isCreate}
                                    data={data}
                                    title={title}
                                    fields={fields}
                                    canDisable={canDisable}
                                    disableBlocker={disableBlocker}
                                    setDisableOpen={setDisableOpen}
                                    canRevise={canRevise}
                                    pending={saving}
                                    onBack={() => navigateAway(listHref)}
                                    onMedia={() => setActiveSection("media")}
                                    onSave={() => requestSave(values)}
                                    onReset={
                                        changes.length > 0
                                            ? () => setResetOpen(true)
                                            : undefined
                                    }
                                />
                                {!showSave ? feedback : null}
                                <div
                                    className={cn(
                                        surfacePanelClassName,
                                        "min-w-0",
                                    )}
                                >
                                    <ProductSectionTabs
                                        value={section}
                                        isCreate={isCreate}
                                        onValueChange={setActiveSection}
                                    />
                                    <div
                                        className={cn(
                                            "min-w-0 p-4 md:p-6",
                                            section === "basic" &&
                                                "bg-muted/20",
                                        )}
                                    >
                                        {section === "basic" ? (
                                            <div className="space-y-5">
                                                <div
                                                    className={
                                                        productCardClassName
                                                    }
                                                >
                                                    <ProductBasicSection
                                                        idPrefix="master-data-product-detail-basic"
                                                        isCreate={isCreate}
                                                        canRevise={canRevise}
                                                        name={name}
                                                        setName={setName}
                                                        fields={fields}
                                                        setFields={setFields}
                                                        unitOptions={
                                                            unitOptionsQuery.data
                                                        }
                                                        categoryOptions={
                                                            categoryOptions
                                                        }
                                                        brandOptions={
                                                            brandOptions
                                                        }
                                                        categoryLoading={
                                                            categoryListQuery.isPending
                                                        }
                                                        brandLoading={
                                                            brandListQuery.isPending
                                                        }
                                                    />
                                                </div>
                                                <div
                                                    className={
                                                        productCardClassName
                                                    }
                                                >
                                                    <ProductSkuSection
                                                        idPrefix="master-data-product-detail-sku"
                                                        isCreate={isCreate}
                                                        canRevise={canRevise}
                                                        name={name}
                                                        fields={fields}
                                                        specDrafts={specDrafts}
                                                        activeSpecs={
                                                            activeSpecs
                                                        }
                                                        inventoryPreviewSkus={
                                                            inventoryPreviewSkus
                                                        }
                                                        syncSpecDrafts={
                                                            syncSpecDrafts
                                                        }
                                                        applySpecDrafts={
                                                            applySpecDrafts
                                                        }
                                                        resetSpecDrafts={
                                                            resetSpecDrafts
                                                        }
                                                        updateSku={updateSku}
                                                        batchBulkPrice={
                                                            values.batchBulkPrice
                                                        }
                                                        batchBulkMinQuantity={
                                                            values.batchBulkMinQuantity
                                                        }
                                                        setBatchBulkPrice={(
                                                            next,
                                                        ) =>
                                                            form.setFieldValue(
                                                                "batchBulkPrice",
                                                                next,
                                                            )
                                                        }
                                                        setBatchBulkMinQuantity={(
                                                            next,
                                                        ) =>
                                                            form.setFieldValue(
                                                                "batchBulkMinQuantity",
                                                                next,
                                                            )
                                                        }
                                                        batchSalePrice={
                                                            values.batchSalePrice
                                                        }
                                                        batchMarketPrice={
                                                            values.batchMarketPrice
                                                        }
                                                        setBatchSalePrice={(
                                                            next,
                                                        ) =>
                                                            form.setFieldValue(
                                                                "batchSalePrice",
                                                                next,
                                                            )
                                                        }
                                                        setBatchMarketPrice={(
                                                            next,
                                                        ) =>
                                                            form.setFieldValue(
                                                                "batchMarketPrice",
                                                                next,
                                                            )
                                                        }
                                                        onApplyBatchReferencePrices={
                                                            applyBatchReferencePrices
                                                        }
                                                        inventoryActionHint={
                                                            inventoryActionHint
                                                        }
                                                        onOpenInventory={
                                                            openInventoryPreview
                                                        }
                                                        rememberSkuFile={
                                                            rememberSkuFile
                                                        }
                                                    />
                                                </div>
                                            </div>
                                        ) : null}
                                        {section === "supply" && !isCreate ? (
                                            <ProductSupplySection
                                                fields={fields}
                                                name={name}
                                                onRegisterSupply={
                                                    setSupplierDialogSku
                                                }
                                            />
                                        ) : null}
                                        {section === "media" ? (
                                            <ProductMediaSection
                                                idPrefix="master-data-product-detail-media"
                                                canRevise={canRevise}
                                                fields={fields}
                                                setFields={setFields}
                                                rememberPendingFiles={
                                                    rememberPendingFiles
                                                }
                                            />
                                        ) : null}
                                        {section === "history" && !isCreate ? (
                                            <ProductHistorySection
                                                data={data}
                                            />
                                        ) : null}
                                    </div>
                                </div>
                            </form>
                            {changeConfirmation.dialog}
                            {priceUndo ? (
                                <div
                                    role="status"
                                    className="fixed bottom-20 right-6 z-40 flex items-center gap-3 rounded-lg border bg-background p-3 shadow-md"
                                >
                                    批量价格已应用
                                    <Button
                                        id="master-data-product-price-undo"
                                        size="sm"
                                        variant="outline"
                                        onClick={() => {
                                            priceUndo.undo()
                                            setPriceUndo(null)
                                        }}
                                    >
                                        撤销
                                    </Button>
                                </div>
                            ) : null}
                            <ProductSaveDialog
                                open={showSave}
                                onOpenChange={closeSave}
                                pending={saving}
                                changes={changes}
                                onConfirm={() => {
                                    void form.handleSubmit()
                                }}
                                feedback={feedback}
                                attempted={saveAttempted}
                                isCreate={isCreate}
                                canRevise={canRevise}
                                effectiveFrom={effectiveFrom}
                                effectiveTo={effectiveTo}
                                changeReason={changeReason}
                                setEffectiveFrom={setEffectiveFrom}
                                setEffectiveTo={setEffectiveTo}
                                setChangeReason={setChangeReason}
                            />
                            <DiscardConfirmDialog
                                open={resetOpen}
                                onOpenChange={setResetOpen}
                                title="取消本次修改？"
                                description="商品资料将恢复到本次编辑前的内容。"
                                confirmLabel="取消修改"
                                cancelLabel="继续编辑"
                                onConfirm={() => {
                                    form.reset(initialFormValues)
                                    editor.setFormError(null)
                                    editor.setResult(null)
                                    setResetOpen(false)
                                }}
                            />
                            <ProductDetailDialogs
                                isCreate={isCreate}
                                data={data}
                                disableOpen={disableOpen}
                                setDisableOpen={setDisableOpen}
                                supplierDialogSku={supplierDialogSku}
                                setSupplierDialogSku={setSupplierDialogSku}
                                inventoryOpen={inventoryOpen}
                                onInventoryOpenChange={
                                    handleInventoryOpenChange
                                }
                                productName={title}
                                productKind={fields.productKind}
                                inventoryPreviewSkus={inventoryPreviewSkus}
                                inventoryInitialSkuId={inventoryInitialSkuId}
                                discardOpen={discardOpen}
                                setDiscardOpen={setDiscardOpen}
                                pendingNav={pendingNav}
                                setPendingNav={setPendingNav}
                                router={router}
                            />
                        </PageScaffold>
                    )
                }}
            </form.Subscribe>
        </ProductDetailEntryGate>
    )
}
