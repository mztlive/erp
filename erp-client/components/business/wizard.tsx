"use client"

import * as React from "react"
import { CheckIcon } from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

type WizardStep = Readonly<{
  id: string
  label: string
}>

type WizardStepsProps = {
  steps: readonly WizardStep[]
  currentStepId: string
  className?: string
}

/**
 * 分步流程的步骤指示条：当前/已完成（已走过）/未完成三态，纯展示。
 * 步骤顺序即业务顺序，不支持跳过点击——前进/后退动作由页面底部操作条承载。
 */
function WizardSteps({ steps, currentStepId, className }: WizardStepsProps) {
  const currentIndex = steps.findIndex((step) => step.id === currentStepId)
  return (
    <ol
      className={cn("flex flex-wrap items-center gap-2", className)}
      aria-label="创建步骤"
    >
      {steps.map((step, index) => {
        const isCurrent = step.id === currentStepId
        const isDone = index < currentIndex
        return (
          <li key={step.id} className="flex items-center gap-2">
            {index > 0 ? (
              <span className="h-px w-5 bg-border" aria-hidden="true" />
            ) : null}
            <Badge
              variant={isCurrent ? "default" : isDone ? "success" : "neutral"}
              aria-current={isCurrent ? "step" : undefined}
            >
              {isDone ? (
                <CheckIcon aria-hidden="true" />
              ) : (
                <span className="num">{index + 1}</span>
              )}
              {step.label}
            </Badge>
          </li>
        )
      })}
    </ol>
  )
}

export { WizardSteps, type WizardStep, type WizardStepsProps }
