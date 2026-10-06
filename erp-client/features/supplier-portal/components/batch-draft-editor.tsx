"use client"

import { useState } from "react"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { usePortalApplication } from "../hooks/queries"
import type { PortalApplication } from "../types"
import { PortalNewProductEditor } from "../pages/new-product-page"
import { PortalError } from "./surface"

/** 在原批次内补齐原草稿，保存后返回批次统一提交。 */
export function PortalBatchDraftEditor({
    id,
    onSaved,
    onClose,
}: {
    id: string
    onSaved: (application: PortalApplication) => void
    onClose: () => void
}) {
    const query = usePortalApplication(id)
    const [unresolved, setUnresolved] = useState(false)
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !unresolved) onClose()
            }}
        >
            <DialogContent
                id="supplier-portal-batch-draft-dialog"
                showCloseButton={!unresolved}
                className="flex max-h-[90vh] w-[min(96vw,72rem)] flex-col overflow-hidden"
                closeButtonId="supplier-portal-batch-draft-close"
            >
                <DialogHeader>
                    <DialogTitle>补充本批新品资料</DialogTitle>
                </DialogHeader>
                <div className="min-h-0 flex-1 overflow-y-auto">
                    <PortalError
                        error={query.error}
                        retry={() => void query.refetch()}
                        id="supplier-portal-batch-draft-reload"
                    />
                    {query.isPending && <p>正在读取原草稿…</p>}
                    {query.data &&
                        ["draft", "returned", "withdrawn"].includes(
                            query.data.status,
                        ) && (
                            <PortalNewProductEditor
                                key={query.data.id}
                                draft={query.data}
                                idPrefix={`supplier-portal-batch-draft-${toAutomationIdSegment(query.data.id)}`}
                                onSaved={onSaved}
                                onUnresolvedChange={setUnresolved}
                            />
                        )}
                    {query.data &&
                        !["draft", "returned", "withdrawn"].includes(
                            query.data.status,
                        ) && (
                            <p>
                                原申请已进入采购确认。返回批次读取当前结果后继续。
                            </p>
                        )}
                </div>
                {unresolved && (
                    <p className="text-sm text-muted-foreground">
                        本次上传或保存结果尚未确认，请在原草稿恢复操作后再返回批次。
                    </p>
                )}
                <Button
                    id="supplier-portal-batch-draft-back"
                    type="button"
                    variant="outline"
                    onClick={onClose}
                    disabled={unresolved}
                >
                    返回本批次
                </Button>
            </DialogContent>
        </Dialog>
    )
}
