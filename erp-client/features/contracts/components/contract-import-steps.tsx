"use client"

import { CheckIcon } from "lucide-react"
import { cn } from "@/lib/utils"
import {
    IMPORT_REVIEW_STEPS,
    type ImportReviewStep,
} from "../lib/import-review"

export function ContractImportSteps({
    step,
    validSteps,
    disabled,
    onChange,
}: {
    step: ImportReviewStep
    validSteps: boolean[]
    disabled: boolean
    onChange: (step: ImportReviewStep) => void
}) {
    return (
        <nav
            aria-label="合同核对步骤"
            className="shrink-0 border-b border-border"
        >
            <ol className="flex">
                {IMPORT_REVIEW_STEPS.map((label, index) => (
                    <li key={label} className="min-w-0 flex-1">
                        <button
                            id={`contract-import-step-${index + 1}`}
                            type="button"
                            aria-current={step === index ? "step" : undefined}
                            aria-controls="contract-import-review-form"
                            disabled={
                                disabled ||
                                (index > step &&
                                    !validSteps.slice(0, index).every(Boolean))
                            }
                            onClick={() => onChange(index as ImportReviewStep)}
                            className={cn(
                                "mx-auto flex w-fit max-w-full items-center justify-center gap-2 border-b-2 border-transparent px-2 pb-4 sm:px-6 text-sm text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-70 sm:gap-3",
                                step === index &&
                                    "border-foreground font-medium text-foreground",
                            )}
                        >
                            <span
                                className={cn(
                                    "flex size-8 shrink-0 items-center justify-center rounded-full bg-muted text-sm",
                                    step === index &&
                                        "bg-primary text-primary-foreground",
                                )}
                            >
                                {index < step && validSteps[index] ? (
                                    <CheckIcon
                                        className="size-4"
                                        aria-hidden="true"
                                    />
                                ) : (
                                    index + 1
                                )}
                            </span>
                            {label}
                        </button>
                    </li>
                ))}
            </ol>
        </nav>
    )
}
