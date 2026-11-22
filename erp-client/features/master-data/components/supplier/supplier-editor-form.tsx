"use client"

import { SupplierDeliveryTemplatesSection } from "@/features/supplier-offerings/components/delivery-templates/supplier-delivery-templates-section"
import { SupplierAccountsSection } from "@/features/master-data/components/supplier/supplier-accounts-section"
import { SupplierCatalogSection } from "@/features/supplier-portal/components/admin-catalog-section"
import { SupplierApplicationsSection } from "@/features/supplier-portal/components/admin-applications-section"
import * as React from "react"
import { SaveIcon } from "lucide-react"
import {
    DetailWorkspaceBody,
    DetailWorkspacePage,
    DetailWorkspaceSaveDock,
    DetailWorkspaceSection,
    DiscardConfirmDialog,
    detailWorkspaceStyles,
} from "@/components/business"
import { clearApiFormError } from "@/components/form/api-errors"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
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
import type { SupplierFieldKey } from "@/features/master-data/lib/supplier-editor-model"

export function SupplierEditorForm({
    idPrefix,
    editor,
}: {
    idPrefix?: string
    editor: SupplierEditor
}) {
    const prefix = idPrefix ?? "master-data-supplier-detail-form"
    const {
        isCreate,
        router,
        data,
        form,
        initialFormValues,
        pendingChangeReasonRef,
        formError,
        setFormError,
        formErrorTitle,
        result,
        setResult,
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
        canReadDeliveryTemplates,
        canEditDeliveryTemplates,
        canReadPortalAccounts,
        canReadPortalCatalog,
        canReadPortalApplications,
        reviseBlocker,
        disableBlocker,
    } = editor
    const canEdit = isCreate ? canCreate : canRevise

    const formId = `${prefix}-form`

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

    const [resetOpen, setResetOpen] = React.useState(false)
    const formRef = React.useRef<HTMLFormElement>(null)
    const count = (Object.keys(values) as SupplierFieldKey[]).filter(
        (key) =>
            key !== "changeReason" &&
            key !== "maintainerUserId" &&
            key !== "capabilityOwnerUserId" &&
            (values[key] !== initialFormValues[key] ||
                (key === "address" &&
                    Boolean(addressSensitive) &&
                    editedSensitiveRef.current.has(key) &&
                    !values.address.trim())),
    ).length
    const saveDockOpen = !isCreate && canRevise && count > 0

    React.useEffect(() => {
        if (!saveDockOpen) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.altKey ||
                event.shiftKey ||
                !(event.metaKey || event.ctrlKey) ||
                event.key.toLowerCase() !== "s" ||
                (event.target instanceof Element &&
                    event.target.closest(
                        '[role="dialog"], [role="alertdialog"]',
                    ))
            )
                return

            event.preventDefault()
            if (
                !event.repeat &&
                !pending &&
                !form.state.isSubmitting &&
                !saveReasonOpen &&
                !disableOpen &&
                !discardOpen &&
                !resetOpen
            ) {
                formRef.current?.requestSubmit()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [
        disableOpen,
        discardOpen,
        form,
        pending,
        resetOpen,
        saveDockOpen,
        saveReasonOpen,
    ])

    return (
        <DetailWorkspacePage>
            <SupplierEditorDocumentHeader
                formId={formId}
                dirty={saveDockOpen}
                back={{
                    id: `${prefix}-back-list`,
                    label: "供应商列表",
                    onClick: () => navigateAway(listHref),
                }}
                title={title}
                isCreate={isCreate}
                data={data}
                values={values}
                canEdit={canEdit}
                pending={pending}
                canDisable={canDisable}
                disableBlocker={disableBlocker}
                onDisable={() => setDisableOpen(true)}
                tabs={
                    <SupplierSectionTabs
                        value={activeSection}
                        canReadDeliveryTemplates={canReadDeliveryTemplates}
                        canReadPortalAccounts={canReadPortalAccounts}
                        canReadPortalCatalog={canReadPortalCatalog}
                        canReadPortalApplications={canReadPortalApplications}
                        isCreate={isCreate}
                        onValueChange={setActiveSection}
                    />
                }
            />

            <DetailWorkspaceBody
                saveDockOpen={saveDockOpen}
                summary={<SupplierSummaryStrip rows={summaryRows} />}
                alerts={
                    <SupplierEditorStatusPanel
                        isCreate={isCreate}
                        canRevise={canRevise}
                        reviseBlocker={reviseBlocker}
                        result={result}
                        formError={formError}
                        formErrorTitle={formErrorTitle}
                        errorRef={errorRef}
                    />
                }
            >
                <form
                    id={formId}
                    ref={formRef}
                    onSubmit={(event) => {
                        if (
                            !canEdit ||
                            pending ||
                            form.state.isSubmitting ||
                            saveReasonOpen ||
                            (!isCreate && !saveDockOpen)
                        ) {
                            event.preventDefault()
                            return
                        }
                        requestSave(event)
                    }}
                >
                    {activeSection === "basic" && (
                        <SupplierEditorBasicSection
                            values={values}
                            setFieldValue={setFieldValue}
                            canEdit={canEdit}
                            phoneSensitive={phoneSensitive}
                            addressSensitive={addressSensitive}
                            canRevealSensitive={canRevealSensitive}
                            refreshSensitiveToken={refreshSensitiveToken}
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
                            refreshSensitiveToken={refreshSensitiveToken}
                            editedSensitiveRef={editedSensitiveRef}
                        />
                    )}

                    {activeSection === "history" && !isCreate && data && (
                        <SupplierEditorHistorySection data={data} />
                    )}
                </form>
                {activeSection === "delivery" &&
                !isCreate &&
                data &&
                canReadDeliveryTemplates ? (
                    <DetailWorkspaceSection width="full">
                        <SupplierDeliveryTemplatesSection
                            key={data.stableId}
                            supplierId={data.stableId}
                            canEdit={canEditDeliveryTemplates}
                        />
                    </DetailWorkspaceSection>
                ) : null}
                {activeSection === "portal-catalog" &&
                !isCreate &&
                data &&
                canReadPortalCatalog ? (
                    <DetailWorkspaceSection width="full">
                        <div
                            data-slot="detail-inline-table"
                            className="[&_[data-slot=list-work-surface]]:rounded-none [&_[data-slot=list-work-surface]]:border-x-0"
                        >
                            <SupplierCatalogSection
                                key={data.stableId}
                                supplierId={data.stableId}
                            />
                        </div>
                    </DetailWorkspaceSection>
                ) : null}
                {activeSection === "portal-applications" &&
                !isCreate &&
                data &&
                canReadPortalApplications ? (
                    <DetailWorkspaceSection width="full">
                        <div
                            data-slot="detail-inline-table"
                            className="[&_[data-slot=list-work-surface]]:rounded-none [&_[data-slot=list-work-surface]]:border-x-0"
                        >
                            <SupplierApplicationsSection
                                key={data.stableId}
                                supplierId={data.stableId}
                            />
                        </div>
                    </DetailWorkspaceSection>
                ) : null}
                {activeSection === "portal-accounts" &&
                !isCreate &&
                data &&
                canReadPortalAccounts ? (
                    <SupplierAccountsSection
                        key={data.stableId}
                        supplierId={data.stableId}
                        supplierEnabled={data.lifecycleStatus === "ENABLED"}
                    />
                ) : null}
            </DetailWorkspaceBody>

            {!isCreate && canRevise ? (
                <>
                    <DetailWorkspaceSaveDock
                        open={saveDockOpen}
                        changeCount={count}
                        actions={
                            <>
                                <Button
                                    id={`${prefix}-reset`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    disabled={pending}
                                    onClick={() => setResetOpen(true)}
                                >
                                    取消修改
                                </Button>
                                <LoadingButton
                                    id="master-data-supplier-document-header-submit"
                                    type="submit"
                                    form={formId}
                                    size="sm"
                                    className="px-3.5"
                                    aria-keyshortcuts="Meta+S Control+S"
                                    loading={pending}
                                    disabled={!saveDockOpen || pending}
                                >
                                    <SaveIcon
                                        data-icon="inline-start"
                                        aria-hidden
                                    />
                                    {pending
                                        ? "提交中…"
                                        : masterDataCopy.reviseSubmit}
                                </LoadingButton>
                            </>
                        }
                    />
                    <DiscardConfirmDialog
                        id={`${prefix}-reset-confirm`}
                        open={resetOpen}
                        onOpenChange={setResetOpen}
                        title="取消本次修改？"
                        description="供应商资料将恢复到本次编辑前的内容。"
                        confirmLabel="取消修改"
                        cancelLabel="继续编辑"
                        onConfirm={() => {
                            form.reset({
                                ...initialFormValues,
                                capabilityOwnerUserId:
                                    values.capabilityOwnerUserId,
                            })
                            editedSensitiveRef.current.clear()
                            pendingChangeReasonRef.current = null
                            clearApiFormError(form)
                            setFormError(null)
                            setResult(null)
                            setResetOpen(false)
                        }}
                    />
                </>
            ) : null}

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
        </DetailWorkspacePage>
    )
}
