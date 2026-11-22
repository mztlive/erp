"use client"

import * as React from "react"
import { SaveIcon } from "lucide-react"
import { useSearchParams } from "next/navigation"
import { useSelector } from "@tanstack/react-form"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspacePage,
    DetailWorkspaceSaveDock,
    DetailWorkspaceSection,
    DiscardConfirmDialog,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { toast } from "@/components/ui/toast"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
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
import type { SkuSupplySummary } from "@/features/master-data/components/product/product-sku-table"
import { ProductSourceDiff } from "@/features/master-data/components/product/product-source-diff"
import {
    ProductSupplySection,
    isActiveOffering,
} from "@/features/master-data/components/product/product-supply-section"
import { useProductEditor } from "@/features/master-data/hooks/use-product-editor"
import { productChangeSummary } from "@/features/master-data/lib/product-change-summary"
import { createProductFormBindings } from "@/features/master-data/lib/product-form-bindings"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/queries"
import { compareDecimal } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { useChangeConfirmation } from "../hooks/use-change-confirmation"

export function ProductDetailPage({ stableId }: { stableId: string }) {
    const sourceDiffRequested = useSearchParams().get("sourceDiff") === "true"
    const editor = useProductEditor(stableId)
    const changeConfirmation = useChangeConfirmation(
        "master-data-product-change",
    )
    const priceUndoToastIdRef = React.useRef<string | null>(null)
    const dismissPriceUndoToast = React.useCallback(() => {
        if (!priceUndoToastIdRef.current) return
        toast.close(priceUndoToastIdRef.current)
        priceUndoToastIdRef.current = null
    }, [])
    const offerPriceUndo = React.useCallback(
        (undo: () => void) => {
            dismissPriceUndoToast()
            const toastId = toast.add({
                title: "批量价格已应用",
                description: "可撤销恢复批量改价前的参考售价。",
                type: "info",
                timeout: 8000,
                actionProps: {
                    children: "撤销",
                    onClick: () => {
                        undo()
                        toast.close(toastId)
                        if (priceUndoToastIdRef.current === toastId) {
                            priceUndoToastIdRef.current = null
                        }
                    },
                },
            })
            priceUndoToastIdRef.current = toastId
        },
        [dismissPriceUndoToast],
    )
    React.useEffect(() => {
        dismissPriceUndoToast()
    }, [dismissPriceUndoToast, stableId, editor.saveOpen])
    React.useEffect(() => dismissPriceUndoToast, [dismissPriceUndoToast])

    const [resetOpen, setResetOpen] = React.useState(false)
    const [sourceProcessing, setSourceProcessing] = React.useState(false)
    const [selectedSupplySkuId, setSelectedSupplySkuId] = React.useState<
        string | undefined
    >(undefined)

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

    const [values, isSubmitting] = useSelector(
        form.store,
        (state) => [state.values, state.isSubmitting] as const,
    )
    const saving = pending || isSubmitting || sourceProcessing
    const changes = React.useMemo(
        () => productChangeSummary(initialFormValues, values),
        [initialFormValues, values],
    )
    const hasUnsavedChanges = changes.length > 0
    const saveDockOpen = !isCreate && canRevise && hasUnsavedChanges

    useWorkspaceDirtyState({ dirty: hasUnsavedChanges, busy: saving })

    const permissions = accountQuery.isSuccess
        ? accountQuery.data.permissions
        : undefined
    const canReadSupply = hasPermission(permissions, "supplier_offering:list")
    const canViewCosts = hasPermission(
        permissions,
        "supplier_offering_cost:detail",
    )
    const canReadInventory = hasPermission(permissions, "stock_balance:list")

    const savedSkuIds = React.useMemo(
        () =>
            !isCreate && canReadSupply
                ? (data?.productDetail?.skus
                      .map((sku) => sku.skuId)
                      .filter((id): id is string => Boolean(id)) ?? [])
                : [],
        [canReadSupply, data?.productDetail?.skus, isCreate],
    )
    const offeringsQuery = useSupplierOfferingsForSkusQuery(savedSkuIds)

    const { supplyBySku, totalActiveOfferings, missingSupplyCount } =
        React.useMemo(() => {
            const bySku = new Map<
                string,
                { suppliers: Set<string>; lowestDropship?: string }
            >()
            let activeCount = 0
            for (const offering of offeringsQuery.data ?? []) {
                if (!isActiveOffering(offering)) continue
                activeCount += 1
                const current = bySku.get(offering.sku_id) ?? {
                    suppliers: new Set<string>(),
                }
                current.suppliers.add(offering.supplier_id)
                const price = offering.dropship_supply_price_gross
                if (price) {
                    try {
                        if (
                            !current.lowestDropship ||
                            compareDecimal(price, current.lowestDropship, 4) < 0
                        ) {
                            current.lowestDropship = price
                        }
                    } catch {
                        // 忽略不可解析价格字符串，不影响展示
                    }
                }
                bySku.set(offering.sku_id, current)
            }
            const summary = new Map<string, SkuSupplySummary>()
            let missing = 0
            for (const skuId of savedSkuIds) {
                const entry = bySku.get(skuId)
                const activeSuppliers = entry?.suppliers.size ?? 0
                if (activeSuppliers === 0) missing += 1
                summary.set(skuId, {
                    activeSuppliers,
                    lowestDropshipPrice: entry?.lowestDropship,
                })
            }
            return {
                supplyBySku: summary,
                totalActiveOfferings: activeCount,
                missingSupplyCount: offeringsQuery.isSuccess
                    ? missing
                    : undefined,
            }
        }, [offeringsQuery.data, offeringsQuery.isSuccess, savedSkuIds])

    React.useEffect(() => {
        if (activeSection !== "sku") return
        const frame = window.requestAnimationFrame(() => {
            document
                .getElementById("product-section-sku")
                ?.scrollIntoView({ block: "start" })
        })
        return () => window.cancelAnimationFrame(frame)
    }, [activeSection, data?.stableId])

    // 新建态支持 Cmd/Ctrl+S；编辑态由 DetailWorkspaceSaveDock 统一管理快捷保存。
    React.useEffect(() => {
        if (!isCreate || !canRevise) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.repeat ||
                event.altKey ||
                event.shiftKey ||
                !(event.metaKey || event.ctrlKey) ||
                event.key.toLowerCase() !== "s" ||
                (event.target instanceof Element &&
                    event.target.closest(
                        '[role="dialog"], [role="alertdialog"]',
                    ))
            ) {
                return
            }
            event.preventDefault()
            if (!saving && !saveOpen && !discardOpen && !resetOpen) {
                requestSave(form.state.values)
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [
        canRevise,
        discardOpen,
        form,
        isCreate,
        requestSave,
        resetOpen,
        saveOpen,
        saving,
    ])

    const openSupplyForSku = React.useCallback(
        (skuId: string) => {
            setSelectedSupplySkuId(skuId)
            setActiveSection("supply")
        },
        [setActiveSection],
    )

    const bindings = createProductFormBindings(
        form,
        values,
        isCreate,
        data?.name,
        changeConfirmation.confirm,
        offerPriceUndo,
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

    // Retain old deep links to the effective section, now shown as the save dialog.
    const showSave = canRevise && (saveOpen || activeSection === "effective")
    const section =
        activeSection === "effective" ||
        activeSection === "sku" ||
        (activeSection === "supply" && !canReadSupply)
            ? "basic"
            : activeSection
    const changeDescription =
        changes.length > 0
            ? changes.slice(0, 3).join("、") + (changes.length > 3 ? " 等" : "")
            : undefined
    const totalMediaCount =
        fields.carouselImages.length + fields.detailImages.length

    const closeSave = (open: boolean) => {
        setSaveOpen(open)
        if (!open && activeSection === "effective") setActiveSection("basic")
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
    const loadWarnings = data?.loadWarnings ?? []

    return (
        <ProductDetailEntryGate
            isCreate={isCreate}
            hasDetailData={Boolean(data)}
            detailQuery={detailQuery}
            accountQuery={accountQuery}
            canCreate={canCreate}
            listHref={listHref}
        >
            <DetailWorkspacePage>
                <form
                    id="product-detail-form"
                    className="flex min-w-0 flex-1 flex-col"
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (
                            saving ||
                            showSave ||
                            (!isCreate && !hasUnsavedChanges)
                        ) {
                            return
                        }
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
                        hasUnsavedChanges={hasUnsavedChanges}
                        onBack={() => navigateAway(listHref)}
                        onMedia={() => setActiveSection("media")}
                        onSave={() => requestSave(values)}
                        tabs={
                            <ProductSectionTabs
                                value={section}
                                isCreate={isCreate}
                                canReadSupply={canReadSupply}
                                supplyCount={
                                    offeringsQuery.isSuccess
                                        ? totalActiveOfferings
                                        : undefined
                                }
                                missingSupplyCount={missingSupplyCount}
                                mediaCount={totalMediaCount}
                                onValueChange={setActiveSection}
                            />
                        }
                    />

                    <DetailWorkspaceBody
                        saveDockOpen={saveDockOpen}
                        alerts={
                            <>
                                {loadWarnings.map((message) => (
                                    <BusinessFailureState
                                        key={message}
                                        kind="business"
                                        title="部分资料读取失败"
                                        description={message}
                                    />
                                ))}
                                {!showSave ? feedback : null}
                                <ProductSourceDiff
                                    key={stableId}
                                    productId={stableId}
                                    enabled={!isCreate && Boolean(data)}
                                    canResolve={hasUpdatePermission}
                                    canRevise={canRevise}
                                    dirty={hasUnsavedChanges}
                                    busy={saving}
                                    openRequested={sourceDiffRequested}
                                    onProcessingChange={setSourceProcessing}
                                />
                            </>
                        }
                    >
                        {section === "basic" ? (
                            <>
                                <ProductBasicSection
                                    canSelectUnit={canSelectUnit}
                                    canSelectCategory={canSelectCategory}
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
                                    canRevise={canRevise && !sourceProcessing}
                                    disabled={saving}
                                    name={name}
                                    setName={setName}
                                    fields={fields}
                                    setFields={setFields}
                                    unitOptions={unitOptionsQuery.data}
                                    unitLoading={unitOptionsQuery.isFetching}
                                    categoryOptions={categoryOptions}
                                    brandOptions={brandOptions}
                                    categoryLoading={
                                        categoryListQuery.isFetching
                                    }
                                    brandLoading={brandListQuery.isFetching}
                                    rememberPendingFiles={rememberPendingFiles}
                                    onOpenMediaTab={() =>
                                        setActiveSection("media")
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
                                    canRevise={canRevise && !sourceProcessing}
                                    name={name}
                                    fields={fields}
                                    specDrafts={specDrafts}
                                    activeSpecs={activeSpecs}
                                    inventoryPreviewSkus={inventoryPreviewSkus}
                                    syncSpecDrafts={syncSpecDrafts}
                                    applySpecDrafts={applySpecDrafts}
                                    resetSpecDrafts={resetSpecDrafts}
                                    updateSku={updateSku}
                                    onApplyBatchReferencePrices={
                                        applyBatchReferencePrices
                                    }
                                    inventoryActionHint={inventoryActionHint}
                                    showSupplyColumn={
                                        !isCreate && canReadSupply
                                    }
                                    supplyBySku={supplyBySku}
                                    supplyLoading={offeringsQuery.isPending}
                                    supplyError={offeringsQuery.isError}
                                    canViewCosts={canViewCosts}
                                    onOpenSupply={openSupplyForSku}
                                    onOpenInventory={
                                        canReadInventory
                                            ? openInventoryPreview
                                            : undefined
                                    }
                                    rememberSkuFile={rememberSkuFile}
                                />
                            </>
                        ) : null}

                        {section === "supply" && !isCreate && canReadSupply ? (
                            <DetailWorkspaceSection width="full">
                                <ProductSupplySection
                                    fields={fields}
                                    name={name}
                                    selectedSkuId={selectedSupplySkuId}
                                    onSelectSkuId={setSelectedSupplySkuId}
                                    onRegisterSupply={setSupplierDialogSku}
                                />
                            </DetailWorkspaceSection>
                        ) : null}

                        {section === "media" ? (
                            <ProductMediaSection
                                idPrefix="master-data-product-detail-media"
                                productName={title}
                                disabled={pending || sourceProcessing}
                                canRevise={canRevise}
                                fields={fields}
                                setFields={setFields}
                                rememberPendingFiles={rememberPendingFiles}
                            />
                        ) : null}

                        {section === "history" && !isCreate ? (
                            <ProductHistorySection data={data} />
                        ) : null}
                    </DetailWorkspaceBody>

                    {!isCreate ? (
                        <PermissionGuard permission="product:update">
                            <DetailWorkspaceSaveDock
                                open={saveDockOpen}
                                changeCount={changes.length}
                                summary={changes.join("；")}
                                description={changeDescription}
                                saving={saving}
                                enableSaveShortcut={
                                    !showSave &&
                                    !disableOpen &&
                                    !discardOpen &&
                                    !resetOpen
                                }
                                onSave={() => requestSave(values)}
                                actions={
                                    <>
                                        <Button
                                            id="master-data-product-detail-reset"
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                            disabled={saving}
                                            onClick={() => setResetOpen(true)}
                                        >
                                            取消修改
                                        </Button>
                                        <LoadingButton
                                            id="master-data-product-detail-header-submit"
                                            loading={saving}
                                            type="button"
                                            size="sm"
                                            className="px-3.5"
                                            aria-keyshortcuts="Meta+S Control+S"
                                            disabled={!canRevise || saving}
                                            onClick={() => requestSave(values)}
                                        >
                                            <SaveIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            {saving ? "提交中…" : "保存更新"}
                                        </LoadingButton>
                                    </>
                                }
                            />
                        </PermissionGuard>
                    ) : null}
                </form>

                {changeConfirmation.dialog}
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
                    idPrefix="master-data-product-reset-confirm"
                    open={canRevise && resetOpen}
                    onOpenChange={setResetOpen}
                    title="取消本次修改？"
                    description="商品资料将恢复到本次编辑前的内容。"
                    confirmLabel="取消修改"
                    cancelLabel="继续编辑"
                    onConfirm={() => {
                        form.reset(initialFormValues)
                        dismissPriceUndoToast()
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
            </DetailWorkspacePage>
        </ProductDetailEntryGate>
    )
}
