"use client"

/**
 * 商品详情页 = 查看 + 编辑（同一页面）。
 * - /master-data/products/new  新建
 * - /master-data/products/:id  查看并直接改，保存即形成新版本
 * 不使用侧边 sheet，也不再有单独的 ?mode=edit。
 * 中间态在 ProductDetailEntryGate，值绑定在 createProductFormBindings，
 * 分区 UI 在 product-detail-{header,feedback,dialogs}.tsx 与 product-editor-sections。
 */

import * as React from "react"

import { PageScaffold, surfacePanelClassName } from "@/components/business"
import {
    ProductBasicSection,
    ProductEffectiveSection,
    ProductHistorySection,
    ProductMediaSection,
} from "@/features/master-data/components/product/product-editor-sections"
import { ProductSkuSection } from "@/features/master-data/components/product/product-sku-section"
import { ProductDetailDialogs } from "@/features/master-data/components/product/product-detail-dialogs"
import { ProductDetailEntryGate } from "@/features/master-data/components/product/product-detail-entry-gate"
import { ProductDetailFeedback } from "@/features/master-data/components/product/product-detail-feedback"
import { ProductDetailHeader } from "@/features/master-data/components/product/product-detail-header"
import { createProductFormBindings } from "@/features/master-data/lib/product-form-bindings"
import { useProductEditor } from "@/features/master-data/hooks/use-product-editor"
import { cn } from "@/lib/utils"

export function ProductDetailPage({ stableId }: { stableId: string }) {
    const editor = useProductEditor(stableId)
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
        stickyHeaderRef,
        rememberPendingFiles,
        rememberSkuFile,
        navigateAway,
        openInventoryPreview,
        handleInventoryOpenChange,
        listHref,
        sectionScrollMarginPx,
        pending,
        canCreate,
        hasUpdatePermission,
        canRevise,
        canDisable,
        reviseBlocker,
        disableBlocker,
        runLocalCheck,
    } = editor

    const formId = "product-detail-form"

    return (
        <ProductDetailEntryGate
            isCreate={isCreate}
            hasDetailData={Boolean(data)}
            detailQuery={detailQuery}
            accountQuery={accountQuery}
            canCreate={canCreate}
            listHref={listHref}
        >
            <form.Subscribe selector={(state) => state.values}>
                {(values) => {
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
                        updateSku,
                        handleSubmit,
                        name,
                        effectiveFrom,
                        effectiveTo,
                        changeReason,
                        specDrafts,
                        activeSpecs,
                        applyBatchReferencePrices,
                    } = bindings
                    return (
                        <PageScaffold
                            style={
                                {
                                    "--product-section-scroll-margin": `${sectionScrollMarginPx}px`,
                                } as React.CSSProperties
                            }
                        >
                            <form
                                id={formId}
                                className="flex flex-col gap-4"
                                onSubmit={handleSubmit}
                            >
                                <ProductDetailHeader
                                    isCreate={isCreate}
                                    data={data}
                                    title={title}
                                    hasUpdatePermission={hasUpdatePermission}
                                    canDisable={canDisable}
                                    disableBlocker={disableBlocker}
                                    setDisableOpen={setDisableOpen}
                                    canRevise={canRevise}
                                    pending={pending}
                                    runLocalCheck={runLocalCheck}
                                    values={values}
                                    onBack={() => navigateAway(listHref)}
                                    activeSection={activeSection}
                                    setActiveSection={setActiveSection}
                                    stickyHeaderRef={stickyHeaderRef}
                                />

                                <div className="flex min-w-0 flex-col gap-4">
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

                                    <div
                                        className={cn(
                                            surfacePanelClassName,
                                            "overflow-hidden",
                                        )}
                                    >
                                        <ProductBasicSection
                                            isCreate={isCreate}
                                            canRevise={canRevise}
                                            name={name}
                                            setName={setName}
                                            fields={fields}
                                            setFields={setFields}
                                            unitOptions={unitOptionsQuery.data}
                                            categoryOptions={categoryOptions}
                                            brandOptions={brandOptions}
                                            categoryLoading={
                                                categoryListQuery.isPending
                                            }
                                            brandLoading={
                                                brandListQuery.isPending
                                            }
                                        />

                                        <ProductMediaSection
                                            canRevise={canRevise}
                                            fields={fields}
                                            setFields={setFields}
                                            rememberPendingFiles={
                                                rememberPendingFiles
                                            }
                                        />
                                        <ProductSkuSection
                                            isCreate={isCreate}
                                            canRevise={canRevise}
                                            name={name}
                                            fields={fields}
                                            specDrafts={specDrafts}
                                            activeSpecs={activeSpecs}
                                            inventoryPreviewSkus={
                                                inventoryPreviewSkus
                                            }
                                            syncSpecDrafts={syncSpecDrafts}
                                            updateSku={updateSku}
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
                                                openInventoryPreview
                                            }
                                            rememberSkuFile={rememberSkuFile}
                                            supplierCounts={
                                                supplierCountsQuery.data
                                            }
                                            supplierCountsPending={
                                                supplierCountsQuery.isPending
                                            }
                                            supplierCountsError={
                                                supplierCountsQuery.isError
                                                    ? supplierCountsQuery.error
                                                    : null
                                            }
                                            onRegisterSupply={
                                                setSupplierDialogSku
                                            }
                                            stableId={stableId}
                                        />

                                        <ProductEffectiveSection
                                            isCreate={isCreate}
                                            canRevise={canRevise}
                                            effectiveFrom={effectiveFrom}
                                            effectiveTo={effectiveTo}
                                            changeReason={changeReason}
                                            setEffectiveFrom={setEffectiveFrom}
                                            setEffectiveTo={setEffectiveTo}
                                            setChangeReason={setChangeReason}
                                        />

                                        {!isCreate ? (
                                            <ProductHistorySection data={data} />
                                        ) : null}
                                    </div>
                                </div>
                            </form>

                            <ProductDetailDialogs
                                isCreate={isCreate}
                                data={data}
                                disableOpen={disableOpen}
                                setDisableOpen={setDisableOpen}
                                supplierDialogSku={supplierDialogSku}
                                setSupplierDialogSku={setSupplierDialogSku}
                                inventoryOpen={inventoryOpen}
                                onInventoryOpenChange={handleInventoryOpenChange}
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
