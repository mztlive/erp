"use client"

import { FileTextIcon, Trash2Icon } from "lucide-react"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatFileSize, selectedFileKey } from "@/lib/file-selection"
import { fileUploadText } from "@/lib/ui-text"

export function FileUploadSelection({
    files,
    idPrefix,
    multiple,
    disabled,
    onRemove,
    fileKey = selectedFileKey,
}: {
    files: readonly File[]
    idPrefix?: string
    multiple: boolean
    disabled: boolean
    onRemove?: (file: File) => void
    fileKey?: (file: File) => string
}) {
    return (
        <AttachmentGroup>
            {files.map((file) => (
                <Attachment key={fileKey(file)} aria-label={file.name}>
                    <AttachmentMedia>
                        <FileTextIcon aria-hidden="true" />
                    </AttachmentMedia>
                    <AttachmentContent>
                        <AttachmentTitle>{file.name}</AttachmentTitle>
                        <AttachmentDescription>
                            {fileUploadText.selected} ·{" "}
                            {formatFileSize(file.size)}
                        </AttachmentDescription>
                    </AttachmentContent>
                    {onRemove ? (
                        <AttachmentActions>
                            <AttachmentAction
                                id={
                                    idPrefix
                                        ? multiple
                                            ? `${idPrefix}-${toAutomationIdSegment(fileKey(file))}-remove`
                                            : `${idPrefix}-remove`
                                        : undefined
                                }
                                type="button"
                                variant="destructive"
                                disabled={disabled}
                                aria-label={`${fileUploadText.remove} ${file.name}`}
                                onClick={() => onRemove(file)}
                            >
                                <Trash2Icon aria-hidden="true" />
                            </AttachmentAction>
                        </AttachmentActions>
                    ) : null}
                </Attachment>
            ))}
        </AttachmentGroup>
    )
}
