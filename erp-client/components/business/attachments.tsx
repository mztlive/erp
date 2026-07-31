"use client"

import * as React from "react"
import {
  EyeIcon,
  FileIcon,
  RefreshCwIcon,
  Trash2Icon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
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
import { FileUpload } from "@/components/ui/file-upload"
import { Spinner } from "@/components/ui/spinner"
import { cn } from "@/lib/utils"

type DocumentAttachmentState =
  | "idle"
  | "uploading"
  | "processing"
  | "error"
  | "done"

type DocumentAttachment = {
  id: string
  name: string
  description?: string
  state: DocumentAttachmentState
  required?: boolean
  errorMessage?: string
  onOpen?: () => void
  onRetry?: () => void
  onRemove?: () => void
}

interface DocumentAttachmentListProps
  extends Omit<React.ComponentPropsWithoutRef<"section">, "children" | "title"> {
  attachments: readonly DocumentAttachment[]
  onFilesSelected?: (files: File[]) => void
  accept?: string
  uploadDisabled?: boolean
  requiredMissing?: boolean
  title?: React.ReactNode
}

function DocumentAttachmentList({
  attachments,
  onFilesSelected,
  accept,
  uploadDisabled = false,
  requiredMissing = false,
  title = "附件",
  className,
  ...props
}: DocumentAttachmentListProps) {
  const failedCount = attachments.filter(
    (attachment) => attachment.state === "error"
  ).length
  const submissionBlocked = requiredMissing || failedCount > 0

  return (
    <section
      data-slot="document-attachment-list"
      className={cn("space-y-4", className)}
      {...props}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="text-sm font-medium">{title}</h3>
        <span className="num text-xs text-muted-foreground">
          {attachments.length} 个文件
        </span>
      </div>

      {attachments.length > 0 ? (
        <AttachmentGroup>
          {attachments.map((attachment) => (
            <Attachment
              key={attachment.id}
              state={attachment.state}
              aria-label={attachment.name}
            >
              <AttachmentMedia>
                {attachment.state === "uploading" ||
                attachment.state === "processing" ? (
                  <Spinner />
                ) : attachment.state === "error" ? (
                  <TriangleAlertIcon aria-hidden="true" />
                ) : (
                  <FileIcon aria-hidden="true" />
                )}
              </AttachmentMedia>
              <AttachmentContent>
                <AttachmentTitle>{attachment.name}</AttachmentTitle>
                <AttachmentDescription>
                  {attachment.errorMessage ??
                    attachment.description ??
                    (attachment.required ? "必需附件" : "补充附件")}
                </AttachmentDescription>
              </AttachmentContent>
              <AttachmentActions>
                {attachment.onOpen && attachment.state === "done" ? (
                  <AttachmentAction
                    type="button"
                    onClick={attachment.onOpen}
                    aria-label={`查看 ${attachment.name}`}
                  >
                    <EyeIcon aria-hidden="true" />
                  </AttachmentAction>
                ) : null}
                {attachment.onRetry && attachment.state === "error" ? (
                  <AttachmentAction
                    type="button"
                    onClick={attachment.onRetry}
                    aria-label={`重试上传 ${attachment.name}`}
                  >
                    <RefreshCwIcon aria-hidden="true" />
                  </AttachmentAction>
                ) : null}
                {attachment.onRemove ? (
                  <AttachmentAction
                    type="button"
                    variant="destructive"
                    onClick={attachment.onRemove}
                    aria-label={`移除 ${attachment.name}`}
                  >
                    <Trash2Icon aria-hidden="true" />
                  </AttachmentAction>
                ) : null}
              </AttachmentActions>
            </Attachment>
          ))}
        </AttachmentGroup>
      ) : (
        <p className="text-sm text-muted-foreground">尚未上传附件</p>
      )}

      {onFilesSelected ? (
        <FileUpload
          onFilesSelected={onFilesSelected}
          accept={accept}
          disabled={uploadDisabled}
        />
      ) : null}

      {submissionBlocked ? (
        <Alert variant="warning">
          <TriangleAlertIcon aria-hidden="true" />
          <AlertTitle>附件尚未满足提交条件</AlertTitle>
          <AlertDescription>
            {requiredMissing ? "仍缺少必需附件。" : null}
            {failedCount > 0 ? `有 ${failedCount} 个文件上传失败。` : null}
          </AlertDescription>
        </Alert>
      ) : null}
    </section>
  )
}

export {
  DocumentAttachmentList,
  type DocumentAttachment,
  type DocumentAttachmentListProps,
  type DocumentAttachmentState,
}
