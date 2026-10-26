"use client"

import * as React from "react"
import { EyeIcon, FileTextIcon, Trash2Icon } from "lucide-react"
import {
    Attachment,
    AttachmentAction,
    AttachmentActions,
    AttachmentContent,
    AttachmentDescription,
    AttachmentGroup,
    AttachmentMedia,
    AttachmentTitle,
} from "@/components/ui/attachment"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatFileSize, selectedFileKey } from "@/lib/file-selection"
import { fileUploadText } from "@/lib/ui-text"

function SelectedFile({
    file,
    baseId,
    disabled,
    description,
    onRemove,
}: {
    file: File
    baseId?: string
    disabled: boolean
    description: string
    onRemove?: (file: File) => void
}) {
    const [open, setOpen] = React.useState(false)
    const [url, setUrl] = React.useState<string>()
    const isImage = ["image/jpeg", "image/png", "image/webp"].includes(
        file.type,
    )
    const isPdf = file.type === "application/pdf"
    React.useEffect(() => {
        if (!isImage && !isPdf) return
        const next = URL.createObjectURL(file)
        setUrl(next)
        return () => URL.revokeObjectURL(next)
    }, [file, isImage, isPdf])

    return (
        <>
            <Attachment className="w-full" state="idle" aria-label={file.name}>
                <AttachmentMedia variant={isImage ? "image" : "icon"}>
                    {isImage && url ? (
                        // eslint-disable-next-line @next/next/no-img-element -- 待提交文件通过本地 Blob 预览。
                        <img src={url} alt="" />
                    ) : (
                        <FileTextIcon aria-hidden="true" />
                    )}
                </AttachmentMedia>
                <AttachmentContent>
                    <AttachmentTitle title={file.name}>
                        {file.name}
                    </AttachmentTitle>
                    <AttachmentDescription className="whitespace-normal">
                        {description} · {formatFileSize(file.size)}
                    </AttachmentDescription>
                </AttachmentContent>
                <AttachmentActions>
                    {url ? (
                        <AttachmentAction
                            id={baseId ? `${baseId}-preview` : undefined}
                            type="button"
                            size="icon-sm"
                            aria-label={`预览 ${file.name}`}
                            title={fileUploadText.preview}
                            onClick={() => setOpen(true)}
                        >
                            <EyeIcon aria-hidden="true" />
                        </AttachmentAction>
                    ) : null}
                    {onRemove ? (
                        <AttachmentAction
                            id={baseId ? `${baseId}-remove` : undefined}
                            type="button"
                            size="icon-sm"
                            variant="destructive"
                            disabled={disabled}
                            aria-label={`${fileUploadText.remove} ${file.name}`}
                            title={fileUploadText.remove}
                            onClick={() => onRemove(file)}
                        >
                            <Trash2Icon aria-hidden="true" />
                        </AttachmentAction>
                    ) : null}
                </AttachmentActions>
            </Attachment>
            {url ? (
                <Dialog open={open} onOpenChange={setOpen}>
                    <DialogContent
                        className="sm:max-w-4xl"
                        closeButtonId={
                            baseId ? `${baseId}-preview-close` : undefined
                        }
                    >
                        <DialogHeader>
                            <DialogTitle className="break-all">
                                {file.name}
                            </DialogTitle>
                            <DialogDescription>{description}</DialogDescription>
                        </DialogHeader>
                        {isImage ? (
                            // eslint-disable-next-line @next/next/no-img-element -- 待提交文件通过本地 Blob 预览。
                            <img
                                src={url}
                                alt={file.name}
                                className="max-h-[70dvh] w-full object-contain"
                            />
                        ) : (
                            <div className="space-y-2">
                                <a
                                    id={
                                        baseId
                                            ? `${baseId}-preview-open`
                                            : undefined
                                    }
                                    href={url}
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    className="text-sm text-primary underline underline-offset-2"
                                >
                                    在新窗口打开
                                </a>
                                <iframe
                                    src={url}
                                    title={file.name}
                                    sandbox=""
                                    className="h-[70dvh] w-full rounded-lg border"
                                />
                            </div>
                        )}
                    </DialogContent>
                </Dialog>
            ) : null}
        </>
    )
}

export function FileUploadSelection({
    files,
    idPrefix,
    multiple,
    disabled,
    readOnly = false,
    description = fileUploadText.selected,
    onRemove,
    fileKey = selectedFileKey,
}: {
    files: readonly File[]
    idPrefix?: string
    multiple: boolean
    disabled: boolean
    readOnly?: boolean
    description?: string
    onRemove?: (file: File) => void
    fileKey?: (file: File) => string
}) {
    return (
        <div className="space-y-1">
            {multiple ? (
                <p className="text-xs text-muted-foreground" role="status">
                    已选择 {files.length} 个文件
                </p>
            ) : null}
            <AttachmentGroup className="flex-col overflow-visible">
                {files.map((file) => (
                    <SelectedFile
                        key={fileKey(file)}
                        file={file}
                        baseId={
                            idPrefix
                                ? multiple
                                    ? `${idPrefix}-${toAutomationIdSegment(fileKey(file))}`
                                    : idPrefix
                                : undefined
                        }
                        disabled={disabled}
                        description={description}
                        onRemove={readOnly ? undefined : onRemove}
                    />
                ))}
            </AttachmentGroup>
        </div>
    )
}
