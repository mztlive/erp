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
  density = "default",
  className,
}: {
  onFilesSelected: (files: File[]) => void
  accept?: string
  multiple?: boolean
  disabled?: boolean
  label?: React.ReactNode
  description?: React.ReactNode
  density?: "default" | "compact"
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
        "flex min-w-0 items-center justify-center rounded-lg border border-dashed bg-surface-sunken transition-colors data-dragging:border-ring data-dragging:bg-accent",
        density === "compact"
          ? "flex-row gap-2 p-2 text-left"
          : "flex-col gap-3 p-6 text-center",
        disabled && "cursor-not-allowed opacity-50",
        className,
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
      <div
        className={cn(
          "flex shrink-0 items-center justify-center bg-muted text-foreground",
          density === "compact" ? "size-8 rounded-lg" : "size-10 rounded-xl",
        )}
      >
        <UploadCloudIcon
          className={density === "compact" ? "size-4" : "size-5"}
          aria-hidden="true"
        />
      </div>
      <div className={cn(density === "compact" && "min-w-0 flex-1")}>
        <div
          className={cn(
            "font-medium text-foreground",
            density === "compact" ? "text-xs" : "text-sm",
          )}
        >
          {label}
        </div>
        <div
          className={cn(
            "text-xs text-muted-foreground",
            density === "compact" ? "truncate" : "mt-1",
          )}
        >
          {description}
        </div>
      </div>
      <Button
        type="button"
        variant="outline"
        size={density === "compact" ? "xs" : "sm"}
        disabled={disabled}
        onClick={() => inputRef.current?.click()}
      >
        <FileUpIcon data-icon="inline-start" aria-hidden="true" />
        {density === "compact" ? "选择" : "选择文件"}
      </Button>
    </div>
  )
}

export { FileUpload }
