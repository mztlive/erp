"use client"

import * as React from "react"
import { useRouter } from "@/lib/navigation"
import { useMutation } from "@tanstack/react-query"
import { UploadIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { inquiryPath, previewInquiryCreation } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import {
    InquiryImportDialog,
    type InquiryImportConfirmResult,
} from "./inquiry-import-dialog"

export function InquiryCreateImport({ userId }: { userId: string }) {
    const router = useRouter()
    const [open, setOpen] = React.useState(false)
    const command = useInquiryCommand(userId, "import-new")
    const preview = useMutation({
        mutationFn: ({
            file,
            customerId,
        }: {
            file: File
            customerId: string
        }) => previewInquiryCreation(file, customerId),
        meta: { suppressErrorToast: true },
    })
    React.useEffect(() => {
        const result = command.resolution?.result as
            | InquiryImportConfirmResult
            | undefined
        if (command.resolution?.replayed && result?.inquiry_id)
            router.push(
                `/sales/inquiries/${encodeURIComponent(result.inquiry_id)}`,
            )
    }, [command.resolution, router])
    return (
        <>
            <Button
                id="sales-inquiries-import-create"
                variant="outline"
                size="sm"
                onClick={() => setOpen(true)}
            >
                <UploadIcon aria-hidden="true" data-icon="inline-start" />
                导入客户选品
            </Button>
            <InquiryImportDialog
                creation
                open={open}
                onOpenChange={setOpen}
                busy={preview.isPending || command.isPending}
                disabled={!command.ready || command.unresolved}
                error={command.error ?? preview.error}
                onPreview={(file, customerId) =>
                    preview.mutateAsync({ file, customerId })
                }
                onConfirm={async (result, confirmMerge, customerId) => {
                    const created =
                        await command.run<InquiryImportConfirmResult>(
                            `${inquiryPath()}/imports/create`,
                            {
                                customer_id: customerId,
                                preview_id: result.preview_id,
                                input_fingerprint: result.input_fingerprint,
                                confirm_merge_duplicates: confirmMerge,
                            },
                        )
                    router.push(
                        `/sales/inquiries/${encodeURIComponent(created.inquiry_id)}`,
                    )
                }}
                recovery={
                    command.unresolved ? (
                        <Alert variant="warning">
                            <AlertDescription>
                                上次创建结果尚待核对，请先确认，避免重复创建。
                                <Button
                                    id="sales-inquiries-import-recover"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={command.isPending}
                                    onClick={() =>
                                        void command
                                            .recover()
                                            .catch(() => undefined)
                                    }
                                >
                                    核对上次创建结果
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ) : undefined
                }
            />
        </>
    )
}
