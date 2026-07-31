"use client"

import * as React from "react"
import { FileUpIcon, UploadCloudIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

function FileUpload({
  onFilesSelected,
  accept,
  multiple = true,
  disabled = false,
  label = "上传文件",
  description = "点击选择文件，或将文件拖到此处",
  className,
}: {
  onFilesSelected: (files: File[]) => void
  accept?: string
  multiple?: boolean
  disabled?: boolean
  label?: React.ReactNode
  description?: React.ReactNode
  className?: string
}) {
  const inputRef = React.useRef<HTMLInputElement>(null)
  const [dragging, setDragging] = React.useState(false)

  const selectFiles = (files: FileList | null) => {
    if (disabled || !files?.length) return
    onFilesSelected(Array.from(files))
  }

  return (
    <div
      data-slot="file-upload"
      data-dragging={dragging || undefined}
      className={cn(
        "flex min-w-0 flex-col items-center justify-center gap-3 rounded-lg border border-dashed bg-surface-sunken p-6 text-center transition-colors data-dragging:border-ring data-dragging:bg-accent",
        disabled && "cursor-not-allowed opacity-50",
        className
      )}
      onDragEnter={(event) => {
        event.preventDefault()
        if (!disabled) setDragging(true)
      }}
      onDragOver={(event) => event.preventDefault()}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setDragging(false)
        }
      }}
      onDrop={(event) => {
        event.preventDefault()
        setDragging(false)
        selectFiles(event.dataTransfer.files)
      }}
    >
      <input
        ref={inputRef}
        className="sr-only"
        type="file"
        accept={accept}
        multiple={multiple}
        disabled={disabled}
        onChange={(event) => {
          selectFiles(event.target.files)
          event.target.value = ""
        }}
        aria-label={typeof label === "string" ? label : "选择上传文件"}
      />
      <div className="flex size-10 items-center justify-center rounded-xl bg-muted text-foreground">
        <UploadCloudIcon className="size-5" aria-hidden="true" />
      </div>
      <div>
        <div className="text-sm font-medium text-foreground">{label}</div>
        <div className="mt-1 text-xs text-muted-foreground">{description}</div>
      </div>
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={disabled}
        onClick={() => inputRef.current?.click()}
      >
        <FileUpIcon data-icon="inline-start" aria-hidden="true" />
        选择文件
      </Button>
    </div>
  )
}

export { FileUpload }
