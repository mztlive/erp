"use client"

import * as React from "react"
import { SaveIcon } from "lucide-react"
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
import { ProductDetailDialogs } from "@/features/master-data/components/product/product-detail-dialogs"
import { ProductDetailEntryGate } from "@/features/master-data/components/product/product-detail-entry-gate"
import { ProductDetailFeedback } from "@/features/master-data/components/product/product-detail-feedback"
import { ProductDetailHeader } from "@/features/master-data/components/product/product-detail-header"
import { ProductSectionTabs } from "@/features/master-data/components/product/product-detail-navigation"
import { ProductSaveDialog } from "@/features/master-data/components/product/product-save-dialog"
import { createProductFormBindings } from "@/features/master-data/lib/product-form-bindings"
import { productChangeSummary } from "@/features/master-data/lib/product-change-summary"
import { formatEffectiveRange } from "@/features/master-data/lib/filter"
import { useProductEditor } from "@/features/master-data/hooks/use-product-editor"
import { cn } from "@/lib/utils"

export function ProductDetailPage({ stableId }: { stableId: string }) {
    const editor = useProductEditor(stableId)
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
        supplierCountsQuery,
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
                    [state.values, state.isSubmitting, state.isDirty] as const
                }
            >
                {([values, isSubmitting, isDirty]) => {
                    const bindings = createProductFormBindings(
                        form,
                        values,
                        isCreate,
                        data?.name,
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
                                    <div className="min-w-0 p-4 md:p-6">
                                        {section === "basic" ? (
                                            <div className="space-y-6">
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
                                                    brandOptions={brandOptions}
                                                    categoryLoading={
                                                        categoryListQuery.isPending
                                                    }
                                                    brandLoading={
                                                        brandListQuery.isPending
                                                    }
                                                />
                                                <div className="border-t border-border pt-6">
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
                                                        supplierCounts={
                                                            supplierCountsQuery.data
                                                        }
                                                        supplierCountsPending={
                                                            supplierCountsQuery.isPending
                                                        }
                                                        supplierCountsError={
                                                            supplierCountsQuery.error
                                                        }
                                                        onRegisterSupply={
                                                            setSupplierDialogSku
                                                        }
                                                        stableId={stableId}
                                                    />
                                                </div>
                                            </div>
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
                                {!isCreate && data ? (
                                    <div className="flex flex-wrap justify-between gap-2 text-xs text-muted-foreground">
                                        <span>
                                            版本{" "}
                                            {data.currentRevision.revisionNo} ·{" "}
                                            {formatEffectiveRange(
                                                data.currentRevision
                                                    .effectiveFrom,
                                                data.currentRevision
                                                    .effectiveTo,
                                            )}
                                        </span>
                                        <span>
                                            {data.revisionTimingLabel} ·{" "}
                                            {data.productConstraints
                                                ?.hasFormalReferences
                                                ? "已被业务单据引用"
                                                : "尚未被业务单据引用"}
                                        </span>
                                    </div>
                                ) : null}
                                {isDirty && canRevise ? (
                                    <div
                                        className="sticky bottom-0 z-20 flex flex-wrap items-center gap-3 border-t border-border bg-card py-4"
                                        role="region"
                                        aria-label="未保存的修改"
                                    >
                                        <div className="min-w-0 flex-1">
                                            <p className="text-sm font-medium">
                                                有修改未保存
                                            </p>
                                            <p className="truncate text-xs text-muted-foreground">
                                                {changes
                                                    .slice(0, 2)
                                                    .join(" · ") ||
                                                    "完成编辑后保存更新"}
                                            </p>
                                        </div>
                                        <Button
                                            id="master-data-product-detail-reset"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={saving}
                                            onClick={() => setResetOpen(true)}
                                        >
                                            取消修改
                                        </Button>
                                        <Button
                                            id="master-data-product-detail-save-bottom"
                                            type="button"
                                            size="sm"
                                            disabled={saving}
                                            onClick={() => requestSave(values)}
                                        >
                                            <SaveIcon aria-hidden />
                                            保存更新
                                        </Button>
                                    </div>
                                ) : null}
                            </form>
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
