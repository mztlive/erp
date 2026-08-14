"use client"

import { ArrowLeftIcon } from "lucide-react"

import { PageHeader, PageScaffold, surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { SupplierEditorBasicSection } from "@/features/master-data/components/supplier/supplier-editor-basic-section"
import { SupplierEditorCommercialSection } from "@/features/master-data/components/supplier/supplier-editor-commercial-section"
import { SupplierEditorContractSection } from "@/features/master-data/components/supplier/supplier-editor-contract-section"
import { SupplierEditorDialogs } from "@/features/master-data/components/supplier/supplier-editor-dialogs"
import { SupplierEditorDocumentHeader } from "@/features/master-data/components/supplier/supplier-editor-document-header"
import { SupplierEditorHistorySection } from "@/features/master-data/components/supplier/supplier-editor-history-section"
import { SupplierEditorInvoiceSection } from "@/features/master-data/components/supplier/supplier-editor-invoice-section"
import {
    SupplierSectionTabs,
    SupplierSummaryStrip,
} from "@/features/master-data/components/supplier/supplier-editor-navigation"
import { SupplierEditorStatusPanel } from "@/features/master-data/components/supplier/supplier-editor-status-panel"
import type { SupplierEditor } from "@/features/master-data/hooks/use-supplier-editor"
import { useSupplierSaveFlow } from "@/features/master-data/hooks/use-supplier-save-flow"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { cn } from "@/lib/utils"

export function SupplierEditorForm({ editor }: { editor: SupplierEditor }) {
    const {
        isCreate,
        router,
        data,
        formError,
        result,
        disableOpen,
        setDisableOpen,
        discardOpen,
        setDiscardOpen,
        saveReasonOpen,
        setSaveReasonOpen,
        reasonDraft,
        setReasonDraft,
        reasonError,
        setReasonError,
        pendingNav,
        setPendingNav,
        activeSection,
        setActiveSection,
        errorRef,
        editedSensitiveRef,
        rememberMediaFiles,
        mediaUrlsFor,
        mediaAssetIdsFor,
        navigateAway,
        listHref,
        pending,
        canCreate,
        canRevise,
        canDisable,
        canRevealSensitive,
        reviseBlocker,
        disableBlocker,
    } = editor
    const canEdit = isCreate ? canCreate : canRevise

    const formId = "supplier-detail-form"

    const {
        values,
        setFieldValue,
        requestSave,
        confirmSaveWithReason,
        phoneSensitive,
        addressSensitive,
        bankSensitive,
        refreshSensitiveToken,
        summaryRows,
    } = useSupplierSaveFlow(editor)

    const title = isCreate
        ? masterDataCopy.supplierCreateTitle
        : values.name || data?.name || "供应商详情"

    return (
        <PageScaffold density="compact">
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    {
                        id: "master-data",
                        label: "基础资料",
                        href: "/master-data",
                    },
                    {
                        id: "suppliers",
                        label: "供应商",
                        href: listHref,
                    },
                    {
                        id: "detail",
                        label: isCreate
                            ? "新建供应商"
                            : data?.stableNo || title,
                        current: true,
                    },
                ]}
                actions={
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => navigateAway(listHref)}
                    >
                        <ArrowLeftIcon data-icon="inline-start" aria-hidden />
                        返回列表
                    </Button>
                }
            />

            <form id={formId} className="space-y-4" onSubmit={requestSave}>
                <SupplierEditorDocumentHeader
                    title={title}
                    isCreate={isCreate}
                    data={data}
                    values={values}
                    canEdit={canEdit}
                    pending={pending}
                    canDisable={canDisable}
                    disableBlocker={disableBlocker}
                    onDisable={() => setDisableOpen(true)}
                />

                <div className="space-y-3">
                    <SupplierEditorStatusPanel
                        isCreate={isCreate}
                        canRevise={canRevise}
                        reviseBlocker={reviseBlocker}
                        result={result}
                        formError={formError}
                        errorRef={errorRef}
                    />

                    <SupplierSummaryStrip rows={summaryRows} />

                    <div
                        className={cn(
                            surfacePanelClassName,
                            "overflow-hidden",
                        )}
                    >
                        <SupplierSectionTabs
                            value={activeSection}
                            isCreate={isCreate}
                            onValueChange={setActiveSection}
                        />

                        <div className="p-4 md:p-5">
                            {activeSection === "basic" && (
                                <SupplierEditorBasicSection
                                    values={values}
                                    setFieldValue={setFieldValue}
                                    canEdit={canEdit}
                                    phoneSensitive={phoneSensitive}
                                    addressSensitive={addressSensitive}
                                    canRevealSensitive={canRevealSensitive}
                                    refreshSensitiveToken={
                                        refreshSensitiveToken
                                    }
                                    editedSensitiveRef={editedSensitiveRef}
                                />
                            )}

                            {activeSection === "commercial" && (
                                <SupplierEditorCommercialSection
                                    values={values}
                                    setFieldValue={setFieldValue}
                                    canEdit={canEdit}
                                />
                            )}

                            {activeSection === "contract" && (
                                <SupplierEditorContractSection
                                    values={values}
                                    setFieldValue={setFieldValue}
                                    canEdit={canEdit}
                                    mediaUrlsFor={mediaUrlsFor}
                                    mediaAssetIdsFor={mediaAssetIdsFor}
                                    rememberMediaFiles={rememberMediaFiles}
                                />
                            )}

                            {activeSection === "invoice" && (
                                <SupplierEditorInvoiceSection
                                    values={values}
                                    setFieldValue={setFieldValue}
                                    canEdit={canEdit}
                                    bankSensitive={bankSensitive}
                                    canRevealSensitive={canRevealSensitive}
                                    refreshSensitiveToken={
                                        refreshSensitiveToken
                                    }
                                    editedSensitiveRef={editedSensitiveRef}
                                />
                            )}

                            {activeSection === "history" &&
                                !isCreate &&
                                data && (
                                    <SupplierEditorHistorySection
                                        data={data}
                                    />
                                )}
                        </div>
                    </div>
                </div>
            </form>

            <SupplierEditorDialogs
                isCreate={isCreate}
                data={data}
                disableOpen={disableOpen}
                setDisableOpen={setDisableOpen}
                saveReasonOpen={saveReasonOpen}
                setSaveReasonOpen={setSaveReasonOpen}
                reasonDraft={reasonDraft}
                setReasonDraft={setReasonDraft}
                reasonError={reasonError}
                setReasonError={setReasonError}
                pending={pending}
                onConfirm={confirmSaveWithReason}
                discardOpen={discardOpen}
                setDiscardOpen={setDiscardOpen}
                pendingNav={pendingNav}
                setPendingNav={setPendingNav}
                router={router}
            />
        </PageScaffold>
    )
}
