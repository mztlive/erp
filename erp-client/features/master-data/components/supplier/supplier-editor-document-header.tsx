"use client"

import { BanIcon, SaveIcon } from "lucide-react"

import { DocumentHeader } from "@/components/business"
import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { SupplierEditorFormValues } from "@/features/master-data/lib/supplier-editor-model"
import type { MasterDataCenterView } from "@/features/master-data/types"

export function SupplierEditorDocumentHeader({
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
        <DocumentHeader
            density="compact"
            title={title}
            documentNumber={
                isCreate ? "待生成" : data?.stableNo || "—"
            }
            primaryStatus={
                !isCreate && data
                    ? {
                          label: data.lifecycleStatusLabel,
                          tone: data.lifecycleTone,
                      }
                    : {
                          label: "待创建",
                          tone: "neutral",
                      }
            }
            version={
                !isCreate && data ? data.currentRevision.revisionNo : undefined
            }
            meta={
                <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                    <span>
                        企业主体{" "}
                        <span className="font-medium text-foreground">
                            {values.company.trim() || "待填写"}
                        </span>
                    </span>
                    <span className="text-border" aria-hidden="true">
                        ·
                    </span>
                    <span>
                        联系人{" "}
                        <span className="font-medium text-foreground">
                            {values.contactName.trim() || "待填写"}
                        </span>
                    </span>
                </span>
            }
            secondaryActions={
                !isCreate && data ? (
                    <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!canDisable}
                        title={disableBlocker?.message}
                        onClick={onDisable}
                    >
                        <BanIcon data-icon="inline-start" aria-hidden />
                        {masterDataCopy.actionDisable}
                    </Button>
                ) : null
            }
            primaryAction={
                <Button type="submit" size="sm" disabled={!canEdit || pending}>
                    <SaveIcon data-icon="inline-start" aria-hidden />
                    {pending
                        ? "提交中…"
                        : isCreate
                          ? masterDataCopy.createSubmit
                          : masterDataCopy.reviseSubmit}
                </Button>
            }
        />
    )
}
