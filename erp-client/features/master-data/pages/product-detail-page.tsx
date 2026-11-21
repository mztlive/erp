"use client"

import * as React from "react"
import { useSearchParams } from "next/navigation"

import {
    DetailWorkspaceBody,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DiscardConfirmDialog,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { ProductDetailDialogs } from "@/features/master-data/components/product/product-detail-dialogs"
import { ProductDetailEntryGate } from "@/features/master-data/components/product/product-detail-entry-gate"
import { ProductDetailFeedback } from "@/features/master-data/components/product/product-detail-feedback"
import { ProductDetailHeader } from "@/features/master-data/components/product/product-detail-header"
import { ProductSectionTabs } from "@/features/master-data/components/product/product-detail-navigation"
import {
    ProductBasicSection,
    ProductHistorySection,
    ProductMediaSection,
} from "@/features/master-data/components/product/product-editor-sections"
import { ProductSaveDialog } from "@/features/master-data/components/product/product-save-dialog"
import { ProductSkuSection } from "@/features/master-data/components/product/product-sku-section"
import { ProductSourceDiff } from "@/features/master-data/components/product/product-source-diff"
import { ProductSupplySection } from "@/features/master-data/components/product/product-supply-section"
import { useProductEditor } from "@/features/master-data/hooks/use-product-editor"
import { productChangeSummary } from "@/features/master-data/lib/product-change-summary"
import { createProductFormBindings } from "@/features/master-data/lib/product-form-bindings"
import { hasPermission } from "@/lib/permissions"
import { useChangeConfirmation } from "../hooks/use-change-confirmation"

export function ProductDetailPage({ stableId }: { stableId: string }) {
    const sourceDiffRequested = useSearchParams().get("sourceDiff") === "true"
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
    const [sourceProcessing, setSourceProcessing] = React.useState(false)
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
        canSelectUnit,
        canSelectCategory,
        canSelectBrand,
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
        hasUpdatePermission,
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

    const permissions = accountQuery.isSuccess
        ? accountQuery.data.permissions
        : undefined
    const canReadSupply = hasPermission(permissions, "supplier_offering:list")
    const canReadInventory = hasPermission(permissions, "stock_balance:list")

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
                    [state.values, state.isSubmitting, state.isDirty] as const
                }
            >
                {([values, isSubmitting, isDirty]) => {
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
                    const saving = pending || isSubmitting || sourceProcessing
                    // Retain old deep links to the effective section, now shown as the save dialog.
                    const showSave =
                        canRevise && (saveOpen || activeSection === "effective")
                    const section =
                        activeSection === "effective" ||
                        activeSection === "sku" ||
                        (activeSection === "supply" && !canReadSupply)
                            ? "basic"
                            : activeSection
                    const changes = productChangeSummary(
                        initialFormValues,
                        values,
                    )
                    const hasUnsavedChanges = changes.length > 0
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
                        <DetailWorkspacePage>
                            <form
                                id="product-detail-form"
                                className="flex min-w-0 flex-1 flex-col"
                                onSubmit={(event) => {
                                    event.preventDefault()
                                    if (!sourceProcessing) requestSave(values)
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
                                    hasUnsavedChanges={hasUnsavedChanges}
                                    onBack={() => navigateAway(listHref)}
                                    onMedia={() => setActiveSection("media")}
                                    onSave={() => requestSave(values)}
                                    onReset={
                                        hasUnsavedChanges
                                            ? () => setResetOpen(true)
                                            : undefined
                                    }
                                    tabs={
                                        <ProductSectionTabs
                                            value={section}
                                            isCreate={isCreate}
                                            canReadSupply={canReadSupply}
                                            onValueChange={setActiveSection}
                                        />
                                    }
                                />

                                <DetailWorkspaceBody
                                    alerts={
                                        <>
                                            {!showSave ? feedback : null}
                                            <ProductSourceDiff
                                                key={stableId}
                                                productId={stableId}
                                                enabled={
                                                    !isCreate && Boolean(data)
                                                }
                                                canResolve={hasUpdatePermission}
                                                canRevise={canRevise}
                                                dirty={isDirty}
                                                busy={saving}
                                                openRequested={
                                                    sourceDiffRequested
                                                }
                                                onProcessingChange={
                                                    setSourceProcessing
                                                }
                                            />
                                        </>
                                    }
                                >
                                    {section === "basic" ? (
                                        <>
                                            <ProductBasicSection
                                                canSelectUnit={canSelectUnit}
                                                canSelectCategory={
                                                    canSelectCategory
                                                }
                                                canSelectBrand={canSelectBrand}
                                                onRefreshUnits={() =>
                                                    unitOptionsQuery.refetch({
                                                        throwOnError: true,
                                                    })
                                                }
                                                onRefreshCategories={() =>
                                                    categoryListQuery.refetch({
                                                        throwOnError: true,
                                                    })
                                                }
                                                onRefreshBrands={() =>
                                                    brandListQuery.refetch({
                                                        throwOnError: true,
                                                    })
                                                }
                                                idPrefix="master-data-product-detail-basic"
                                                isCreate={isCreate}
                                                canRevise={
                                                    canRevise &&
                                                    !sourceProcessing
                                                }
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
                                                brandOptions={brandOptions}
                                                categoryLoading={
                                                    categoryListQuery.isFetching
                                                }
                                                brandLoading={
                                                    brandListQuery.isFetching
                                                }
                                            />
                                            <ProductSkuSection
                                                canEditLifecycle={
                                                    isCreate
                                                        ? canCreate
                                                        : hasUpdatePermission
                                                }
                                                idPrefix="master-data-product-detail-sku"
                                                isCreate={isCreate}
                                                canRevise={
                                                    canRevise &&
                                                    !sourceProcessing
                                                }
                                                name={name}
                                                fields={fields}
                                                specDrafts={specDrafts}
                                                activeSpecs={activeSpecs}
                                                inventoryPreviewSkus={
                                                    inventoryPreviewSkus
                                                }
                                                syncSpecDrafts={syncSpecDrafts}
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
                                                setBatchBulkPrice={(next) =>
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
                                                setBatchSalePrice={(next) =>
                                                    form.setFieldValue(
                                                        "batchSalePrice",
                                                        next,
                                                    )
                                                }
                                                setBatchMarketPrice={(next) =>
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
                                                    canReadInventory
                                                        ? openInventoryPreview
                                                        : undefined
                                                }
                                                rememberSkuFile={
                                                    rememberSkuFile
                                                }
                                            />
                                        </>
                                    ) : null}

                                    {section === "supply" &&
                                    !isCreate &&
                                    canReadSupply ? (
                                        <DetailWorkspaceSection width="full">
                                            <ProductSupplySection
                                                fields={fields}
                                                name={name}
                                                onRegisterSupply={
                                                    setSupplierDialogSku
                                                }
                                            />
                                        </DetailWorkspaceSection>
                                    ) : null}

                                    {section === "media" ? (
                                        <DetailWorkspaceSection width="full">
                                            <ProductMediaSection
                                                idPrefix="master-data-product-detail-media"
                                                disabled={
                                                    pending || sourceProcessing
                                                }
                                                canRevise={canRevise}
                                                fields={fields}
                                                setFields={setFields}
                                                rememberPendingFiles={
                                                    rememberPendingFiles
                                                }
                                            />
                                        </DetailWorkspaceSection>
                                    ) : null}

                                    {section === "history" && !isCreate ? (
                                        <DetailWorkspaceSection width="form">
                                            <ProductHistorySection
                                                data={data}
                                            />
                                        </DetailWorkspaceSection>
                                    ) : null}
                                </DetailWorkspaceBody>
                            </form>

                            {changeConfirmation.dialog}
                            {canRevise && priceUndo ? (
                                <div
                                    role="status"
                                    className="fixed bottom-6 right-6 z-40 flex items-center gap-3 rounded-xl border border-border/90 bg-popover/95 px-3.5 py-2 text-body-compact shadow-lg backdrop-blur-md"
                                >
                                    <span>批量价格已应用</span>
                                    <Button
                                        id="master-data-product-price-undo"
                                        size="sm"
                                        variant="ghost"
                                        className="h-control-sm text-primary hover:bg-sidebar-accent/70 hover:text-primary"
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
                                open={canRevise && resetOpen}
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
                        </DetailWorkspacePage>
                    )
                }}
            </form.Subscribe>
        </ProductDetailEntryGate>
    )
}
