"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import { BanIcon, SaveIcon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    detailWorkspaceStyles,
    type DetailWorkspaceChromeProps,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { SupplierEditorFormValues } from "@/features/master-data/lib/supplier-editor-model"
import type { MasterDataCenterView } from "@/features/master-data/types"

export function SupplierEditorDocumentHeader({
    back,
    formId,
    dirty,
    tabs,
    title,
    isCreate,
    data,
    values,
    canEdit,
    pending,
    canDisable,
    disableBlocker,
    onDisable,
}: {
    formId: string
    dirty: boolean
    tabs: React.ReactNode
    back: DetailWorkspaceChromeProps["back"]
    title: string
    isCreate: boolean
    data: MasterDataCenterView | null | undefined
    values: SupplierEditorFormValues
    canEdit: boolean
    pending: boolean
    canDisable: boolean
    disableBlocker: { message: string } | undefined
    onDisable: () => void
}) {
    return (
        <DetailWorkspaceChrome
            back={back}
            numberLabel="供应商编号"
            title={title}
            documentNumber={isCreate ? "待生成" : data?.stableNo || "—"}
            primaryStatus={
                !isCreate && data
                    ? {
                          label: data.lifecycleStatusLabel,
                          tone: data.lifecycleTone,
                      }
                    : {
                          label: "待创建",
                          tone: "info",
                      }
            }
            version={
                !isCreate && data ? data.currentRevision.revisionNo : undefined
            }
            titleExtra={dirty ? <DetailWorkspaceDirtyBadge /> : null}
            tabs={tabs}
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="company">
                            企业主体{" "}
                            <span className="font-medium text-foreground">
                                {values.company.trim() || "待填写"}
                            </span>
                        </span>,
                        <span key="contact">
                            联系人{" "}
                            <span className="font-medium text-foreground">
                                {values.contactName.trim() || "待填写"}
                            </span>
                        </span>,
                    ]}
                />
            }
            secondaryActions={
                !isCreate && data ? (
                    <PermissionGuard permission={"supplier:delete"}>
                        <Button
                            id="master-data-supplier-document-header-disable"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietDangerButton}
                            disabled={!canDisable || pending}
                            title={disableBlocker?.message}
                            onClick={onDisable}
                        >
                            <BanIcon data-icon="inline-start" aria-hidden />
                            {masterDataCopy.actionDisable}
                        </Button>
                    </PermissionGuard>
                ) : null
            }
            primaryAction={
                isCreate ? (
                    <PermissionGuard permission="supplier:create">
                        <LoadingButton
                            id="master-data-supplier-document-header-submit"
                            type="submit"
                            form={formId}
                            size="sm"
                            className="px-3.5"
                            loading={pending}
                            disabled={!canEdit || pending}
                        >
                            <SaveIcon data-icon="inline-start" aria-hidden />
                            {pending ? "提交中…" : "创建供应商"}
                        </LoadingButton>
                    </PermissionGuard>
                ) : null
            }
        />
    )
}
